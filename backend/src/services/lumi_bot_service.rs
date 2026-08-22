//! LumiBot（QQ 机器人事件接收中心）集成服务
//!
//! 外部事件（目前为白名单新申请）在产生时 **立即** 通过 LumiBot HTTP API
//! `POST /api/v1/events` 上报，触发 QQ 机器人立即推送通知。
//! 若立即上报失败，事件会降级写入 `lumi_bot_event_queue` 表，由后台任务
//! 每隔 `LUMI_BOT_SYNC_INTERVAL_SECS`（默认 1800s = 30 分钟）集中兜底重试，
//! 避免事件丢失。
//!
//! 协议说明见 LumiBot HTTP API 文档：
//! - 请求头：`Content-Type: application/json` + `X-API-Key`
//! - 成功响应：HTTP 202 `{"success": true, "event_id": "..."}`
//! - 失败响应：HTTP 400/401/429/500 `{"success": false, "error": "..."}`
//!
//! 兜底队列中上报失败的事件保留为 pending，下轮重试；超过最大重试次数后标记为
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

/// 事件队列逐条日志（供 LumiBot 状态页排查 bot 上报问题）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EventLogItem {
    pub id: Uuid,
    pub event_type: String,
    pub level: String,
    pub title: Option<String>,
    pub message: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub queued_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// 事件日志查询输入。
#[derive(Debug, Clone, Default)]
pub struct EventLogQuery {
    pub status: Option<String>,
    pub page: i64,
    pub page_size: i64,
}

