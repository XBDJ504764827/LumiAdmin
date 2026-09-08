//! LumiBot（QQ 机器人事件接收中心）集成服务
//!
//! 外部事件（目前为白名单新申请）在产生时先写入
//! `lumi_bot_event_queue` 表，用户请求立即返回；后台任务再通过
//! LumiBot HTTP API `POST /api/v1/events` 异步发送并重试，避免外部服务阻塞业务请求。
//!
//! 协议说明见 LumiBot HTTP API 文档：
//! - 请求头：`Content-Type: application/json` + `X-API-Key`
//! - 成功响应：HTTP 202 `{"success": true, "event_id": "..."}`
//! - 失败响应：HTTP 400/401/429/500 `{"success": false, "error": "..."}`
//!
//! 队列中上报失败的事件保留为 pending，下轮重试；超过最大重试次数后标记为
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

/// 低风险白名单自动通过事件类型（info 级别，默认不触发管理员通知，仅作记录与扩展）
pub const EVENT_WHITELIST_AUTO_APPROVED: &str = "WHITELIST_AUTO_APPROVED";

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
        Some(value) => {
            format!(r#"SELECT COUNT(*) FROM lumi_bot_event_queue WHERE status = '{value}'"#)
        }
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
        None => r#"SELECT id, event_type, level, title, message, status, attempts, last_error,
                          occurred_at, queued_at, sent_at, updated_at
               FROM lumi_bot_event_queue
               ORDER BY queued_at DESC
               LIMIT $1 OFFSET $2"#
            .to_string(),
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
    /// 全球封禁原因（最近一条，ban_type + notes）
    pub global_ban_reason: Option<String>,
    /// 全球封禁原因列表（全部未过期记录，最多 5 条）
    pub global_ban_reasons: Vec<String>,
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
    // 全球封禁原因（未过期，ban_type + notes 拼接，最多 5 条）
    let global_ban_rows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT ban_type, notes FROM global_bans
           WHERE steam_id64 = $1 AND is_expired = false AND manual_unbanned = false
           ORDER BY COALESCE(created_on, updated_on) DESC, synced_at DESC
           LIMIT 5"#,
    )
    .bind(steamid64)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();
    let global_ban_reasons: Vec<String> = global_ban_rows
        .into_iter()
        .map(|(ban_type, notes)| {
            // 违规展示以 ban_type 为主，notes 附在后；用户示例仅显示 ban_type
            let notes = notes
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            match notes {
                Some(notes) if !notes.eq_ignore_ascii_case(ban_type.trim()) => {
                    format!("{ban_type} - {notes}")
                }
                _ => ban_type,
            }
        })
        .collect();
    let global_ban_reason: Option<String> = global_ban_reasons.first().cloned();
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
        global_ban_reason,
        global_ban_reasons,
        has_active_ban,
        active_ban_count,
        active_ban_reason,
    }
}

