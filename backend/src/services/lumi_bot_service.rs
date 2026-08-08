//! LumiBot（QQ 机器人事件接收中心）集成服务
//!
//! 外部事件（目前为白名单新申请）先写入 `lumi_bot_event_queue` 表，
//! 由后台任务每隔 `LUMI_BOT_SYNC_INTERVAL_SECS`（默认 1800s = 30 分钟）
//! 集中通过 LumiBot HTTP API `POST /api/v1/events` 上报。
//!
//! 协议说明见 LumiBot HTTP API 文档：
//! - 请求头：`Content-Type: application/json` + `X-API-Key`
//! - 成功响应：HTTP 202 `{"success": true, "event_id": "..."}`
//! - 失败响应：HTTP 400/401/429/500 `{"success": false, "error": "..."}`
//!
//! 上报失败的事件保留为 pending，下轮重试；超过最大重试次数后标记为
//! failed（死信），不再自动重试，便于人工排查。

use crate::{
    config::Config,
    db::Database,
    http_client,
    services::{observability_service, whitelist_service::WhitelistItem},
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

/// LumiBot 事件来源标识（对应 LumiBot API 文档 source 字段）
pub const SOURCE_LUMI_ADMIN: &str = "LumiAdmin";

/// 白名单新申请事件类型（自定义事件类型，LumiBot 全部接收并记录日志，
/// 是否触发 QQ 通知由 LumiBot 侧通知规则决定）
pub const EVENT_WHITELIST_REQUEST_CREATED: &str = "WHITELIST_REQUEST_CREATED";

/// 事件入队输入
#[derive(Debug, Clone, Serialize)]
pub struct EventInput {
    pub event_type: String,
    pub level: String,
    pub title: String,
    pub message: String,
    pub data: serde_json::Value,
}

/// 待上报的队列行
#[derive(Debug, sqlx::FromRow)]
struct QueuedEventRow {
    id: Uuid,
    event_type: String,
    level: String,
    title: Option<String>,
    message: Option<String>,
    data: serde_json::Value,
    occurred_at: DateTime<Utc>,
}

/// 一轮同步的结果统计
#[derive(Debug, Default, Serialize)]
pub struct SyncSummary {
    pub total: usize,
    pub sent: usize,
    pub failed: usize,
}

// ---------------------------------------------------------------------------
// 入队
// ---------------------------------------------------------------------------

/// 通用事件入队：写入 `lumi_bot_event_queue`，等待后台任务批量上报。
/// 返回队列记录 ID（同时作为上报给 LumiBot 的事件 id，保证幂等）。
pub async fn enqueue_event(db: &Database, input: EventInput) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO lumi_bot_event_queue (id, event_type, level, title, message, data)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(&input.event_type)
    .bind(&input.level)
    .bind(&input.title)
    .bind(&input.message)
    .bind(&input.data)
    .execute(&db.pool)
    .await
    .context("写入 LumiBot 事件队列失败")?;
    Ok(id)
}

