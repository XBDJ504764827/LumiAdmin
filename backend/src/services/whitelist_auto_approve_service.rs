//! 白名单低风险自动通过服务
//!
//! 玩家提交白名单申请后，默认等待管理员审核。若申请状态为 `pending` 且
//! 风险评分为 `Allow`（无本地封禁、无全球封禁、无同 IP 高风险关联），
//! 在申请提交满 `whitelist_auto_approve_config.hours`（默认 3 小时）后，
//! 仍无人审核则自动通过。
//!
//! 自动通过规则：
//! - 仅 `RiskAction::Allow`（低风险）参与自动通过；
//! - `Warn`（历史风险）/ `RequireForce` / `Deny`（中高风险，含自身全球封禁）
//!   保持 `pending`，继续等待管理员审核；
//! - 若管理员在等待窗口内已审批（status 不再是 pending），自动通过会因
//!   `WHERE status='pending'` 原子条件更新命中 0 行而跳过，不会覆盖人工结果；
//! - 自动通过后主动刷新白名单缓存（`WhitelistCache`），玩家可立即进服，
//!   无需等待缓存刷新周期；
//! - 自动通过写入审计日志（operator = 系统，source = system），可追溯；
//! - 开关与等待时长存储在数据库（`whitelist_auto_approve_config`），
//!   可通过网站开关实时控制，无需重启服务。

use crate::{
    db::Database,
    services::{
        access_cache::WhitelistCache,
        audit_service, observability_service,
        player_risk_service::{self, RiskAction},
        whitelist_service::{self, ApproveWhitelistInput},
    },
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

/// 自动通过配置（单行表，由网站开关控制）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AutoApproveConfig {
    pub enabled: bool,
    pub hours: i32,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// 一轮自动通过的结果统计
#[derive(Debug, Default, Serialize)]
pub struct AutoApproveSummary {
    pub scanned: usize,
    pub approved: usize,
    pub skipped_high_risk: usize,
    pub already_processed: usize,
}

/// 读取自动通过配置（表为空时使用默认值：开启 + 3 小时）
pub async fn load_config(db: &Database) -> anyhow::Result<AutoApproveConfig> {
    let row: Option<AutoApproveConfig> = sqlx::query_as(
        r#"SELECT enabled, hours, updated_by, updated_at
           FROM whitelist_auto_approve_config
           WHERE id = true"#,
    )
    .fetch_optional(&db.pool)
    .await
    .context("读取白名单自动通过配置失败")?;

    Ok(row.unwrap_or(AutoApproveConfig {
        enabled: true,
        hours: 3,
        updated_by: None,
        updated_at: Utc::now(),
    }))
}

/// 更新自动通过配置（网站开关）
pub async fn update_config(
    db: &Database,
    enabled: bool,
    hours: i32,
    updated_by: &str,
) -> anyhow::Result<AutoApproveConfig> {
    let hours = hours.clamp(1, 72);
    sqlx::query(
        r#"INSERT INTO whitelist_auto_approve_config (id, enabled, hours, updated_by, updated_at)
           VALUES (true, $1, $2, $3, now())
           ON CONFLICT (id) DO UPDATE
           SET enabled = EXCLUDED.enabled,
               hours = EXCLUDED.hours,
               updated_by = EXCLUDED.updated_by,
               updated_at = now()"#,
    )
    .bind(enabled)
    .bind(hours)
    .bind(updated_by.trim())
    .execute(&db.pool)
    .await
    .context("更新白名单自动通过配置失败")?;

    load_config(db).await
}

/// 启动白名单低风险自动通过循环。
///
/// 每 `interval_secs` 扫描一次：
/// 1. 读取开关配置，关闭时直接跳过（任务仍登记，便于运维观察）；
/// 2. 取出「pending 且 applied_at 超时」的申请；
/// 3. 逐条构建风险评分，仅 `Allow` 自动通过，其余保持 pending。
pub fn start_auto_approve_loop(
    db: Database,
    whitelist_cache: Arc<WhitelistCache>,
    config: crate::config::Config,
    interval_secs: u64,
) {
    observability_service::register_task(
        "whitelist_auto_approve",
        "白名单低风险自动通过",
        "白名单",
        Some(interval_secs),
        true,
    );
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(30)));
        loop {
            interval.tick().await;
            match observability_service::observe_task(
                "whitelist_auto_approve",
                process_auto_approve(&db, &whitelist_cache, &config),
                |summary| {
                    format!(
                        "扫描 {} 条（自动通过 {}，高风险跳过 {}，已处理 {}）",
                        summary.scanned,
                        summary.approved,
                        summary.skipped_high_risk,
                        summary.already_processed
                    )
                },
            )
            .await
            {
                Ok(summary) => {
                    if summary.approved > 0 {
                        tracing::info!(
                            scanned = summary.scanned,
                            approved = summary.approved,
                            skipped_high_risk = summary.skipped_high_risk,
                            already_processed = summary.already_processed,
                            "白名单低风险自动通过完成"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "白名单低风险自动通过执行失败");
                }
            }
        }
    });
}

