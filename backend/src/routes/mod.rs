pub mod access;
pub mod auth;
pub mod ban;
pub mod ban_api;
pub mod ban_appeal;
pub mod community;
pub mod external_ban_api;
pub mod external_server;
pub mod map_sync;
pub mod misc;
pub mod notification;
pub mod player_detail;
pub mod player_report;
pub mod plugin;
pub mod public;
#[cfg(test)]
pub mod tests;
pub mod user;
pub mod whitelist;

use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
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
    models::{Operator, preferred_operator_name},
    services::{
        access_snapshot_service::SnapshotStore,
        notification_service::{NotificationHub, create_notification_hub},
        r2_storage::R2Storage,
        server_config_cache::ServerConfigCache,
        steam_service::SteamResolver,
    },
};

// ---------------------------------------------------------------------------
// Shared context
// ---------------------------------------------------------------------------

type GlobalBansCache = Arc<RwLock<HashMap<String, (serde_json::Value, chrono::DateTime<Utc>)>>>;
type GokzStatsCache = Arc<RwLock<HashMap<String, (serde_json::Value, chrono::DateTime<Utc>)>>>;

#[derive(Clone)]
pub struct AppCtx {
    pub config: Config,
    pub db: Database,
    pub access_snapshot: SnapshotStore,
    pub global_bans_cache: GlobalBansCache,
    pub gokz_stats_cache: GokzStatsCache,
    pub server_config_cache: Arc<ServerConfigCache>,
    pub steam_resolver: SteamResolver,
    pub notification_hub: NotificationHub,
    pub r2_storage: Option<R2Storage>,
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
            if trimmed.is_empty() {
                return None;
            }
            Some(format!(
                "%{}%",
                trimmed.replace('%', "\\%").replace('_', "\\_")
            ))
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
        .route("/api/review-counts", get(misc::review_counts))
        // -- map sync --
        .route(
            "/api/map-sync/config",
            get(map_sync::get_map_sync_overview).put(map_sync::update_map_sync_config),
        )
        .route("/api/map-sync/agents", post(map_sync::create_map_sync_agent))
        .route(
            "/api/map-sync/agents/:id",
            delete(map_sync::delete_map_sync_agent),
        )
        .route(
            "/api/map-sync/agents/:id/reset-token",
            post(map_sync::reset_map_sync_agent_token),
        )
        .route("/api/map-sync/check", post(map_sync::check_all_maps))
        .route(
            "/api/map-sync/maps/:map_name/sync",
            post(map_sync::sync_single_map),
        )
        .route(
            "/api/map-sync/agent/inventory",
            post(map_sync::agent_inventory),
        )
        .route("/api/map-sync/agent/tasks", get(map_sync::agent_tasks))
        .route(
            "/api/map-sync/agent/map-pool",
            get(map_sync::agent_map_pool),
        )
        .route(
            "/api/map-sync/agent/tasks/:task_id/report",
            post(map_sync::agent_task_report),
        )
        // -- community --
        .route("/api/community/servers", get(community::community_servers))
        .route(
            "/api/community/groups",
            post(community::create_community_group),
        )
        .route(
            "/api/community/groups/:group_id",
            delete(community::delete_community_group),
        )
        .route(
            "/api/community/groups/:group_id/servers",
            post(community::create_community_server),
        )
        .route(
            "/api/community/groups/:group_id/access",
            put(community::update_community_access),
        )
        .route(
            "/api/community/servers/test-rcon",
            post(community::test_server_rcon),
        )
        .route(
            "/api/community/servers/:server_id",
            put(community::update_community_server).delete(community::delete_community_server),
        )
        .route(
            "/api/community/servers/:server_id/players",
            get(community::get_online_players),
        )
        .route(
            "/api/community/servers/:server_id/report-token",
            get(community::get_server_report_token),
        )
        .route(
            "/api/community/servers/:server_id/report-token/reset",
            post(community::reset_server_report_token),
        )
        .route(
            "/api/community/servers/:server_id/rcon",
            post(community::execute_rcon),
        )
        // -- plugin --
        .route(
            "/api/plugin/online-players/report",
            post(community::report_plugin_online_players),
        )
        .route("/api/plugin/bans", post(ban::create_plugin_ban))
        .route("/api/plugin/bans/poll", post(ban::poll_plugin_bans))
        .route(
            "/api/plugin/bans/poll/incremental",
            post(ban::poll_plugin_bans_incremental),
        )
        .route("/api/plugin/bans/unban", post(ban::unban_plugin_ban))
        .route("/api/plugin/bans/check", post(ban::check_plugin_ban))
        .route(
            "/api/plugin/access/check",
            post(access::check_plugin_access),
        )
        .route(
            "/api/plugin/access/snapshot",
            post(access::plugin_access_snapshot),
        )
        .route(
            "/api/plugin/server-status",
            post(plugin::report_server_status),
        )
        .route(
            "/api/plugin/offline/sync",
            post(plugin::sync_offline_operations),
        )
        // -- external servers --
        .route(
            "/api/external-servers",
            get(external_server::list_external_servers)
                .post(external_server::create_external_server),
        )
        .route(
            "/api/external-servers/:id",
            put(external_server::update_external_server)
                .delete(external_server::delete_external_server),
        )
        .route(
            "/api/external-servers/:id/test",
            post(external_server::test_external_server),
        )
        // -- external ban api --
        .route(
            "/api/external-ban-api/targets",
            get(external_ban_api::list_targets).post(external_ban_api::create_target),
        )
        .route(
            "/api/external-ban-api/targets/:id",
            put(external_ban_api::update_target).delete(external_ban_api::delete_target),
        )
        .route(
            "/api/external-ban-api/targets/:id/test",
            post(external_ban_api::test_target),
        )
        .route(
            "/api/bans/:id/sync-external",
            post(external_ban_api::sync_ban),
        )
        .route(
            "/api/bans/:ban_id/sync-external/:target_id",
            post(external_ban_api::sync_ban_to_target),
        )
        // -- player api --
        .route("/api/player-api/players", get(plugin::player_api_players))
        .route(
            "/api/player-api/config",
            get(plugin::get_player_api_config).put(plugin::update_player_api_config),
        )
        .route("/webhook/:public_path", get(plugin::webhook_public))
        // -- player detail --
        .route("/api/player-detail", get(player_detail::get_player_detail))
        .route(
            "/api/player-detail/internal/:steamid64",
            put(player_detail::update_player_internal_profile),
        )
        .route(
            "/api/player-detail/evidence/:source_type/:file_id",
            put(player_detail::update_evidence_metadata),
        )
        // -- whitelist --
        .route("/api/whitelist", get(whitelist::whitelist))
        .route("/api/whitelist/manual", post(whitelist::create_whitelist))
        .route(
            "/api/whitelist/:id/approve",
            post(whitelist::approve_whitelist_request),
        )
        .route(
            "/api/whitelist/:id/reject",
            post(whitelist::reject_whitelist_request),
        )
        .route(
            "/api/whitelist/:id/restore",
            post(whitelist::restore_whitelist_request),
        )
        .route(
            "/api/whitelist/:id/revoke",
            post(whitelist::revoke_whitelist_request),
        )
        .route(
            "/api/whitelist/:id/refresh-steam-name",
            post(whitelist::refresh_single_steam_name),
        )
        .route(
            "/api/whitelist/refresh-steam-names",
            post(whitelist::refresh_all_steam_names),
        )
        // -- bans --
        .route("/api/bans", get(ban::bans).post(ban::create_ban))
        .route(
            "/api/bans/:id",
            get(ban::get_ban)
                .put(ban::update_ban)
                .delete(ban::delete_ban),
        )
        .route("/api/bans/:id/unban", post(ban::unban_ban))
        .route(
            "/api/bans/:id/files",
            get(ban::list_ban_files).post(ban::upload_ban_files),
        )
        .route("/api/bans/files/:file_id/url", get(ban::get_ban_file_url))
        .route(
            "/api/ban-api/keys",
            get(ban_api::list_keys).post(ban_api::create_key),
        )
        .route("/api/ban-api/keys/:id", delete(ban_api::delete_key))
        .route(
            "/api/integration/bans",
            get(ban_api::integration_bans).post(ban_api::create_integration_ban),
        )
        .route(
            "/api/integration/bans/check",
            post(ban_api::check_integration_ban),
        )
        // -- ban appeals --
        .route("/api/ban-appeals", get(ban_appeal::list_appeals))
        .route(
            "/api/ban-appeals/:id/approve",
            post(ban_appeal::approve_appeal),
        )
        .route(
            "/api/ban-appeals/:id/reject",
            post(ban_appeal::reject_appeal),
        )
        .route(
            "/api/ban-appeals/:id/files",
            get(ban_appeal::list_appeal_files),
        )
        .route(
            "/api/ban-appeals/files/:file_id/url",
            get(ban_appeal::get_appeal_file_url),
        )
        // -- player reports --
        .route("/api/player-reports", get(player_report::list_reports))
        .route(
            "/api/player-reports/:id/review",
            post(player_report::review_report),
        )
        .route(
            "/api/player-reports/:id/ban",
            post(player_report::ban_report),
        )
        .route(
            "/api/player-reports/:id/files",
            get(player_report::list_report_files),
        )
        // -- audit --
        .route("/api/audit/logs", get(misc::list_audit_logs))
        // -- users --
        .route("/api/users", get(user::users).post(user::create_user))
        .route(
            "/api/users/:id",
            put(user::update_user).delete(user::delete_user),
        )
        .route("/api/users/:id/password", put(user::update_user_password))
        .route(
            "/api/users/:id/toggle-enabled",
            post(user::toggle_user_enabled),
        )
        .route(
            "/api/auth/users/:user_id/sessions",
            delete(auth::revoke_user_sessions),
        )
        // -- logs --
        .route("/api/logs", get(misc::logs))
        // -- docs --
        .route("/api/docs/endpoints", get(misc::api_endpoint_docs))
        // -- player access rules --
        .route(
            "/api/player-access/rules",
            get(access::player_access_rules).post(access::create_player_access_rule),
        )
        .route(
            "/api/player-access/rules/:id",
            put(access::update_player_access_rule).delete(access::delete_player_access_rule),
        )
        // -- notifications --
        .route("/api/notifications", get(notification::list_notifications))
        .route(
            "/api/notifications/unread-count",
            get(notification::unread_count),
        )
        .route("/api/notifications/:id/read", post(notification::mark_read))
        .route(
            "/api/notifications/read-all",
            post(notification::mark_all_read),
        )
        .route("/ws/notifications", get(notification::ws_handler))
        // -- public --
        .route(
            "/api/public/whitelist",
            get(public::public_whitelist).post(public::submit_whitelist),
        )
        .route("/api/public/bans", get(public::public_bans))
        .route("/api/public/steam/resolve", post(public::resolve_steam))
        .route("/api/public/bans/query", post(public::query_active_bans))
        .route(
            "/api/public/ban-appeals",
            get(public::public_ban_appeals_info).post(public::submit_ban_appeal),
        )
        .route(
            "/api/public/ban-appeals/",
            get(public::public_ban_appeals_info).post(public::submit_ban_appeal),
        )
        .route(
            "/api/public/ban-appeals/query",
            post(public::query_appeal_status),
        )
        .route(
            "/api/public/ban-appeals/submit",
            post(public::submit_ban_appeal),
        )
        .route(
            "/api/public/ban-appeals/:id/files",
            post(public::upload_appeal_files),
        )
        .route(
            "/api/public/player-reports",
            post(player_report::submit_player_report),
        )
        .route(
            "/api/public/player-reports/:id/files",
            post(player_report::upload_player_report_files),
        )
        .route(
            "/api/public/global-bans/:steamid64",
            get(public::get_global_bans),
        )
        .route(
            "/api/public/global-bans/batch",
            post(public::get_global_bans_batch),
        )
        .route(
            "/api/public/gokz/player-stats/:steamid64",
            get(public::get_gokz_player_stats),
        )
        .route(
            "/api/public/gokz/player-stats/batch",
            post(public::get_gokz_player_stats_batch),
        )
        .with_state(AppCtx {
            config: config.clone(),
            db,
            access_snapshot,
            global_bans_cache: Arc::new(RwLock::new(HashMap::new())),
            gokz_stats_cache: Arc::new(RwLock::new(HashMap::new())),
            server_config_cache,
            steam_resolver,
            notification_hub: create_notification_hub(),
            r2_storage: R2Storage::new(&config),
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
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing token" })),
        ))?;