/// 白名单新申请事件入队（公开页面提交 / 撤销后重新申请都会产生新申请）。
///
/// 级别使用 `warning`：与 LumiBot 默认通知规则（warning 及以上默认触发 QQ
/// 通知）对齐，确保管理员能及时收到审核提醒。
pub async fn enqueue_whitelist_created(
    db: &Database,
    item: &WhitelistItem,
) -> anyhow::Result<Uuid> {
    let display_name = item.steam_persona_name.as_deref().unwrap_or(&item.nickname);
    enqueue_event(
        db,
        EventInput {
            event_type: EVENT_WHITELIST_REQUEST_CREATED.to_string(),
            level: "warning".to_string(),
            title: "新白名单申请".to_string(),
            message: format!(
                "玩家 {}（{}）提交了白名单申请，等待审核",
                display_name, item.steamid64
            ),
            data: serde_json::json!({
                "whitelist_id": item.id,
                "steamid64": item.steamid64,
                "steamid": item.steamid,
                "steamid3": item.steamid3,
                "nickname": item.nickname,
                "steam_persona_name": item.steam_persona_name,
                "contact": item.contact,
                "profile_url": item.profile_url,
                "applied_at": item.applied_at,
                // 未指定 target_openid：通知发给 LumiBot 配置的默认管理员
            }),
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// 后台定时上报任务
// ---------------------------------------------------------------------------

/// 启动 LumiBot 事件上报循环。
/// 未配置 `LUMI_BOT_API_URL` / `LUMI_BOT_API_KEY` 时不启动（仅登记任务状态）。
pub fn start_sync_loop(db: Database, config: Config) {
    let enabled = config.lumi_bot_enabled();
    observability_service::register_task(
        "lumi_bot_sync",
        "LumiBot 事件上报",
        "集成",
        Some(config.lumi_bot_sync_interval_secs),
        enabled,
    );
    if !enabled {
        tracing::info!(
            "LumiBot 未配置（缺少 LUMI_BOT_API_URL / LUMI_BOT_API_KEY），事件上报已禁用"
        );
        return;
    }

    tokio::spawn(async move {
        // 间隔至少 60 秒，避免误配置导致高频请求
        let interval_secs = config.lumi_bot_sync_interval_secs.max(60);
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            match observability_service::observe_task(
                "lumi_bot_sync",
                sync_pending_events(&db, &config),
                |summary| format!("本轮上报 {} 条（成功 {}，失败 {}）", summary.total, summary.sent, summary.failed),
            )
            .await
            {
                Ok(summary) => {
                    if summary.total > 0 {
                        tracing::info!(
                            total = summary.total,
                            sent = summary.sent,
                            failed = summary.failed,
                            "LumiBot 事件上报完成"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "LumiBot 事件上报失败");
                }
            }
        }
    });
}

/// 执行一轮同步：取出待上报队列（含重试），逐条调用 LumiBot API 上报。
pub async fn sync_pending_events(db: &Database, config: &Config) -> anyhow::Result<SyncSummary> {
    let api_base_url = config
        .lumi_bot_api_url
        .as_deref()
        .context("LUMI_BOT_API_URL 未配置")?;
    let api_key = config
        .lumi_bot_api_key
        .as_deref()
        .context("LUMI_BOT_API_KEY 未配置")?;

    let rows: Vec<QueuedEventRow> = sqlx::query_as(
        r#"
        SELECT id, event_type, level, title, message, data, occurred_at
        FROM lumi_bot_event_queue
        WHERE status = 'pending' AND attempts < $1
        ORDER BY occurred_at ASC, queued_at ASC
        LIMIT $2
        "#,
    )
    .bind(config.lumi_bot_max_attempts as i32)
    .bind(config.lumi_bot_batch_size as i64)
    .fetch_all(&db.pool)
    .await
    .context("读取 LumiBot 事件队列失败")?;

    let mut summary = SyncSummary {
        total: rows.len(),
        ..SyncSummary::default()
    };

    for row in rows {
        match send_event(api_base_url, api_key, &row).await {
            Ok(()) => {
                sqlx::query(
                    r#"
                    UPDATE lumi_bot_event_queue
                    SET status = 'sent',
                        sent_at = now(),
                        last_error = NULL,
                        updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(row.id)
                .execute(&db.pool)
                .await
                .context("更新 LumiBot 事件队列状态失败")?;
                summary.sent += 1;
                tracing::info!(
                    event_id = %row.id,
                    event_type = %row.event_type,
                    "LumiBot 事件上报成功"
                );
            }
            Err(error) => {
                let attempts = record_failure(db, row.id, &error, config.lumi_bot_max_attempts as i32)
                    .await?;
                summary.failed += 1;
                if attempts >= config.lumi_bot_max_attempts as i32 {
                    tracing::warn!(
                        event_id = %row.id,
                        attempts,
                        max_attempts = config.lumi_bot_max_attempts,
                        %error,
                        "LumiBot 事件重试次数耗尽，标记为 failed（不再自动重试）"
                    );
                } else {
                    tracing::warn!(
                        event_id = %row.id,
                        attempts,
                        %error,
                        "LumiBot 事件上报失败，将在下一轮重试"
                    );
                }
            }
        }
    }

    Ok(summary)
}

/// 上报失败：累计尝试次数；达到上限标记为 failed（死信），否则保留 pending 等待下轮重试。
async fn record_failure(
    db: &Database,
    id: Uuid,
    error: &anyhow::Error,
    max_attempts: i32,
) -> anyhow::Result<i32> {
    let (attempts,): (i32,) = sqlx::query_as(
        r#"
        UPDATE lumi_bot_event_queue
        SET attempts = attempts + 1,
            last_error = $2,
            status = CASE WHEN attempts + 1 >= $3 THEN 'failed' ELSE 'pending' END,
            updated_at = now()
        WHERE id = $1
        RETURNING attempts
        "#,
    )
    .bind(id)
    .bind(error.to_string())
    .bind(max_attempts)
    .fetch_one(&db.pool)
    .await
    .context("更新 LumiBot 事件队列失败状态失败")?;
    Ok(attempts)
}

// ---------------------------------------------------------------------------
// 上报
// ---------------------------------------------------------------------------

/// 向 LumiBot 上报单条事件。
/// 按 LumiBot HTTP API 文档组装统一事件模型：
/// `POST {api_base_url}/api/v1/events`，Header 携带 `X-API-Key`。
async fn send_event(
    api_base_url: &str,
    api_key: &str,
    row: &QueuedEventRow,
) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/events", api_base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "id": row.id,
        "source": SOURCE_LUMI_ADMIN,
        "event_type": row.event_type,
        "level": row.level,
        "timestamp": row.occurred_at.to_rfc3339(),
        "title": row.title,
        "message": row.message,
        "data": row.data,
    });

    let response = http_client::http_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-API-Key", api_key)
        .json(&body)
        .send()
        .await
        .context("请求 LumiBot 失败")?;

    let status = response.status();
    if status == axum::http::StatusCode::ACCEPTED {
        return Ok(());
    }

    // 读取响应体（截断，避免超长错误信息刷日志）
    let text = response.text().await.unwrap_or_default();
    let truncated: String = text.chars().take(300).collect();
    anyhow::bail!("LumiBot 返回 HTTP {status}: {truncated}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database, test_util};
    use uuid::Uuid;

    fn schema_url(base_url: &str, schema: &str) -> String {
        test_util::schema_url(base_url, schema)
    }

    async fn create_schema(base_url: &str, schema: &str) {
        test_util::create_schema(base_url, schema).await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        test_util::drop_schema(base_url, schema).await;
    }

    async fn with_test_db(test: impl AsyncFnOnce(Database) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect_for_test(&scoped_url).await?;
            db.migrate().await?;
            test(db).await
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn enqueue_event_inserts_pending_row() {
        with_test_db(async |db| {
            let id = enqueue_event(
                &db,
                EventInput {
                    event_type: EVENT_WHITELIST_REQUEST_CREATED.to_string(),
                    level: "warning".to_string(),
                    title: "新白名单申请".to_string(),
                    message: "玩家 A 提交了白名单申请".to_string(),
                    data: serde_json::json!({"steamid64": "76561198000000001"}),
                },
            )
            .await?;

            let (status, attempts): (String, i32) = sqlx::query_as(
                "SELECT status, attempts FROM lumi_bot_event_queue WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "pending");
            assert_eq!(attempts, 0);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn sync_pending_events_marks_failed_after_max_attempts() {
        with_test_db(async |db| {
            let mut config = Config::from_env();
            config.lumi_bot_api_url = Some("http://127.0.0.1:9".to_string()); // 必然连接失败
            config.lumi_bot_api_key = Some("key-admin".to_string());
            config.lumi_bot_max_attempts = 2;
            config.lumi_bot_batch_size = 100;

            let id = enqueue_event(
                &db,
                EventInput {
                    event_type: EVENT_WHITELIST_REQUEST_CREATED.to_string(),
                    level: "warning".to_string(),
                    title: "新白名单申请".to_string(),
                    message: "测试".to_string(),
                    data: serde_json::json!({}),
                },
            )
            .await?;

            // 第一轮：上报失败，保留 pending，attempts = 1
            let summary = sync_pending_events(&db, &config).await?;
            assert_eq!(summary.total, 1);
            assert_eq!(summary.failed, 1);
            let (status, attempts): (String, i32) = sqlx::query_as(
                "SELECT status, attempts FROM lumi_bot_event_queue WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "pending");
            assert_eq!(attempts, 1);

            // 第二轮：再次失败，达到上限，标记 failed
            let summary = sync_pending_events(&db, &config).await?;
            assert_eq!(summary.total, 1);
            assert_eq!(summary.failed, 1);
            let (status, attempts, last_error): (String, i32, Option<String>) = sqlx::query_as(
                "SELECT status, attempts, last_error FROM lumi_bot_event_queue WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "failed");
            assert_eq!(attempts, 2);
            assert!(last_error.is_some());

            // 第三轮：failed 不再被取出
            let summary = sync_pending_events(&db, &config).await?;
            assert_eq!(summary.total, 0);
            Ok(())
        })
        .await;
    }
}
