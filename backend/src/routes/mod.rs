pub mod auth;
pub mod community;
pub mod external_server;
pub mod ban;
pub mod ban_appeal;
pub mod whitelist;
pub mod user;
pub mod public;
pub mod access;
pub mod plugin;
pub mod misc;
pub mod notification;
#[cfg(test)]
pub mod tests;

use axum::{
    http::{header, HeaderMap, StatusCode},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    models::Operator,
    services::{
        access_snapshot_service::SnapshotStore,
        notification_service::{NotificationHub, create_notification_hub},
        server_config_cache::ServerConfigCache,
        steam_service::SteamResolver,
    },
};

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

type GlobalBansCache = Arc<RwLock<HashMap<String, (serde_json::Value, chrono::DateTime<Utc>)>>>;

#[derive(Clone)]
pub struct AppCtx {
    pub config: Config,
    pub db: Database,
    pub access_snapshot: SnapshotStore,
    pub global_bans_cache: GlobalBansCache,
    pub server_config_cache: Arc<ServerConfigCache>,
    pub steam_resolver: SteamResolver,
    pub notification_hub: NotificationHub,
}

// ---------------------------------------------------------------------------
// Shared query / response types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
pub struct ListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl ListQuery {
    pub fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn page_size(&self) -> i64 {
        self.page_size.unwrap_or(20).clamp(1, 100)
    }

    pub fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }

    pub fn search_pattern(&self) -> Option<String> {
        self.search.as_ref().and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() { return None; }
            Some(format!("%{}%", trimmed.replace('%', "\\%").replace('_', "\\_")))
        })
    }
}

#[derive(serde::Serialize)]
pub struct PaginatedResponse<T: serde::Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(
    config: Config,
    db: Database,
    access_snapshot: SnapshotStore,
    server_config_cache: Arc<ServerConfigCache>,
    steam_resolver: SteamResolver,
) -> Router {
    Router::new()
        .route("/health", get(misc::health_check))
        // -- auth --
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/logout-all", post(auth::logout_all_devices))
        .route("/api/auth/me", get(auth::me))
        // -- dashboard --
        .route("/api/dashboard", get(misc::dashboard))
        // -- community --
        .route("/api/community/servers", get(community::community_servers))
        .route("/api/community/groups", post(community::create_community_group))
        .route("/api/community/groups/:group_id", delete(community::delete_community_group))
        .route("/api/community/groups/:group_id/servers", post(community::create_community_server))
        .route("/api/community/groups/:group_id/access", put(community::update_community_access))
        .route("/api/community/servers/test-rcon", post(community::test_server_rcon))
        .route(
            "/api/community/servers/:server_id",
            put(community::update_community_server).delete(community::delete_community_server),
        )
        .route("/api/community/servers/:server_id/players", get(community::get_online_players))
        .route("/api/community/servers/:server_id/report-token", get(community::get_server_report_token))
        .route("/api/community/servers/:server_id/report-token/reset", post(community::reset_server_report_token))
        .route("/api/community/servers/:server_id/rcon", post(community::execute_rcon))
        // -- plugin --
        .route("/api/plugin/online-players/report", post(community::report_plugin_online_players))
        .route("/api/plugin/bans", post(ban::create_plugin_ban))
        .route("/api/plugin/bans/poll", post(ban::poll_plugin_bans))
        .route("/api/plugin/bans/poll/incremental", post(ban::poll_plugin_bans_incremental))
        .route("/api/plugin/bans/unban", post(ban::unban_plugin_ban))
        .route("/api/plugin/bans/check", post(ban::check_plugin_ban))
        .route("/api/plugin/access/check", post(access::check_plugin_access))
        .route("/api/plugin/access/snapshot", post(access::plugin_access_snapshot))
        .route("/api/plugin/server-status", post(plugin::report_server_status))
        .route("/api/plugin/offline/sync", post(plugin::sync_offline_operations))
        // -- external servers --
        .route("/api/external-servers", get(external_server::list_external_servers).post(external_server::create_external_server))
        .route("/api/external-servers/:id", put(external_server::update_external_server).delete(external_server::delete_external_server))
        .route("/api/external-servers/:id/test", post(external_server::test_external_server))
        // -- player api --
        .route("/api/player-api/players", get(plugin::player_api_players))
        .route("/api/player-api/config", get(plugin::get_player_api_config).put(plugin::update_player_api_config))
        .route("/webhook/:public_path", get(plugin::webhook_public))
        // -- whitelist --
        .route("/api/whitelist", get(whitelist::whitelist))
        .route("/api/whitelist/manual", post(whitelist::create_whitelist))
        .route("/api/whitelist/:id/approve", post(whitelist::approve_whitelist_request))
        .route("/api/whitelist/:id/reject", post(whitelist::reject_whitelist_request))
        .route("/api/whitelist/:id/restore", post(whitelist::restore_whitelist_request))
        .route("/api/whitelist/:id/revoke", post(whitelist::revoke_whitelist_request))
        .route("/api/whitelist/:id/refresh-steam-name", post(whitelist::refresh_single_steam_name))
        .route("/api/whitelist/refresh-steam-names", post(whitelist::refresh_all_steam_names))
        // -- bans --
        .route("/api/bans", get(ban::bans).post(ban::create_ban))
        .route("/api/bans/:id", put(ban::update_ban).delete(ban::delete_ban))
        .route("/api/bans/:id/unban", post(ban::unban_ban))
        // -- ban appeals --
        .route("/api/ban-appeals", get(ban_appeal::list_appeals))
        .route("/api/ban-appeals/:id/approve", post(ban_appeal::approve_appeal))
        .route("/api/ban-appeals/:id/reject", post(ban_appeal::reject_appeal))
        // -- audit --
        .route("/api/audit/logs", get(misc::list_audit_logs))
        // -- users --
        .route("/api/users", get(user::users).post(user::create_user))
        .route("/api/users/:id", put(user::update_user).delete(user::delete_user))
        .route("/api/users/:id/password", put(user::update_user_password))
        .route("/api/users/:id/toggle-enabled", post(user::toggle_user_enabled))
        .route("/api/auth/users/:user_id/sessions", delete(auth::revoke_user_sessions))
        // -- logs --
        .route("/api/logs", get(misc::logs))
        // -- docs --
        .route("/api/docs/endpoints", get(misc::api_endpoint_docs))
        // -- player access rules --
        .route("/api/player-access/rules", get(access::player_access_rules).post(access::create_player_access_rule))
        .route("/api/player-access/rules/:id", put(access::update_player_access_rule).delete(access::delete_player_access_rule))
        // -- notifications --
        .route("/api/notifications", get(notification::list_notifications))
        .route("/api/notifications/unread-count", get(notification::unread_count))
        .route("/api/notifications/:id/read", post(notification::mark_read))
        .route("/api/notifications/read-all", post(notification::mark_all_read))
        .route("/ws/notifications", get(notification::ws_handler))
        // -- public --
        .route("/api/public/whitelist", get(public::public_whitelist).post(public::submit_whitelist))
        .route("/api/public/bans", get(public::public_bans))
        .route("/api/public/steam/resolve", post(public::resolve_steam))
        .route("/api/public/bans/query", post(public::query_active_bans))
        .route("/api/public/ban-appeals", post(public::submit_ban_appeal))
        .route("/api/public/global-bans/:steamid64", get(public::get_global_bans))
        .route("/api/public/global-bans/batch", post(public::get_global_bans_batch))
        .with_state(AppCtx {
            config,
            db,
            access_snapshot,
            global_bans_cache: Arc::new(RwLock::new(HashMap::new())),
            server_config_cache,
            steam_resolver,
            notification_hub: create_notification_hub(),
        })
}

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------