/// 待自动通过的候选行
#[derive(sqlx::FromRow)]
struct PendingCandidate {
    id: Uuid,
    steamid64: String,
}

/// 执行一轮自动通过扫描。
pub async fn process_auto_approve(
    db: &Database,
    whitelist_cache: &WhitelistCache,
    app_config: &crate::config::Config,
) -> anyhow::Result<AutoApproveSummary> {
    let auto_config = load_config(db).await?;
    let mut summary = AutoApproveSummary::default();

    // 开关关闭：不处理任何申请
    if !auto_config.enabled {
        return Ok(summary);
    }

    // 取出「pending 且等待时长已满」的申请（按申请时间升序）
    let candidates: Vec<PendingCandidate> = sqlx::query_as(
        r#"
        SELECT id, steamid64
        FROM whitelist_requests
        WHERE status = 'pending'
          AND applied_at <= now() - make_interval(hours => $1)
        ORDER BY applied_at ASC
        LIMIT 200
        "#,
    )
    .bind(auto_config.hours)
    .fetch_all(&db.pool)
    .await
    .context("查询待自动通过白名单失败")?;
    summary.scanned = candidates.len();

    let mut approved_any = false;
    for candidate in candidates {
        // 构建风险评分：仅低风险（Allow）自动通过
        let risk_profile =
            match player_risk_service::build_player_risk_profile(db, &candidate.steamid64).await {
                Ok(profile) => profile,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        steamid64 = %candidate.steamid64,
                        "白名单自动通过：风险评分构建失败，跳过"
                    );
                    continue;
                }
            };

        if risk_profile.action != RiskAction::Allow {
            summary.skipped_high_risk += 1;
            continue;
        }

        match auto_approve_one(db, &candidate, auto_config.hours).await {
            Ok(Some(item)) => {
                summary.approved += 1;
                approved_any = true;
                // 上报 LumiBot 自动通过事件（info 级，仅记录，不打扰管理员）
                if let Err(error) =
                    crate::services::lumi_bot_service::report_whitelist_auto_approved(
                        db,
                        app_config,
                        &item,
                        auto_config.hours as i64,
                    )
                    .await
                {
                    tracing::warn!(%error, whitelist_id = %candidate.id, "白名单自动通过事件上报失败");
                }
            }
            Ok(None) => summary.already_processed += 1,
            Err(error) => {
                tracing::warn!(%error, whitelist_id = %candidate.id, "白名单自动通过失败");
            }
        }
    }

    // 有通过时主动刷新白名单缓存，玩家可立即进服
    if approved_any {
        if let Err(error) = whitelist_cache.refresh(db).await {
            tracing::warn!(%error, "白名单自动通过后刷新缓存失败");
        }
    }

    Ok(summary)
}