/// 获取 LumiBot 事件日志（按入队时间倒序）。
///
/// status 仅允许 `pending` / `sent` / `failed`，非法值按未过滤处理。
pub async fn list_queue_events(
    db: &Database,
    query: &EventLogQuery,
) -> anyhow::Result<(Vec<EventLogItem>, i64)> {
    let status = query
        .status
        .as_deref()
        .filter(|value| matches!(*value, "pending" | "sent" | "failed"));

    let count_sql = match status {
        Some(value) => format!(r#"SELECT COUNT(*) FROM lumi_bot_event_queue WHERE status = '{value}'"#),
        None => r#"SELECT COUNT(*) FROM lumi_bot_event_queue"#.to_string(),
    };
    let data_sql = match status {
        Some(value) => format!(
            r#"SELECT id, event_type, level, title, message, status, attempts, last_error,
                      occurred_at, queued_at, sent_at, updated_at
               FROM lumi_bot_event_queue
               WHERE status = '{value}'
               ORDER BY queued_at DESC
               LIMIT $1 OFFSET $2"#
        ),
        None => format!(
            r#"SELECT id, event_type, level, title, message, status, attempts, last_error,
                      occurred_at, queued_at, sent_at, updated_at
               FROM lumi_bot_event_queue
               ORDER BY queued_at DESC
               LIMIT $1 OFFSET $2"#
        ),
    };

    let total: i64 = sqlx::query_scalar(&count_sql).fetch_one(&db.pool).await?;
    let items: Vec<EventLogItem> = sqlx::query_as(&data_sql)
        .bind(query.page_size)
        .bind((query.page - 1) * query.page_size)
        .fetch_all(&db.pool)
        .await?;
    Ok((items, total))
}

/// 一轮同步的结果统计
#[derive(Debug, Default, Serialize)]
pub struct SyncSummary {
    pub total: usize,
    pub sent: usize,
    pub failed: usize,
}

/// LumiBot 事件队列概况。
#[derive(Debug, Clone, Serialize)]
pub struct QueueOverview {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
}

/// LumiBot 集成状态，供管理后台的运维页面使用。
#[derive(Debug, Clone, Serialize)]
pub struct StatusOverview {
    pub configured: bool,
    pub api_url: Option<String>,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub checked_at: DateTime<Utc>,
    pub health_error: Option<String>,
    pub queue: QueueOverview,
    pub sync_task: Option<observability_service::TaskMetric>,
    pub last_error: Option<String>,
}

/// 获取 LumiBot 集成状态。
///
/// 健康探测与队列查询都在后端完成，前端不会接触 API Key。探测超时固定为
/// 3 秒，避免管理页面因为 LumiBot 不可达而长时间阻塞。
pub async fn status(db: &Database, config: &Config) -> anyhow::Result<StatusOverview> {
    let checked_at = Utc::now();
    let api_url = config
        .lumi_bot_api_url
        .as_ref()
        .map(|url| url.trim_end_matches('/').to_string());
    let configured = config.lumi_bot_enabled();

    let (reachable, latency_ms, health_error) = if configured {
        let health_url = format!("{}/health", api_url.as_deref().unwrap_or_default());
        let started = std::time::Instant::now();
        match tokio::time::timeout(
            Duration::from_secs(3),
            http_client::http_client().get(health_url).send(),
        )
        .await
        {
            Ok(Ok(response)) if response.status().is_success() => {
                (true, Some(started.elapsed().as_millis() as u64), None)
            }
            Ok(Ok(response)) => (
                false,
                Some(started.elapsed().as_millis() as u64),
                Some(format!("LumiBot 返回 HTTP {}", response.status())),
            ),
            Ok(Err(error)) => (
                false,
                Some(started.elapsed().as_millis() as u64),
                Some(error.to_string()),
            ),
            Err(_) => (false, Some(3_000), Some("健康检查超时（3 秒）".to_string())),
        }
    } else {
        (
            false,
            None,
            Some("未配置 LUMI_BOT_API_URL / LUMI_BOT_API_KEY".to_string()),
        )
    };

    let (pending, sent, failed, last_sent_at, last_failure_at): (
        i64,
        i64,
        i64,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    ) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending'),
            COUNT(*) FILTER (WHERE status = 'sent'),
            COUNT(*) FILTER (WHERE status = 'failed'),
            MAX(sent_at),
            MAX(updated_at) FILTER (WHERE status = 'failed')
        FROM lumi_bot_event_queue
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .context("读取 LumiBot 事件队列状态失败")?;

    let sync_task = observability_service::task_metric("lumi_bot_sync");
    let last_error = sync_task.as_ref().and_then(|task| task.last_error.clone());

    Ok(StatusOverview {
        configured,
        api_url,
        reachable,
        latency_ms,
        checked_at,
        health_error,
        queue: QueueOverview {
            pending,
            sent,
            failed,
            last_sent_at,
            last_failure_at,
        },
        sync_task,
        last_error,
    })
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

/// 白名单申请玩家补充信息（用于 LumiBot 定向推送时展示）
#[derive(Debug, Default, Serialize)]
pub struct WhitelistNotifyPlayerInfo {
    /// Steam 等级
    pub steam_level: Option<i32>,
    /// 各模式 rating
    pub ratings: serde_json::Value,
    /// 是否存在本地封禁记录
    pub has_local_ban: bool,
    /// 本地封禁条数
    pub local_ban_count: i64,
    /// 最近一条本地封禁原因
    pub local_ban_reason: Option<String>,
    /// 是否在全球封禁中留有记录
    pub has_global_ban: bool,
    /// 是否存在未解封（未过期）的封禁记录
    pub has_active_ban: bool,
    /// 未解封封禁条数
    pub active_ban_count: i64,
    /// 最近一条未解封封禁原因
    pub active_ban_reason: Option<String>,
}

/// 收集白名单申请玩家的补充信息：Steam 等级、各模式 rating、本地/全球/未解封封禁记录。
pub async fn collect_whitelist_player_info(
    db: &Database,
    steamid64: &str,
) -> WhitelistNotifyPlayerInfo {
    // Steam 等级：取缓存中存在的最新的非零等级
    let steam_level: Option<i32> = sqlx::query_scalar(
        r#"SELECT MAX(steam_level)
           FROM player_access_cache
           WHERE steamid64 = $1 AND steam_level > 0"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();

    // 各模式 rating：取 gokz_stats 来源的缓存行
    let kzt_rating: Option<f64> = sqlx::query_scalar(
        r#"SELECT (kzt_data->>'rating')::double precision
           FROM player_access_cache
           WHERE steamid64 = $1 AND rating_source = 'gokz_stats' AND kzt_data IS NOT NULL"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();
    let skz_rating: Option<f64> = sqlx::query_scalar(
        r#"SELECT (skz_data->>'rating')::double precision
           FROM player_access_cache
           WHERE steamid64 = $1 AND rating_source = 'gokz_stats' AND skz_data IS NOT NULL"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();
    let vnl_rating: Option<f64> = sqlx::query_scalar(
        r#"SELECT (vnl_data->>'rating')::double precision
           FROM player_access_cache
           WHERE steamid64 = $1 AND rating_source = 'gokz_stats' AND vnl_data IS NOT NULL"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();
    let ovr_rating: Option<f64> = sqlx::query_scalar(
        r#"SELECT (ovr_data->>'rating')::double precision
           FROM player_access_cache
           WHERE steamid64 = $1 AND rating_source = 'gokz_stats' AND ovr_data IS NOT NULL"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();

    // 是否有本地封禁记录
    let has_local_ban: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM ban_records WHERE steam_id = $1 LIMIT 1)"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(false);
    // 本地封禁条数
    let local_ban_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM ban_records WHERE steam_id = $1"#)
            .bind(steamid64)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0);
    // 最近一条本地封禁原因
    let raw_local_ban_reason: Option<String> = sqlx::query_scalar(
        r#"SELECT reason FROM ban_records WHERE steam_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();
    let local_ban_reason: Option<String> = raw_local_ban_reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "未填写");
    // 是否在全球封禁中留有记录
    let has_global_ban: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM global_bans WHERE steam_id64 = $1 LIMIT 1)"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(false);
    // 是否有未解封（未过期）的封禁记录
    let has_active_ban: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM ban_records
                        WHERE steam_id = $1
                          AND status = 'active'
                          AND (expires_at IS NULL OR expires_at > now())
                        LIMIT 1)"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(false);
    // 未解封封禁条数
    let active_ban_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM ban_records
           WHERE steam_id = $1
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > now())"#,
    )
    .bind(steamid64)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(0);
    // 最近一条未解封封禁原因
    let raw_active_ban_reason: Option<String> = sqlx::query_scalar(
        r#"SELECT reason FROM ban_records
           WHERE steam_id = $1
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > now())
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(steamid64)
    .fetch_optional(&db.pool)
    .await
    .ok()
    .flatten();
    let active_ban_reason: Option<String> = raw_active_ban_reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "未填写");

    WhitelistNotifyPlayerInfo {
        steam_level,
        ratings: serde_json::json!({
            "kzt": kzt_rating,
            "skz": skz_rating,
            "vnl": vnl_rating,
            "ovr": ovr_rating,
        }),
        has_local_ban,
        local_ban_count,
        local_ban_reason,
        has_global_ban,
        has_active_ban,
        active_ban_count,
        active_ban_reason,
    }
}