pub(crate) async fn current_operator(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<Operator, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "missing token" }))))?;

    let token = Uuid::parse_str(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid token" }))))?;

    let session = crate::services::auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))))?;

    let row: Option<(Uuid, String, String, String, bool)> = sqlx::query_as(
        r#"SELECT id, username, display_name, role, enabled FROM users WHERE id = $1"#,
    )
    .bind(session.user_id)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))))?;

    let (id, username, display_name, role, enabled) = row
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))))?;

    if !enabled {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "账号已禁用" }))));
    }

    Ok(Operator { id, username, display_name, role })
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

pub(crate) fn forbidden() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "权限不足" })))
}

pub(crate) fn invalid_request(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    let error_msg = error.to_string();
    let friendly_msg = translate_db_error(&error_msg);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": friendly_msg })),
    )
}

pub(crate) fn translate_db_error(msg: &str) -> String {
    if msg.contains("duplicate key value violates unique constraint") {
        if msg.contains("users_username_key") || msg.contains("username") {
            return "该用户名已存在，请更换其他用户名".to_string();
        }
        if msg.contains("steam_id") || msg.contains("steamid64") {
            return "该 SteamID 已存在".to_string();
        }
        if msg.contains("report_token") {
            return "服务器令牌已存在".to_string();
        }
        return "该记录已存在，无法重复创建".to_string();
    }
    if msg.contains("violates foreign key constraint") {
        return "关联数据不存在".to_string();
    }
    if msg.contains("violates check constraint") {
        return "数据格式不符合要求".to_string();
    }
    if msg.contains("violates not-null constraint") {
        return "必填字段不能为空".to_string();
    }
    if msg.contains("no rows returned") || msg.contains("not found") {
        return "记录不存在".to_string();
    }

    msg.to_string()
}

pub(crate) fn invalid_request_status(error: anyhow::Error) -> StatusCode {
    let _ = error;
    StatusCode::BAD_REQUEST
}

pub(crate) fn internal_error(error: anyhow::Error) -> StatusCode {
    let _ = error;
    StatusCode::INTERNAL_SERVER_ERROR
}
