//! 授权事件同步接口（LumiAuth Data Plane 面向游戏服）。
//!
//! 协议（Phase2 定稿，Long-Poll 首版）：
//! - `POST /api/plugin/auth/events/poll {report_token, port, after_version, wait_secs?}`
//!   后端 hold ≤20s，有新事件即返回；超时返回空数组。插件写库成功后再 ACK。
//! - `POST /api/plugin/auth/ack {report_token, port, version}` 单调 ACK。
//! - `GET /api/plugin/auth/snapshot` 全量（version + bans + whitelist + server rules）。
//! - 版本跳跃（`after_version < latest - 500`）时 poll 直接返回 `snapshot_required`，
//!   插件走 snapshot 恢复。

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use crate::routes::{invalid_request, AppCtx};
use crate::services::{auth_event_service, plugin_ban_service::ServerAuth};

#[derive(Deserialize)]
pub(crate) struct AuthEventsPollBody {
    pub report_token: String,
    pub port: i32,
    pub after_version: i64,
    /// 客户端期望等待秒数（上限 25s，默认 25s；后端实际 hold ≤20s）。
    #[serde(default)]
    pub wait_secs: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct AuthAckBody {
    pub report_token: String,
    pub port: i32,
    pub version: i64,
}

#[derive(Deserialize)]
pub(crate) struct AuthSnapshotQuery {
    pub report_token: String,
    pub port: i32,
}

async fn auth_server(
    db: &crate::db::Database,
    report_token: &str,
    port: i32,
) -> anyhow::Result<ServerAuth> {
    ServerAuth::authenticate(db, port, report_token).await
}

pub(crate) async fn poll_auth_events(
    State(ctx): State<AppCtx>,
    Json(body): Json<AuthEventsPollBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let server = auth_server(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(invalid_request)?;
    let after = body.after_version.max(0);

    // 快路径：已有新事件直接返回。
    let latest = auth_event_service::latest_version(&ctx.db)
        .await
        .map_err(invalid_request)?;
    if auth_event_service::needs_snapshot(latest, after) {
        auth_event_service::mark_seen(&ctx.db, server.id, true)
            .await
            .map_err(invalid_request)?;
        return Ok(Json(serde_json::json!({
            "events": [],
            "latest_version": latest,
            "snapshot_required": true,
        })));
    }
    let events = auth_event_service::events_after(&ctx.db, server.id, after, 100)
        .await
        .map_err(invalid_request)?;
    if !events.is_empty() {
        auth_event_service::mark_seen(&ctx.db, server.id, true)
            .await
            .map_err(invalid_request)?;
        return Ok(Json(serde_json::json!({
            "events": events,
            "latest_version": latest,
            "snapshot_required": false,
        })));
    }

    // Long-Poll：hold ≤20s，每 1s 检查一次新事件。
    let wait_secs = body.wait_secs.unwrap_or(25).clamp(1, 25);
    let hold_secs = wait_secs
        .min(auth_event_service::EVENTS_POLL_HOLD_SECS)
        .min(20);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(hold_secs);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let latest = auth_event_service::latest_version(&ctx.db)
            .await
            .map_err(invalid_request)?;
        if auth_event_service::needs_snapshot(latest, after) {
            auth_event_service::mark_seen(&ctx.db, server.id, true)
                .await
                .map_err(invalid_request)?;
            return Ok(Json(serde_json::json!({
                "events": [],
                "latest_version": latest,
                "snapshot_required": true,
            })));
        }
        let events = auth_event_service::events_after(&ctx.db, server.id, after, 100)
            .await
            .map_err(invalid_request)?;
        if !events.is_empty() || tokio::time::Instant::now() >= deadline {
            auth_event_service::mark_seen(&ctx.db, server.id, true)
                .await
                .map_err(invalid_request)?;
            return Ok(Json(serde_json::json!({
                "events": events,
                "latest_version": latest,
                "snapshot_required": false,
            })));
        }
    }
}

pub(crate) async fn ack_auth_version(
    State(ctx): State<AppCtx>,
    Json(body): Json<AuthAckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let server = auth_server(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(invalid_request)?;
    if body.version < 0 {
        return Err(invalid_request(anyhow::anyhow!("version 非法")));
    }
    let (acked, latest) = auth_event_service::ack_version(&ctx.db, server.id, body.version)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "acked_version": acked,
        "latest_version": latest,
    })))
}

/// 全量 Snapshot：version + bans + whitelist + server rules（插件本地整体替换）。
///
/// 直接按需构建（不依赖 access_snapshot 文件/TTL），保证版本落后时总能恢复。
pub(crate) async fn auth_snapshot(
    State(ctx): State<AppCtx>,
    Json(body): Json<AuthSnapshotQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let server = auth_server(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(invalid_request)?;

    let snapshot =
        crate::services::access_snapshot_service::refresh_snapshot(&ctx.db, &ctx.access_snapshot)
            .await
            .map_err(invalid_request)?;
    let item = crate::services::access_snapshot_service::snapshot_for_plugin(
        &snapshot,
        &body.report_token,
        body.port,
        chrono::Utc::now(),
    )
    .map_err(invalid_request)?;

    let latest = auth_event_service::latest_version(&ctx.db)
        .await
        .map_err(invalid_request)?;
    auth_event_service::mark_seen(&ctx.db, server.id, true)
        .await
        .map_err(invalid_request)?;
    // 标记一次 snapshot 同步时间（UI 的 LastSync 来源之一）。
    let _ = sqlx::query(
        r#"INSERT INTO auth_server_state (server_id, connection_status, last_seen_at, last_snapshot_at, updated_at)
           VALUES ($1, 'connected', now(), now(), now())
           ON CONFLICT (server_id) DO UPDATE SET
             connection_status = 'connected', last_seen_at = now(),
             last_snapshot_at = now(), updated_at = now()"#,
    )
    .bind(server.id)
    .execute(&ctx.db.pool)
    .await;

    Ok(Json(serde_json::json!({
        "server_id": server.id,
        "latest_version": latest,
        "item": item,
    })))
}