/// 单条申请自动通过：事务内原子更新 + 审计日志。
/// 返回 Some(item) = 自动通过成功；None = 已被管理员处理（无可执行更新）。
async fn auto_approve_one(
    db: &Database,
    candidate: &PendingCandidate,
    hours: i32,
) -> anyhow::Result<Option<whitelist_service::WhitelistItem>> {
    let mut tx = db.pool.begin().await?;

    let item = whitelist_service::approve_whitelist_tx(
        &mut tx,
        candidate.id,
        ApproveWhitelistInput {
            operator_name: "系统",
            reason: Some(&format!("低风险玩家，申请满 {hours} 小时无人审核自动通过")),
            force: true,
            via: "auto",
        },
    )
    .await;

    let item = match item {
        Ok(item) => item,
        // 已被管理员审批 / 状态已变化 → 正常跳过，不算错误
        Err(_) => {
            tx.rollback().await?;
            return Ok(None);
        }
    };

    audit_service::write_audit_log_in_transaction(
        &mut tx,
        audit_service::AuditLogInput {
            operation: "whitelist_auto_approve".to_string(),
            target: item.steamid64.clone(),
            target_type: "whitelist".to_string(),
            player_name: Some(item.nickname.clone()),
            reason: item.approval_reason.clone(),
            duration_minutes: None,
            operator_id: None,
            operator_name: "系统".to_string(),
            operator_steamid: None,
            source: "system".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!(
                "低风险白名单申请自动通过（等待 {} 小时无人审核），ID: {}",
                hours, item.id
            )),
            idempotency_key: None,
        },
        None,
        Some(serde_json::json!({
            "action": "auto_approve_whitelist",
            "whitelist_id": item.id,
            "steamid64": item.steamid64,
            "nickname": item.nickname,
            "status": item.status,
            "approved_at": item.approved_at,
            "approved_by": item.approved_by,
            "approval_reason": item.approval_reason,
            "auto_approve_hours": hours,
        })),
    )
    .await?;

    tx.commit().await?;

    tracing::info!(
        whitelist_id = %item.id,
        steamid64 = %item.steamid64,
        "白名单低风险申请已自动通过"
    );
    Ok(Some(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database, test_util};
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = test_util::schema_url(&base_url, &schema);
        test_util::create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        test_util::drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    async fn insert_pending(
        db: &Database,
        steamid64: &str,
        nickname: &str,
        applied_at: chrono::DateTime<Utc>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, status,
                applied_at, source, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, 'public', now())"#,
        )
        .bind(id)
        .bind(steamid64)
        .bind(steamid64)
        .bind(format!("STEAM_0:0:{}", &steamid64[..6]))
        .bind(format!("[U:1:{}]", &steamid64[..6]))
        .bind(format!("https://steamcommunity.com/profiles/{steamid64}"))
        .bind(nickname)
        .bind(applied_at)
        .execute(&db.pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn low_risk_pending_older_than_hours_is_auto_approved() {
        with_test_db(async |db| {
            let id = insert_pending(
                &db,
                "76561198000000001",
                "低风险玩家",
                Utc::now() - Duration::hours(4),
            )
            .await;

            let cache = WhitelistCache::new();
            let summary = process_auto_approve(&db, &cache, &Config::from_env()).await?;

            assert_eq!(summary.scanned, 1);
            assert_eq!(summary.approved, 1);
            assert_eq!(summary.skipped_high_risk, 0);

            let (status, approved_by, approval_reason): (String, Option<String>, Option<String>) =
                sqlx::query_as(
                    "SELECT status, approved_by, approval_reason FROM whitelist_requests WHERE id = $1",
                )
                .bind(id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(status, "approved");
            assert_eq!(approved_by.as_deref(), Some("系统"));
            assert!(approval_reason.as_deref().unwrap_or("").contains("自动通过"));
            assert!(cache.contains("76561198000000001").await);

            // 审计日志应存在
            let audit_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE operation = 'whitelist_auto_approve'",
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(audit_count, 1);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn candidate_not_yet_ready_is_not_scanned() {
        with_test_db(async |db| {
            insert_pending(
                &db,
                "76561198000000002",
                "未满时长",
                Utc::now() - Duration::minutes(30),
            )
            .await;

            let cache = WhitelistCache::new();
            let summary = process_auto_approve(&db, &cache, &Config::from_env()).await?;
            assert_eq!(summary.scanned, 0);
            assert_eq!(summary.approved, 0);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn disabled_config_skips_processing() {
        with_test_db(async |db| {
            insert_pending(
                &db,
                "76561198000000003",
                "开关关闭",
                Utc::now() - Duration::hours(5),
            )
            .await;
            update_config(&db, false, 3, "测试管理员").await?;

            let cache = WhitelistCache::new();
            let summary = process_auto_approve(&db, &cache, &Config::from_env()).await?;
            assert_eq!(summary.scanned, 0);
            assert_eq!(summary.approved, 0);

            let status: String =
                sqlx::query_scalar("SELECT status FROM whitelist_requests WHERE steamid64 = $1")
                    .bind("76561198000000003")
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "pending");
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn already_approved_by_admin_is_not_overwritten() {
        with_test_db(async |db| {
            let id = insert_pending(
                &db,
                "76561198000000004",
                "已人工通过",
                Utc::now() - Duration::hours(4),
            )
            .await;
            // 管理员在自动通过前已审批
            sqlx::query(
                r#"UPDATE whitelist_requests
                   SET status = 'approved', approved_by = '管理员A', approved_at = now()
                   WHERE id = $1"#,
            )
            .bind(id)
            .execute(&db.pool)
            .await?;

            let cache = WhitelistCache::new();
            let summary = process_auto_approve(&db, &cache, &Config::from_env()).await?;
            // 已被管理员审批的记录状态不再为 pending，不会进入自动通过扫描
            assert_eq!(summary.scanned, 0);
            assert_eq!(summary.approved, 0);

            let (status, approved_by): (String, Option<String>) =
                sqlx::query_as("SELECT status, approved_by FROM whitelist_requests WHERE id = $1")
                    .bind(id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "approved");
            assert_eq!(approved_by.as_deref(), Some("管理员A"));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn auto_approve_one_returns_none_when_status_not_pending() {
        with_test_db(async |db| {
            let id = insert_pending(
                &db,
                "76561198000000005",
                "原子更新保护",
                Utc::now() - Duration::hours(4),
            )
            .await;
            // 模拟管理员在自动通过事务开始前已审批
            sqlx::query(
                r#"UPDATE whitelist_requests
                   SET status = 'approved', approved_by = '管理员B', approved_at = now()
                   WHERE id = $1"#,
            )
            .bind(id)
            .execute(&db.pool)
            .await?;

            let candidate = PendingCandidate {
                id,
                steamid64: "76561198000000005".to_string(),
            };
            let result = auto_approve_one(&db, &candidate, 3).await?;
            assert!(
                result.is_none(),
                "非 pending 状态应返回 None（不覆盖人工结果）"
            );

            let (status, approved_by): (String, Option<String>) =
                sqlx::query_as("SELECT status, approved_by FROM whitelist_requests WHERE id = $1")
                    .bind(id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "approved");
            assert_eq!(approved_by.as_deref(), Some("管理员B"));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn update_config_clamps_hours_and_persists() {
        with_test_db(async |db| {
            let config = update_config(&db, true, 99, "测试管理员").await?;
            assert_eq!(config.hours, 72); // 上限 72 小时
            assert!(config.enabled);
            assert_eq!(config.updated_by.as_deref(), Some("测试管理员"));

            let loaded = load_config(&db).await?;
            assert_eq!(loaded.hours, 72);
            assert!(loaded.enabled);
            Ok(())
        })
        .await;
    }
}