/// 白名单新申请上报（公开页面提交 / 撤销后重新申请都会产生新申请）。
///
/// 级别使用 `warning`：与 LumiBot 默认通知规则（warning 及以上默认触发 QQ
/// 通知）对齐，确保管理员能及时收到审核提醒。
///
/// 上报策略：白名单申请产生时 **立即** 调用 LumiBot API 上报，再由 QQ 机器人
/// 立即推送通知；若立即上报失败（网络/服务不可用等），则降级写入
/// `lumi_bot_event_queue`，由后台定时任务兜底重试，避免事件丢失。
pub async fn report_whitelist_created(
    db: &Database,
    config: &Config,
    item: &WhitelistItem,
) -> anyhow::Result<()> {
    let display_name = item.steam_persona_name.as_deref().unwrap_or(&item.nickname);
    // 收集玩家补充信息（Steam 等级、各模式 rating、封禁记录）
    let player_info = collect_whitelist_player_info(db, &item.steamid64).await;
    // 收集已启用网站管理员的 openid，供 LumiBot 按账号定向推送 QQ 通知
    let admin_openids: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT openid FROM users
           WHERE role IN ('developer', 'admin', 'normal')
             AND enabled = true
             AND whitelist_notification_enabled = true
             AND openid IS NOT NULL AND openid <> ''"#,
    )
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();
    let input = EventInput {
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
            // 玩家补充信息：Steam 等级、各模式 rating、封禁记录
            "steam_level": player_info.steam_level,
            "ratings": player_info.ratings,
            "has_local_ban": player_info.has_local_ban,
            "local_ban_count": player_info.local_ban_count,
            "local_ban_reason": player_info.local_ban_reason,
            "has_global_ban": player_info.has_global_ban,
            "has_active_ban": player_info.has_active_ban,
            "active_ban_count": player_info.active_ban_count,
            "active_ban_reason": player_info.active_ban_reason,
            // 优先发给 LumiBot 配置的默认管理员；若配置了管理员 openid 则同时定向通知
            "openids": admin_openids,
        }),
    };

    // 未配置 LumiBot：仍写入队列便于排查（事件保留在队列中，待配置后由后台任务补报）
    if !config.lumi_bot_enabled() {
        tracing::info!(
            "LumiBot 未配置（缺少 LUMI_BOT_API_URL / LUMI_BOT_API_KEY），白名单申请事件已入队待配置后上报"
        );
        let queued_id = enqueue_event(db, input).await?;
        tracing::info!(
            queued_id = %queued_id,
            "白名单申请事件已写入 LumiBot 事件队列（等待配置后上报）"
        );
        return Ok(());
    }

    let api_base_url = config
        .lumi_bot_api_url
        .as_deref()
        .context("LUMI_BOT_API_URL 未配置")?;
    let api_key = config
        .lumi_bot_api_key
        .as_deref()
        .context("LUMI_BOT_API_KEY 未配置")?;

    let id = Uuid::new_v4();
    let occurred_at = Utc::now();
    let body = build_event_body(
        id,
        &input.event_type,
        &input.level,
        Some(&input.title),
        Some(&input.message),
        &input.data,
        &occurred_at,
    );

    match send_event_payload(api_base_url, api_key, &body).await {
        Ok(()) => {
            // 先入队再标记成功：保证事件日志完整（queued_at 即提交时间，sent_at 为上报时间）
            let queued_id = enqueue_event(db, input.clone()).await?;
            sqlx::query(
                r#"
                UPDATE lumi_bot_event_queue
                SET status = 'sent',
                    sent_at = now(),
                    attempts = 1,
                    last_error = NULL,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(queued_id)
            .execute(&db.pool)
            .await
            .context("标记 LumiBot 事件为已上报失败")?;
            tracing::info!(
                event_id = %id,
                event_type = %input.event_type,
                "白名单申请事件已立即上报 LumiBot"
            );
            Ok(())
        }
        Err(error) => {
            // 立即上报失败，降级入队由后台任务兜底重试
            tracing::warn!(
                %error,
                event_id = %id,
                "白名单申请事件立即上报失败，降级入队等待后台重试"
            );
            let queued_id = enqueue_event(db, input).await?;
            tracing::warn!(
                queued_id = %queued_id,
                "白名单申请事件已写入 LumiBot 事件队列（后台兜底重试）"
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// 后台定时兜底重试任务
//
// 白名单事件在产生时已立即上报；仅当立即上报失败时才入队，由本任务集中
// 兜底重试，避免事件因临时的网络/服务不可用而丢失。
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
                |summary| {
                    format!(
                        "本轮上报 {} 条（成功 {}，失败 {}）",
                        summary.total, summary.sent, summary.failed
                    )
                },
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
                let attempts =
                    record_failure(db, row.id, &error, config.lumi_bot_max_attempts as i32).await?;
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

/// 向 LumiBot 上报单条事件（自定义载荷）。
/// 按 LumiBot HTTP API 文档组装统一事件模型：
/// `POST {api_base_url}/api/v1/events`，Header 携带 `X-API-Key`。
async fn send_event_payload(
    api_base_url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/events", api_base_url.trim_end_matches('/'));

    let response = http_client::http_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("X-API-Key", api_key)
        .json(body)
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

/// 向 LumiBot 上报单条事件的统一载荷构建。
fn build_event_body(
    id: Uuid,
    event_type: &str,
    level: &str,
    title: Option<&str>,
    message: Option<&str>,
    data: &serde_json::Value,
    occurred_at: &DateTime<Utc>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "source": SOURCE_LUMI_ADMIN,
        "event_type": event_type,
        "level": level,
        "timestamp": occurred_at.to_rfc3339(),
        "title": title,
        "message": message,
        "data": data,
    })
}

/// 向 LumiBot 上报单条事件。
/// 按 LumiBot HTTP API 文档组装统一事件模型：
/// `POST {api_base_url}/api/v1/events`，Header 携带 `X-API-Key`。
async fn send_event(api_base_url: &str, api_key: &str, row: &QueuedEventRow) -> anyhow::Result<()> {
    let body = build_event_body(
        row.id,
        &row.event_type,
        &row.level,
        row.title.as_deref(),
        row.message.as_deref(),
        &row.data,
        &row.occurred_at,
    );
    send_event_payload(api_base_url, api_key, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::Database, test_util};
    use axum::{http::StatusCode, routing::get, Router};
    use tokio::task::JoinHandle;
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

    async fn spawn_health_server(status_code: StatusCode) -> (String, JoinHandle<()>) {
        let app = Router::new().route("/health", get(move || async move { status_code }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    #[tokio::test]
    async fn status_reports_unconfigured_integration_and_queue_counts() {
        with_test_db(async |db| {
            sqlx::query(
                r#"INSERT INTO lumi_bot_event_queue
                   (id, event_type, level, status, sent_at, updated_at)
                   VALUES
                   ($1, 'TEST_PENDING', 'info', 'pending', NULL, now()),
                   ($2, 'TEST_SENT', 'info', 'sent', now() - interval '1 minute', now()),
                   ($3, 'TEST_FAILED', 'error', 'failed', NULL, now() - interval '2 minutes')"#,
            )
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            let mut config = Config::from_env();
            config.lumi_bot_api_url = None;
            config.lumi_bot_api_key = None;

            let overview = status(&db, &config).await?;
            assert!(!overview.configured);
            assert!(!overview.reachable);
            assert_eq!(overview.api_url, None);
            assert_eq!(overview.latency_ms, None);
            assert!(overview.health_error.is_some());
            assert_eq!(overview.queue.pending, 1);
            assert_eq!(overview.queue.sent, 1);
            assert_eq!(overview.queue.failed, 1);
            assert!(overview.queue.last_sent_at.is_some());
            assert!(overview.queue.last_failure_at.is_some());
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn status_reports_reachable_health_endpoint() {
        with_test_db(async |db| {
            let (api_url, server) = spawn_health_server(StatusCode::OK).await;
            let mut config = Config::from_env();
            config.lumi_bot_api_url = Some(format!("{api_url}/"));
            config.lumi_bot_api_key = Some("test-key".to_string());

            let overview = status(&db, &config).await;
            server.abort();
            let overview = overview?;

            assert!(overview.configured);
            assert!(overview.reachable);
            assert_eq!(overview.api_url.as_deref(), Some(api_url.as_str()));
            assert!(overview.latency_ms.is_some());
            assert_eq!(overview.health_error, None);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn status_reports_unhealthy_http_response() {
        with_test_db(async |db| {
            let (api_url, server) = spawn_health_server(StatusCode::SERVICE_UNAVAILABLE).await;
            let mut config = Config::from_env();
            config.lumi_bot_api_url = Some(api_url);
            config.lumi_bot_api_key = Some("test-key".to_string());

            let overview = status(&db, &config).await;
            server.abort();
            let overview = overview?;

            assert!(overview.configured);
            assert!(!overview.reachable);
            assert!(overview.latency_ms.is_some());
            assert!(overview
                .health_error
                .as_deref()
                .is_some_and(|error| error.contains("503 Service Unavailable")));
            Ok(())
        })
        .await;
    }

    /// 立即上报失败时降级入队，事件不丢失
    #[tokio::test]
    async fn report_whitelist_created_falls_back_to_enqueue_on_failure() {
        with_test_db(async |db| {
            let mut config = Config::from_env();
            config.lumi_bot_api_url = Some("http://127.0.0.1:9".to_string()); // 必然连接失败
            config.lumi_bot_api_key = Some("key-admin".to_string());

            sqlx::query(
                r#"INSERT INTO users
                   (id, username, display_name, password_hash, role, openid, whitelist_notification_enabled)
                   VALUES
                   ($1, 'notify-user', 'Notify User', 'test', 'normal', 'openid-enabled', true),
                   ($2, 'muted-user', 'Muted User', 'test', 'normal', 'openid-disabled', false)"#,
            )
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            // 构造白名单申请项
            let item = WhitelistItem {
                id: Uuid::new_v4(),
                steamid64: "76561198000000001".to_string(),
                steamid: Some("STEAM_1:0:1".to_string()),
                steamid3: Some("[U:1:1]".to_string()),
                profile_url: None,
                nickname: "玩家A".to_string(),
                steam_persona_name: Some("玩家A".to_string()),
                contact: None,
                status: "pending".to_string(),
                applied_at: Utc::now().to_rfc3339(),
                approved_at: None,
                approved_by: None,
                approval_reason: None,
                rejected_at: None,
                rejected_by: None,
                rejection_reason: None,
                risk_profile: None,
            };

            report_whitelist_created(&db, &config, &item).await?;

            // 立即上报失败后应降级入队兜底重试
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM lumi_bot_event_queue WHERE status = 'pending'",
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(count, 1);
            let data: serde_json::Value = sqlx::query_scalar(
                "SELECT data FROM lumi_bot_event_queue WHERE status = 'pending'",
            )
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(data["openids"], serde_json::json!(["openid-enabled"]));
            Ok(())
        })
        .await;
    }

    /// 立即上报成功时也写入事件日志（status='sent'），保证状态页能看到全部事件
    #[tokio::test]
    async fn report_whitelist_created_records_success_as_sent() {
        with_test_db(async |db| {
            // 健康检查 + 事件上报共用同一个监听端口：/health 返回 200，/api/v1/events 返回 202
            let app = axum::Router::new()
                .route(
                    "/health",
                    axum::routing::get(|| async { axum::http::StatusCode::OK }),
                )
                .route(
                    "/api/v1/events",
                    axum::routing::post(|| async { axum::http::StatusCode::ACCEPTED }),
                );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let mut config = Config::from_env();
            config.lumi_bot_api_url = Some(format!("http://{address}"));
            config.lumi_bot_api_key = Some("key-admin".to_string());

            let item = WhitelistItem {
                id: Uuid::new_v4(),
                steamid64: "76561198000000002".to_string(),
                steamid: Some("STEAM_1:0:2".to_string()),
                steamid3: Some("[U:1:2]".to_string()),
                profile_url: None,
                nickname: "玩家B".to_string(),
                steam_persona_name: None,
                contact: None,
                status: "pending".to_string(),
                applied_at: Utc::now().to_rfc3339(),
                approved_at: None,
                approved_by: None,
                approval_reason: None,
                rejected_at: None,
                rejected_by: None,
                rejection_reason: None,
                risk_profile: None,
            };

            report_whitelist_created(&db, &config, &item).await?;
            server.abort();

            // 立即上报成功后应写入事件日志并标记为已上报
            let (status, attempts): (String, i32) = sqlx::query_as(
                "SELECT status, attempts FROM lumi_bot_event_queue WHERE data->>'steamid64' = $1",
            )
            .bind("76561198000000002")
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "sent");
            assert_eq!(attempts, 1);
            Ok(())
        })
        .await;
    }

    /// 收集玩家补充信息：Steam 等级、各模式 rating、封禁记录
    #[tokio::test]
    async fn collect_whitelist_player_info_gathers_extra_fields() {
        with_test_db(async |db| {
            let sid = "76561198000000001";

            // 写入 Steam 等级 + gokz 各模式 rating 缓存
            sqlx::query(
                r#"INSERT INTO player_access_cache
                   (steamid64, rating, steam_level, rating_source, kzt_data, skz_data, vnl_data, ovr_data, expires_at)
                   VALUES ($1, 0, 42, 'gokz_stats', $2, $3, $4, $5, now() + interval '1 hour')"#,
            )
            .bind(sid)
            .bind(serde_json::json!({ "rating": 1500.5 }))
            .bind(serde_json::json!({ "rating": 2000.0 }))
            .bind(serde_json::json!({ "rating": 1100.25 }))
            .bind(serde_json::json!({ "rating": 1800.75 }))
            .execute(&db.pool)
            .await?;

            // 写入本地封禁记录（一条已过期、一条未解封）
            sqlx::query(
                r#"INSERT INTO ban_records (id, player, steam_id, status, operator_name, expires_at)
                   VALUES (gen_random_uuid(), 'A', $1, 'active', 'admin', now() - interval '1 hour'),
                          (gen_random_uuid(), 'A', $1, 'active', 'admin', now() + interval '1 hour')"#,
            )
            .bind(sid)
            .execute(&db.pool)
            .await?;

            // 写入全球封禁记录
            sqlx::query(
                r#"INSERT INTO global_bans (id, kzt_ban_id, steam_id64, player_name, ban_type, is_expired)
                   VALUES (gen_random_uuid(), 12345, $1, 'A', 'ban', false)"#,
            )
            .bind(sid)
            .execute(&db.pool)
            .await?;

            let info = collect_whitelist_player_info(&db, sid).await;
            assert_eq!(info.steam_level, Some(42));
            assert_eq!(info.ratings["kzt"], serde_json::json!(1500.5));
            assert_eq!(info.ratings["skz"], serde_json::json!(2000.0));
            assert_eq!(info.ratings["vnl"], serde_json::json!(1100.25));
            assert_eq!(info.ratings["ovr"], serde_json::json!(1800.75));
            assert!(info.has_local_ban);
            assert!(info.has_global_ban);
            assert!(info.has_active_ban);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn collect_whitelist_player_info_defaults_to_empty_when_no_data() {
        with_test_db(async |db| {
            let info = collect_whitelist_player_info(&db, "76561198000009999").await;
            assert_eq!(info.steam_level, None);
            assert_eq!(info.ratings["kzt"], serde_json::Value::Null);
            assert!(!info.has_local_ban);
            assert!(!info.has_global_ban);
            assert!(!info.has_active_ban);
            Ok(())
        })
        .await;
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

            let (status, attempts): (String, i32) =
                sqlx::query_as("SELECT status, attempts FROM lumi_bot_event_queue WHERE id = $1")
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
            let (status, attempts): (String, i32) =
                sqlx::query_as("SELECT status, attempts FROM lumi_bot_event_queue WHERE id = $1")
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
