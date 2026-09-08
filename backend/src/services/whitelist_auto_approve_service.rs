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
//!   保持 `pending`，继续等待管理员审核，并通过 QQ 发送提醒（`warning`
//!   级事件，见 `lumi_bot_service::report_whitelist_pending_review`），
//!   绝不自动通过；
//! - 自动通过前还有最后一道闸：直接以最新封禁数据复核，任何存在未解封
//!   本地封禁或未过期全球封禁的玩家都绝不自动通过（防风险数据滞后误判）
//!   ——命中时同样转为人工审核提醒；
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
        audit_service, lumi_bot_service, observability_service,
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
    /// 本轮为中/高风险申请补发的 QQ 人工审核提醒条数（幂等去重后）
    pub reminded: usize,
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
    super::task_runtime::spawn_persistent("whitelist_auto_approve", move || {
        let db = db.clone();
        let whitelist_cache = whitelist_cache.clone();
        let config = config.clone();
        async move {
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
                        if summary.approved > 0 || summary.reminded > 0 {
                            tracing::info!(
                                scanned = summary.scanned,
                                approved = summary.approved,
                                skipped_high_risk = summary.skipped_high_risk,
                                already_processed = summary.already_processed,
                                reminded = summary.reminded,
                                "白名单低风险自动通过完成"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "白名单低风险自动通过执行失败");
                    }
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
            // 中/高风险绝不自动通过：确保管理员通过 QQ 收到人工审核提醒
            //（幂等去重：提交时已以 warning 级推送过的不重复提醒）
            summary.reminded += {
                let risk_label = risk_action_label(risk_profile.action.clone());
                let reasons_text = risk_profile
                    .reasons
                    .iter()
                    .take(3)
                    .map(|reason| reason.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；");
                let risk_reason = if reasons_text.is_empty() {
                    None
                } else {
                    Some(reasons_text.as_str())
                };
                remind_manual_review(
                    db,
                    app_config,
                    &candidate,
                    auto_config.hours,
                    risk_label,
                    risk_reason,
                )
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        %error,
                        whitelist_id = %candidate.id,
                        "白名单中/高风险人工审核提醒入队失败"
                    );
                    0
                })
            };
            continue;
        }

        // 最后一道闸：真正自动通过前，直接以最新封禁数据复核，彻底堵死
        // 「风险评分时数据尚未同步 → 判为低风险 → 自动通过」的窗口。
        // fail-closed：复核失败时不自动通过，等待下一轮。
        match detect_unresolved_ban(db, &candidate.steamid64).await {
            Ok(Some(ban_reason)) => {
                tracing::warn!(
                    whitelist_id = %candidate.id,
                    steamid64 = %candidate.steamid64,
                    %ban_reason,
                    "白名单自动通过终检发现未解封封禁，拒绝自动通过并转人工审核"
                );
                summary.skipped_high_risk += 1;
                summary.reminded += remind_manual_review(
                    db,
                    app_config,
                    &candidate,
                    auto_config.hours,
                    "高风险",
                    Some(ban_reason.as_str()),
                )
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        %error,
                        whitelist_id = %candidate.id,
                        "白名单中/高风险人工审核提醒入队失败"
                    );
                    0
                });
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    %error,
                    whitelist_id = %candidate.id,
                    "白名单自动通过终检失败，本轮跳过（fail-closed）"
                );
                summary.skipped_high_risk += 1;
                continue;
            }
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

/// 风险动作的中文标签（与 QQ 通知文案保持一致）
fn risk_action_label(action: RiskAction) -> &'static str {
    match action {
        RiskAction::Allow => "低风险",
        RiskAction::Warn => "历史风险",
        RiskAction::RequireForce | RiskAction::Deny => "高风险",
    }
}