/// 白名单新申请入队（公开页面提交 / 撤销后重新申请都会产生新申请）。
///
/// 该函数只做本地数据库工作，绝不等待 LumiBot 网络请求。后台 worker 会
/// 通过带 claim 的队列异步发送并重试，保证用户请求不会被外部服务拖住。
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
    // 风险动作：低风险（allow）在无人审核时将由系统自动通过；其余等待管理员
    let risk_action: Option<String> =
        crate::services::player_risk_service::build_player_risk_profile(db, &item.steamid64)
            .await
            .ok()
            .map(|profile| match profile.action {
                crate::services::player_risk_service::RiskAction::Allow => "allow",
                crate::services::player_risk_service::RiskAction::Warn => "warn",
                crate::services::player_risk_service::RiskAction::RequireForce => "require_force",
                crate::services::player_risk_service::RiskAction::Deny => "deny",
            })
            .map(str::to_string);
    // 中文风险标签（供 QQ 通知直接展示）：低风险 / 历史风险 / 高风险（需强制通过）
    let risk_label = risk_action.as_deref().map(|action| match action {
        "allow" => "低风险",
        "warn" => "历史风险",
        "require_force" | "deny" => "高风险",
        _ => action,
    });
    // 风险展示（带 emoji 前缀，供 QQ 模板直接渲染）
    let risk_display = risk_label.map(|label| match label {
        "低风险" => "🟢 低风险".to_string(),
        "历史风险" => "🟡 历史风险".to_string(),
        "高风险" => "🔴 高风险".to_string(),
        other => format!("⚠️ {other}"),
    });
    // 封禁展示：全球封禁用 ❌ 前缀；组合标记
    let mut ban_flags: Vec<&str> = Vec::new();
    if player_info.has_global_ban {
        ban_flags.push("❌ 全球封禁");
    }
    if player_info.has_local_ban {
        ban_flags.push("本地封禁");
    }
    if player_info.has_active_ban {
        ban_flags.push("未解封");
    }
    let ban_flags = if ban_flags.is_empty() {
        "无".to_string()
    } else {
        ban_flags.join(" / ")
    };
    // 违规原因：优先全球封禁原因（同用户示例：仅 ban_type 一行），其次未解封原因，再退本地封禁原因
    let ban_reason = player_info
        .global_ban_reason
        .clone()
        .or_else(|| player_info.active_ban_reason.clone())
        .or_else(|| player_info.local_ban_reason.clone());
    // 自动通过配置（开关 + 等待小时数），供通知展示“预计自动通过”信息
    let auto_approve = crate::services::whitelist_auto_approve_service::load_config(db)
        .await
        .unwrap_or(
            crate::services::whitelist_auto_approve_service::AutoApproveConfig {
                enabled: false,
                hours: 3,
                updated_by: None,
                updated_at: Utc::now(),
            },
        );
    // 自动审核文案：低风险（allow）→ 显示等待时长；中/高风险 → 等待管理员手动审核
    let is_low_risk = risk_action.as_deref() == Some("allow");
    let auto_approve_text = if !is_low_risk {
        "风险玩家等待管理员进行手动审核".to_string()
    } else if auto_approve.enabled {
        format!("{}小时", auto_approve.hours)
    } else {
        "未开启".to_string()
    };
    // 通知级别：只有需要人工审核的申请才推送 QQ（提醒管理员审核）——
    // 低风险且自动通过开启时交由系统自动通过，info 级别仅作记录，
    // LumiBot 侧规则（WHITELIST_REQUEST_CREATED 要求 >= warning）会过滤不推送。
    let notify_level = if is_low_risk && auto_approve.enabled {
        "info"
    } else {
        "warning"
    };
    // 详情链接：优先管理后台白名单页（ADMIN_WEB_URL），其次 Steam 主页
    let detail_url = config
        .admin_web_url
        .as_ref()
        .map(|base| format!("{base}/whitelist"))
        .or_else(|| item.profile_url.clone());
    let input = EventInput {
        event_type: EVENT_WHITELIST_REQUEST_CREATED.to_string(),
        level: notify_level.to_string(),
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
            // 通知展示用昵称
            "nickname_show": display_name,
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
            "global_ban_reason": player_info.global_ban_reason,
            "global_ban_reasons": player_info.global_ban_reasons,
            "has_active_ban": player_info.has_active_ban,
            "active_ban_count": player_info.active_ban_count,
            "active_ban_reason": player_info.active_ban_reason,
            // 风险动作（allow / warn / require_force / deny）与自动通过信息
            "risk_action": risk_action,
            "risk_label": risk_label,
            // 展示字段（供 QQ 模板直接渲染）
            "risk_display": risk_display,
            "ban_flags": ban_flags,
            "ban_reason": ban_reason,
            "detail_url": detail_url,
            "auto_approve_text": auto_approve_text,
            "auto_approve_enabled": auto_approve.enabled,
            "auto_approve_hours": auto_approve.hours,
            // 优先发给 LumiBot 配置的默认管理员；若配置了管理员 openid 则同时定向通知
            "openids": admin_openids,
        }),
    };

    let queued_id = enqueue_event(db, input).await?;
    tracing::info!(queued_id = %queued_id, "白名单申请事件已写入 LumiBot 异步队列");
    // 中/高风险申请（warning 级）在提交时即完成提醒，标记 review_notified_at，
    // 供自动通过循环幂等去重，避免后续重复提醒同一申请
    if notify_level == "warning" {
        mark_review_notified(db, &item.id).await?;
    }
    let _ = config; // 保留配置参数以兼容调用方；发送由后台任务决定是否启用。
    Ok(())
}

/// 标记白名单申请已完成人工审核提醒（幂等：仅在未标记时写入）。
async fn mark_review_notified(db: &Database, whitelist_id: &Uuid) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE whitelist_requests SET review_notified_at = now()
           WHERE id = $1 AND review_notified_at IS NULL"#,
    )
    .bind(whitelist_id)
    .execute(&db.pool)
    .await
    .context("更新白名单审核提醒标记失败")?;
    Ok(())
}