    let token = Uuid::parse_str(token).map_err(|e| {
        tracing::warn!(error = %e, "token parse failed");
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid token" })),
        )
    })?;

    let session = crate::services::auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "session lookup failed");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            )
        })?;

    let row: Option<(Uuid, String, Option<String>, String, bool)> =
        sqlx::query_as(r#"SELECT id, username, remark, role, enabled FROM users WHERE id = $1"#)
            .bind(session.user_id)
            .fetch_optional(&ctx.db.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "查询用户信息失败");
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "unauthorized" })),
                )
            })?;

    let (id, username, remark, role, enabled) = row.ok_or((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized" })),
    ))?;

    if !enabled {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "账号已禁用" })),
        ));
    }

    Ok(Operator {
        id,
        display_name: preferred_operator_name(&username, remark.as_deref()),
        username,
        role,
    })
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

pub(crate) fn forbidden() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "权限不足" })),
    )
}

pub(crate) fn invalid_request(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    let friendly_msg = translate_db_error(&error);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": friendly_msg })),
    )
}

/// 使用 sqlx::DatabaseError trait 进行类型安全的错误匹配
pub(crate) fn translate_db_error(error: &anyhow::Error) -> String {
    // 尝试提取 sqlx::Error
    if let Some(sqlx_err) = error.downcast_ref::<sqlx::Error>() {
        match sqlx_err {
            sqlx::Error::RowNotFound => return "记录不存在".to_string(),
            sqlx::Error::Database(db_err) => {
                // 使用 constraint() 和 message() 进行匹配
                let constraint = db_err.constraint().unwrap_or("");
                let message = db_err.message();

                if message.contains("duplicate key") || message.contains("unique constraint") {
                    if constraint.contains("users_username_key") || constraint.contains("username") {
                        return "该用户名已存在，请更换其他用户名".to_string();
                    }
                    if constraint.contains("steam_id") || constraint.contains("steamid64") {
                        return "该 SteamID 已存在".to_string();
                    }
                    if constraint.contains("report_token") {
                        return "服务器令牌已存在".to_string();
                    }
                    return "该记录已存在，无法重复创建".to_string();
                }
                if message.contains("foreign key") {
                    return "关联数据不存在".to_string();
                }
                if message.contains("check constraint") {
                    return "数据格式不符合要求".to_string();
                }
                if message.contains("not-null constraint") || message.contains("null value") {
                    return "必填字段不能为空".to_string();
                }
            }
            _ => {}
        }
    }

    // 回退到字符串匹配（兼容非 sqlx 错误）
    let msg = error.to_string();
    if msg.contains("not found") || msg.contains("不存在") {
        return "记录不存在".to_string();
    }

    msg
}