/// 为超时未审核的中/高风险申请发送 QQ 人工审核提醒（幂等）。
///
/// 去重规则（命中任一即跳过）：
/// 1. `review_notified_at` 已标记（提交时 warning 级提醒过，或此前已补发过提醒）；
/// 2. 队列中已存在该申请的 warning 级 `WHITELIST_REQUEST_CREATED` 事件
///    （历史数据：提交时已提醒过但未回填标记）。
///
/// 返回本轮实际发送的提醒条数（0 或 1）。
async fn remind_manual_review(
    db: &Database,
    config: &crate::config::Config,
    candidate: &PendingCandidate,
    hours: i32,
    risk_label: &str,
    risk_reason: Option<&str>,
) -> anyhow::Result<usize> {
    // 1) 已提醒过 → 幂等跳过
    let notified: (bool,) = sqlx::query_as(
        "SELECT review_notified_at IS NOT NULL FROM whitelist_requests WHERE id = $1",
    )
    .bind(candidate.id)
    .fetch_one(&db.pool)
    .await
    .context("读取白名单审核提醒标记失败")?;
    if notified.0 {
        return Ok(0);
    }

    // 2) 队列里已有 warning 级申请事件（提交时已提醒过）→ 跳过
    let already: (bool,) = sqlx::query_as(
        r#"SELECT EXISTS(
               SELECT 1 FROM lumi_bot_event_queue
               WHERE event_type = 'WHITELIST_REQUEST_CREATED'
                 AND level = 'warning'
                 AND data->>'whitelist_id' = $1
                 AND status IN ('pending', 'sent')
           )"#,
    )
    .bind(candidate.id.to_string())
    .fetch_one(&db.pool)
    .await
    .context("查询白名单申请事件队列失败")?;
    if already.0 {
        return Ok(0);
    }

    // 读取申请基础信息，构造提醒事件
    #[derive(sqlx::FromRow)]
    struct ReminderRow {
        id: Uuid,
        steamid64: String,
        steamid: Option<String>,
        steamid3: Option<String>,
        profile_url: Option<String>,
        nickname: String,
        steam_persona_name: Option<String>,
        contact: Option<String>,
        applied_at: chrono::DateTime<Utc>,
    }
    let row: ReminderRow = sqlx::query_as(
        r#"SELECT id, steamid64, steamid, steamid3, profile_url, nickname,
                  steam_persona_name, contact, applied_at
           FROM whitelist_requests WHERE id = $1"#,
    )
    .bind(candidate.id)
    .fetch_one(&db.pool)
    .await
    .context("读取白名单申请信息失败")?;

    let item = whitelist_service::WhitelistItem {
        id: row.id,
        steamid64: row.steamid64,
        steamid: row.steamid,
        steamid3: row.steamid3,
        profile_url: row.profile_url,
        nickname: row.nickname,
        steam_persona_name: row.steam_persona_name,
        contact: row.contact,
        status: "pending".to_string(),
        applied_at: row.applied_at.to_rfc3339(),
        approved_at: None,
        approved_by: None,
        approval_reason: None,
        rejected_at: None,
        rejected_by: None,
        rejection_reason: None,
        risk_profile: None,
    };

    lumi_bot_service::report_whitelist_pending_review(
        db,
        config,
        &item,
        hours as i64,
        risk_label,
        risk_reason,
    )
    .await?;
    Ok(1)
}