/// 白名单中/高风险人工审核提醒入队（warning 级别，会推送 QQ 提醒管理员）。
///
/// 调用场景：自动通过循环扫描到「等待时长已满」的 pending 申请时，若风险
/// 评级为中/高风险（或复核发现存在未解封封禁），则绝不自动通过，改为发送
/// 本提醒，确保管理员一定会通过 QQ 收到通知并手动审核。
///
/// 与提交时的 `report_whitelist_created` 互补：提交时若本地风险数据尚未
/// 同步（如全球封禁尚未拉取），申请会被判为低风险（info 级、不推送 QQ），
/// 由自动通过循环在超时时刻重新评级后补发本提醒。
/// 事件类型沿用 WHITELIST_REQUEST_CREATED，warning 级别命中 LumiBot 既有
/// 推送规则，消息文案明确「系统不会自动通过，等待管理员手动审核」。
pub async fn report_whitelist_pending_review(
    db: &Database,
    config: &Config,
    item: &WhitelistItem,
    hours: i64,
    risk_label: &str,
    risk_reason: Option<&str>,
) -> anyhow::Result<()> {
    let display_name = item.steam_persona_name.as_deref().unwrap_or(&item.nickname);
    let player_info = collect_whitelist_player_info(db, &item.steamid64).await;
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
    let risk_display = match risk_label {
        "低风险" => "🟢 低风险".to_string(),
        "历史风险" => "🟡 历史风险".to_string(),
        "高风险" => "🔴 高风险".to_string(),
        other => format!("⚠️ {other}"),
    };
    let mut ban_flags: Vec<&str> = Vec::new();
    if player_info.has_global_ban {
        ban_flags.push("❌ 全球封禁");
    }
    if player_info.has_local_ban {
        ban_flags.push("本地封禁");
    }
    if player_info.has_active_ban {
        ban_flags.push("未解封");
    }
    let ban_flags = if ban_flags.is_empty() {
        "无".to_string()
    } else {
        ban_flags.join(" / ")
    };
    let ban_reason = player_info
        .global_ban_reason
        .clone()
        .or_else(|| player_info.active_ban_reason.clone())
        .or_else(|| player_info.local_ban_reason.clone());
    let detail_url = config
        .admin_web_url
        .as_ref()
        .map(|base| format!("{base}/whitelist"))
        .or_else(|| item.profile_url.clone());
    let input = EventInput {
        event_type: EVENT_WHITELIST_REQUEST_CREATED.to_string(),
        level: "warning".to_string(),
        title: "白名单待人工审核提醒".to_string(),
        message: format!(
            "玩家 {}（{}）的白名单申请已等待 {} 小时无人审核，风险评级：{}。{}系统不会自动通过，请尽快人工审核",
            display_name,
            item.steamid64,
            hours,
            risk_display,
            risk_reason
                .map(|reason| format!("{}。", reason))
                .unwrap_or_default()
        ),
        data: serde_json::json!({
            "whitelist_id": item.id,
            "steamid64": item.steamid64,
            "steamid": item.steamid,
            "steamid3": item.steamid3,
            "nickname": item.nickname,
            "steam_persona_name": item.steam_persona_name,
            "nickname_show": display_name,
            "contact": item.contact,
            "profile_url": item.profile_url,
            "applied_at": item.applied_at,
            "has_local_ban": player_info.has_local_ban,
            "local_ban_count": player_info.local_ban_count,
            "local_ban_reason": player_info.local_ban_reason,
            "has_global_ban": player_info.has_global_ban,
            "global_ban_reason": player_info.global_ban_reason,
            "global_ban_reasons": player_info.global_ban_reasons,
            "has_active_ban": player_info.has_active_ban,
            "active_ban_count": player_info.active_ban_count,
            "active_ban_reason": player_info.active_ban_reason,
            "risk_display": risk_display,
            "ban_flags": ban_flags,
            "ban_reason": ban_reason,
            "detail_url": detail_url,
            "auto_approve_text": "不自动通过，等待管理员手动审核",
            "auto_approve_enabled": false,
            "openids": admin_openids,
        }),
    };

    let queued_id = enqueue_event(db, input).await?;
    mark_review_notified(db, &item.id).await?;
    tracing::info!(
        queued_id = %queued_id,
        whitelist_id = %item.id,
        "白名单中/高风险人工审核提醒已写入 LumiBot 异步队列"
    );
    Ok(())
}