pub(crate) fn invalid_request_status(error: anyhow::Error) -> StatusCode {
    tracing::warn!(error = %error, "无效请求");
    StatusCode::BAD_REQUEST
}

pub(crate) fn internal_error(error: anyhow::Error) -> StatusCode {
    tracing::error!(error = %error, "内部服务器错误");
    StatusCode::INTERNAL_SERVER_ERROR
}

// ---------------------------------------------------------------------------
// 统一错误类型
// ---------------------------------------------------------------------------

/// 应用统一错误类型，所有路由 handler 应使用此类型返回错误。
/// 自动实现 `IntoResponse`，保证所有错误响应均为 `{"error": "..."}` JSON 格式。
pub(crate) enum AppError {
    /// 401 未授权
    Unauthorized(String),
    /// 403 禁止访问
    Forbidden,
    /// 404 资源不存在
    NotFound(String),
    /// 400 请求无效（含数据库错误翻译）
    BadRequest(String),
    /// 500 内部服务器错误
    Internal(String),
}

impl AppError {
    /// 构造 403 权限不足错误
    pub(crate) fn forbidden() -> Self {
        Self::Forbidden
    }

    /// 从 `anyhow::Error` 构造 400 错误，自动翻译数据库错误
    pub(crate) fn bad_request(error: anyhow::Error) -> Self {
        Self::BadRequest(translate_db_error(&error))
    }

    /// 从 `anyhow::Error` 构造 500 错误
    pub(crate) fn internal(error: anyhow::Error) -> Self {
        tracing::error!(error = %error, "内部服务器错误");
        Self::Internal(error.to_string())
    }

    /// 构造 404 错误
    pub(crate) fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::Forbidden => (StatusCode::FORBIDDEN, "权限不足".to_string()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