/// 最后一道闸：自动通过前直接以最新封禁数据复核。
///
/// 与 `build_player_risk_profile` 相互独立：即使风险评分时数据尚未同步
/// （全球封禁同步滞后 / 封禁缓存未刷新）导致误判为低风险，只要玩家存在
/// 未解封本地封禁或未过期全球封禁，这里都会拦截，绝不自动通过。
/// 返回 `Some(原因描述)` 表示存在未解封封禁，应转人工审核。
async fn detect_unresolved_ban(db: &Database, steamid64: &str) -> anyhow::Result<Option<String>> {
    let local_active: (bool, Option<String>) = sqlx::query_as(
        r#"SELECT EXISTS(
                 SELECT 1 FROM ban_records
                 WHERE steam_id = $1
                   AND status = 'active'
                   AND (expires_at IS NULL OR expires_at > now())
                 LIMIT 1
               ),
               (SELECT reason FROM ban_records
                WHERE steam_id = $1
                  AND status = 'active'
                  AND (expires_at IS NULL OR expires_at > now())
                ORDER BY created_at DESC LIMIT 1)"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await?;
    if local_active.0 {
        let reason = local_active
            .1
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or_else(|| "未填写".to_string());
        return Ok(Some(format!("存在未解封本地封禁（原因：{reason}）")));
    }

    let global_active: (bool, Option<String>) = sqlx::query_as(
        r#"SELECT EXISTS(
                 SELECT 1 FROM global_bans
                 WHERE steam_id64 = $1 AND is_expired = false AND manual_unbanned = false
                 LIMIT 1
               ),
               (SELECT ban_type FROM global_bans
                WHERE steam_id64 = $1 AND is_expired = false AND manual_unbanned = false
                ORDER BY COALESCE(created_on, updated_on) DESC, synced_at DESC LIMIT 1)"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await?;
    if global_active.0 {
        let ban_type = global_active
            .1
            .filter(|ban_type| !ban_type.trim().is_empty())
            .unwrap_or_else(|| "未知类型".to_string());
        return Ok(Some(format!("存在未过期全球封禁（类型：{ban_type}）")));
    }

    Ok(None)
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

    // ===== 中/高风险不自动通过 + QQ 人工审核提醒 =====

    async fn insert_global_ban(db: &Database, steamid64: &str, ban_type: &str) {
        sqlx::query(
            r#"INSERT INTO global_bans (id, kzt_ban_id, steam_id64, player_name, ban_type,
                                        is_expired, manual_unbanned)
               VALUES ($1, $2, $3, '被封禁玩家', $4, false, false)"#,
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4().as_u128() as i32)
        .bind(steamid64)
        .bind(ban_type)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    async fn queue_event_count(db: &Database, whitelist_id: &Uuid, level: &str) -> i64 {
        let (count,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM lumi_bot_event_queue
               WHERE event_type = 'WHITELIST_REQUEST_CREATED'
                 AND level = $1
                 AND data->>'whitelist_id' = $2"#,
        )
        .bind(level)
        .bind(whitelist_id.to_string())
        .fetch_one(&db.pool)
        .await
        .unwrap();
        count
    }

    #[tokio::test]
    async fn high_risk_pending_is_not_auto_approved_and_gets_qq_reminder() {
        with_test_db(async |db| {
            let steamid64 = "76561198000000010";
            let id = insert_pending(
                &db,
                steamid64,
                "高风险玩家",
                Utc::now() - Duration::hours(4),
            )
            .await;
            insert_global_ban(&db, steamid64, "bhop_hack").await;

            let config = Config::from_env();
            let cache = std::sync::Arc::new(WhitelistCache::new());
            let summary = process_auto_approve(&db, &cache, &config).await?;

            // 高风险：绝不自动通过，且发送人工审核提醒
            assert_eq!(summary.approved, 0);
            assert_eq!(summary.skipped_high_risk, 1);
            assert_eq!(summary.reminded, 1);

            let (status, approved_via): (String, Option<String>) =
                sqlx::query_as("SELECT status, approved_via FROM whitelist_requests WHERE id = $1")
                    .bind(id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "pending", "高风险玩家不应被自动通过");
            assert_eq!(approved_via, None);

            // QQ 提醒：warning 级事件入队 + review_notified_at 标记
            assert_eq!(queue_event_count(&db, &id, "warning").await, 1);
            let notified: (bool,) = sqlx::query_as(
                "SELECT review_notified_at IS NOT NULL FROM whitelist_requests WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
            assert!(notified.0, "应标记已完成人工审核提醒");

            // 幂等：下一轮不重复提醒
            let summary = process_auto_approve(&db, &cache, &config).await?;
            assert_eq!(summary.reminded, 0);
            assert_eq!(queue_event_count(&db, &id, "warning").await, 1);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn medium_risk_pending_is_not_auto_approved_and_gets_qq_reminder() {
        with_test_db(async |db| {
            let steamid64 = "76561198000000020";
            let linked = "76561198000000099";
            let id = insert_pending(
                &db,
                steamid64,
                "中风险玩家",
                Utc::now() - Duration::hours(4),
            )
            .await;

            // 中风险场景：同 IP 关联账号有旧的历史负面记录（白名单申请被拒）
            // → linked_ip_negative_history（历史关联，Warning 级）→ Warn
            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '测试社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password,
                                        report_token, status, players)
                   VALUES ($1, $2, '测试服', '127.0.0.1', 25575, 'secret', 'token', 'online', $3)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;
            let old_seen = Utc::now() - Duration::days(120);
            for (sid, name) in [(steamid64, "中风险玩家"), (linked, "关联账号")] {
                sqlx::query(
                    r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip,
                                                          ping, server_port, reported_at)
                       VALUES ($1, $2, $3, '203.0.113.50', 30, 25575, $4)"#,
                )
                .bind(server_id)
                .bind(name)
                .bind(sid)
                .bind(old_seen)
                .execute(&db.pool)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO whitelist_requests (id, steam_id, steamid64, steamid, steamid3,
                                                   profile_url, nickname, status,
                                                   rejected_at, rejected_by, rejection_reason,
                                                   applied_at, source, updated_at)
                   VALUES ($1, $2, $3, $4, $5, $6, '关联账号', 'rejected', now(), '管理员A',
                           '风险玩家', $7, 'public', now())"#,
            )
            .bind(Uuid::new_v4())
            .bind(linked)
            .bind(linked)
            .bind(format!("STEAM_0:0:{}", &linked[6..8]))
            .bind(format!("[U:1:{}]", &linked[6..8]))
            .bind(format!("https://steamcommunity.com/profiles/{linked}"))
            .bind(Utc::now())
            .execute(&db.pool)
            .await?;

            let config = Config::from_env();
            let cache = std::sync::Arc::new(WhitelistCache::new());
            let summary = process_auto_approve(&db, &cache, &config).await?;

            assert_eq!(summary.approved, 0, "中风险玩家不应被自动通过");
            assert_eq!(summary.skipped_high_risk, 1);
            assert_eq!(summary.reminded, 1, "应为中风险玩家发送 QQ 人工审核提醒");

            let (status,): (String,) =
                sqlx::query_as("SELECT status FROM whitelist_requests WHERE id = $1")
                    .bind(id)
                    .fetch_one(&db.pool)
                    .await?;
            assert_eq!(status, "pending");
            assert_eq!(queue_event_count(&db, &id, "warning").await, 1);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn reminder_skipped_when_submit_already_notified_as_warning() {
        with_test_db(async |db| {
            let steamid64 = "76561198000000030";
            let id = insert_pending(
                &db,
                steamid64,
                "高风险玩家",
                Utc::now() - Duration::hours(4),
            )
            .await;
            insert_global_ban(&db, steamid64, "rage_hack").await;

            // 模拟提交时已以 warning 级推送过申请事件（历史数据、未回填标记）
            sqlx::query(
                r#"INSERT INTO lumi_bot_event_queue (id, event_type, level, title, message, data)
                   VALUES ($1, 'WHITELIST_REQUEST_CREATED', 'warning', '新白名单申请',
                           '测试', jsonb_build_object('whitelist_id', $2::text))"#,
            )
            .bind(Uuid::new_v4())
            .bind(id.to_string())
            .execute(&db.pool)
            .await?;

            let config = Config::from_env();
            let cache = std::sync::Arc::new(WhitelistCache::new());
            let summary = process_auto_approve(&db, &cache, &config).await?;

            assert_eq!(summary.approved, 0);
            assert_eq!(summary.reminded, 0, "提交时已提醒过的不重复提醒");
            assert_eq!(queue_event_count(&db, &id, "warning").await, 1);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn detect_unresolved_ban_blocks_active_and_expired_bans() {
        with_test_db(async |db| {
            let steamid64 = "76561198000000040";

            // 无封禁 → None
            assert_eq!(detect_unresolved_ban(&db, steamid64).await?, None);

            // 未解封本地封禁 → Some（含原因）
            sqlx::query(
                r#"INSERT INTO ban_records (id, steam_id, reason, status, operator_name)
                   VALUES ($1, $2, '作弊', 'active', '管理员A')"#,
            )
            .bind(Uuid::new_v4())
            .bind(steamid64)
            .execute(&db.pool)
            .await?;
            let reason = detect_unresolved_ban(&db, steamid64).await?.unwrap();
            assert!(reason.contains("未解封本地封禁"), "实际：{reason}");

            // 封禁已过期（expires_at 过去）→ 视为无未解封封禁
            sqlx::query(
                "UPDATE ban_records SET expires_at = now() - interval '1 day' WHERE steam_id = $1",
            )
            .bind(steamid64)
            .execute(&db.pool)
            .await?;
            assert_eq!(detect_unresolved_ban(&db, steamid64).await?, None);

            // 未过期全球封禁 → Some
            insert_global_ban(&db, steamid64, "bhop_hack").await;
            let reason = detect_unresolved_ban(&db, steamid64).await?.unwrap();
            assert!(reason.contains("未过期全球封禁"), "实际：{reason}");
            Ok(())
        })
        .await;
    }
}