/// 低风险白名单自动通过事件上报（info 级别，不做管理员通知，仅记录）。
/// 自动通过后调用，让 LumiBot 侧能够感知系统行为（通知规则由 LumiBot 决定）。
pub async fn report_whitelist_auto_approved(
    db: &Database,
    config: &Config,
    item: &WhitelistItem,
    hours: i64,
) -> anyhow::Result<()> {
    let display_name = item.steam_persona_name.as_deref().unwrap_or(&item.nickname);
    let input = EventInput {
        event_type: EVENT_WHITELIST_AUTO_APPROVED.to_string(),
        level: "info".to_string(),
        title: "白名单自动通过".to_string(),
        message: format!(
            "玩家 {}（{}）的低风险白名单申请已自动通过（等待 {} 小时无人审核）",
            display_name, item.steamid64, hours
        ),
        data: serde_json::json!({
            "whitelist_id": item.id,
            "steamid64": item.steamid64,
            "nickname": item.nickname,
            "steam_persona_name": item.steam_persona_name,
            "hours": hours,
            "approved_at": item.approved_at,
        }),
    };

    if !config.lumi_bot_enabled() {
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
            tracing::info!(event_id = %id, event_type = %input.event_type, "白名单自动通过事件已上报 LumiBot");
            Ok(())
        }
        Err(error) => {
            // 立即上报失败，降级入队由后台任务兜底重试
            tracing::warn!(%error, event_id = %id, "白名单自动通过事件立即上报失败，降级入队");
            enqueue_event(db, input).await?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// 后台定时异步发送任务
//
// 所有事件都先进入持久化队列，由本任务 claim 后发送、重试并记录死信，
// 避免 LumiBot 不可用时阻塞业务请求。
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

    super::task_runtime::spawn_persistent("lumi_bot_sync", move || {
        let db = db.clone();
        let config = config.clone();
        async move {
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

    sqlx::query(
        "UPDATE lumi_bot_event_queue SET status = 'pending', locked_at = NULL, locked_by = NULL WHERE status = 'pending' AND locked_at < now() - interval '5 minutes'",
    )
    .execute(&db.pool)
    .await?;

    let rows: Vec<QueuedEventRow> = sqlx::query_as(
        r#"WITH claimed AS (
             SELECT id
             FROM lumi_bot_event_queue
             WHERE status = 'pending' AND attempts < $1 AND next_attempt_at <= now()
             ORDER BY occurred_at ASC, queued_at ASC
             FOR UPDATE SKIP LOCKED
             LIMIT $2
           )
           UPDATE lumi_bot_event_queue q
           SET locked_at = now(), locked_by = pg_backend_pid()::text, updated_at = now()
           FROM claimed
           WHERE q.id = claimed.id
           RETURNING q.id, q.event_type, q.level, q.title, q.message, q.data, q.occurred_at"#,
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
                        attempts = attempts + 1,
                        last_error = NULL,
                        locked_at = NULL,
                        locked_by = NULL,
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
                        next_attempt_at = now() + make_interval(secs => LEAST(3600, power(2, attempts + 1)::int * 5)),
                        locked_at = NULL,
                        locked_by = NULL,
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

    /// 事件入队后由后台异步发送，事件不丢失
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

            // 请求只写入 pending 队列，后台任务负责重试
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

    /// 低风险玩家（无封禁、无风险关联）提交申请：level 应为 info，
    /// LumiBot 侧规则（>= warning 才推送）会过滤，不打扰管理员。
    #[tokio::test]
    async fn low_risk_whitelist_request_is_info_level() {
        with_test_db(async |db| {
            let mut config = Config::from_env();
            config.lumi_bot_api_url = None;
            config.lumi_bot_api_key = None;

            // 干净玩家：无任何封禁 / IP 关联记录 → RiskAction::Allow
            let item = test_whitelist_item("76561198000000001", "低风险玩家");

            report_whitelist_created(&db, &config, &item).await?;

            let (level, risk_action): (String, Option<String>) = sqlx::query_as(
                "SELECT level, data->>'risk_action' FROM lumi_bot_event_queue WHERE data->>'steamid64' = $1",
            )
            .bind("76561198000000001")
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(risk_action.as_deref(), Some("allow"));
            assert_eq!(level, "info", "低风险玩家申请应以 info 级别上报，避免 QQ 推送");
            Ok(())
        })
        .await;
    }

    /// 中高风险玩家（存在有效本地封禁）提交申请：level 应为 warning，
    /// 需要管理员人工审核，应推送 QQ 通知。
    #[tokio::test]
    async fn high_risk_whitelist_request_is_warning_level() {
        with_test_db(async |db| {
            let mut config = Config::from_env();
            config.lumi_bot_api_url = None;
            config.lumi_bot_api_key = None;

            let steamid64 = "76561198000000002";
            // 写入一条有效本地封禁 → RiskAction::Deny（高风险）
            sqlx::query(
                r#"INSERT INTO ban_records
                   (id, player, steam_id, status, operator_name, reason, created_at)
                   VALUES (gen_random_uuid(), '高风险玩家', $1, 'active', 'admin', '违规', now())"#,
            )
            .bind(steamid64)
            .execute(&db.pool)
            .await?;

            let item = test_whitelist_item(steamid64, "高风险玩家");
            report_whitelist_created(&db, &config, &item).await?;

            let (level, risk_action): (String, Option<String>) = sqlx::query_as(
                "SELECT level, data->>'risk_action' FROM lumi_bot_event_queue WHERE data->>'steamid64' = $1",
            )
            .bind(steamid64)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(risk_action.as_deref(), Some("deny"));
            assert_eq!(level, "warning", "高风险玩家申请应以 warning 级别上报，触发 QQ 推送");
            Ok(())
        })
        .await;
    }

    fn test_whitelist_item(steamid64: &str, nickname: &str) -> WhitelistItem {
        WhitelistItem {
            id: Uuid::new_v4(),
            steamid64: steamid64.to_string(),
            steamid: Some(format!("STEAM_1:0:{}", &steamid64[10..])),
            steamid3: Some(format!("[U:1:{}]", &steamid64[10..])),
            profile_url: None,
            nickname: nickname.to_string(),
            steam_persona_name: Some(nickname.to_string()),
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
        }
    }

    /// 即使 LumiBot 可达，请求也只写入队列，避免外部 HTTP 阻塞用户请求。
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

            // 请求返回时任务仍处于待发送状态，由后台 worker 异步处理。
            let (status, attempts): (String, i32) = sqlx::query_as(
                "SELECT status, attempts FROM lumi_bot_event_queue WHERE data->>'steamid64' = $1",
            )
            .bind("76561198000000002")
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(status, "pending");
            assert_eq!(attempts, 0);
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
            sqlx::query("UPDATE lumi_bot_event_queue SET next_attempt_at = now() WHERE id = $1")
                .bind(id)
                .execute(&db.pool)
                .await?;
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

    async fn insert_whitelist_request_row(db: &Database, steamid64: &str) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, status,
                applied_at, source, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, '高风险玩家', 'pending', now(), 'public', now())"#,
        )
        .bind(id)
        .bind(steamid64)
        .bind(steamid64)
        .bind(format!("STEAM_0:0:{}", &steamid64[..6]))
        .bind(format!("[U:1:{}]", &steamid64[..6]))
        .bind(format!("https://steamcommunity.com/profiles/{steamid64}"))
        .execute(&db.pool)
        .await?;
        Ok(id)
    }

    fn sample_whitelist_item(id: Uuid, steamid64: &str) -> WhitelistItem {
        WhitelistItem {
            id,
            steamid64: steamid64.to_string(),
            steamid: Some(format!("STEAM_0:0:{}", &steamid64[..6])),
            steamid3: Some(format!("[U:1:{}]", &steamid64[..6])),
            profile_url: Some(format!("https://steamcommunity.com/profiles/{steamid64}")),
            nickname: "高风险玩家".to_string(),
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
        }
    }

    #[tokio::test]
    async fn report_whitelist_pending_review_enqueues_warning_and_marks_notified() {
        with_test_db(async |db| {
            let steamid64 = "76561198000000050";
            let id = insert_whitelist_request_row(&db, steamid64).await?;
            let item = sample_whitelist_item(id, steamid64);
            let mut config = Config::from_env();
            config.lumi_bot_api_url = None;
            config.lumi_bot_api_key = None;

            report_whitelist_pending_review(&db, &config, &item, 3, "高风险", Some("存在未过期全球封禁")).await?;

            let (level, event_type, message): (String, String, String) = sqlx::query_as(
                r#"SELECT level, event_type, message FROM lumi_bot_event_queue
                   WHERE data->>'whitelist_id' = $1"#,
            )
            .bind(id.to_string())
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(level, "warning", "人工审核提醒必须是 warning 级（会推送 QQ）");
            assert_eq!(event_type, EVENT_WHITELIST_REQUEST_CREATED);
            assert!(message.contains("不会自动通过"), "实际：{message}");

            let notified: (bool,) = sqlx::query_as(
                "SELECT review_notified_at IS NOT NULL FROM whitelist_requests WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
            assert!(notified.0, "提醒入队后应标记 review_notified_at");
            Ok(())
        })
        .await;
    }
}
