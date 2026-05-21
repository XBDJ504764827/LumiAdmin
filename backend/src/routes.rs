use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    models::User,
    services::{
        access_service, access_snapshot_service::{self, SnapshotStore}, audit_service, auth_service, ban_service, community_service, dashboard_service, docs_service, log_service, offline_sync_service, permission_service,
        player_access_rule_service, player_api_service, plugin_ban_service, public_service, server_config_cache::{self, ServerConfigCache}, server_status_service, user_service, whitelist_service,
    },
};

#[derive(Clone)]
pub struct AppCtx {
    pub config: Config,
    pub db: Database,
    pub access_snapshot: SnapshotStore,
    pub global_bans_cache: Arc<RwLock<HashMap<String, (serde_json::Value, chrono::DateTime<Utc>)>>>,
    pub server_config_cache: Arc<ServerConfigCache>,
}

pub fn router(config: Config, db: Database, access_snapshot: SnapshotStore, server_config_cache: Arc<ServerConfigCache>) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(serde_json::json!({ "ok": true })) }))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/logout-all", post(logout_all_devices))
        .route("/api/auth/me", get(me))
        .route("/api/dashboard", get(dashboard))
        .route("/api/community/servers", get(community_servers))
        .route("/api/community/groups", post(create_community_group))
        .route("/api/community/groups/:group_id", delete(delete_community_group))
        .route("/api/community/groups/:group_id/servers", post(create_community_server))
        .route("/api/community/groups/:group_id/access", put(update_community_access))
        .route("/api/community/servers/test-rcon", post(test_server_rcon))
        .route(
            "/api/community/servers/:server_id",
            put(update_community_server).delete(delete_community_server),
        )
        .route("/api/community/servers/:server_id/players", get(get_online_players))
        .route("/api/plugin/online-players/report", post(report_plugin_online_players))
        .route("/api/player-api/players", get(player_api_players))
        .route("/api/player-api/config", get(get_player_api_config).put(update_player_api_config))
        .route("/webhook/:public_path", get(webhook_public))
        .route("/api/community/servers/:server_id/report-token", get(get_server_report_token))
        .route("/api/community/servers/:server_id/report-token/reset", post(reset_server_report_token))
        .route("/api/community/servers/:server_id/rcon", post(execute_rcon))
        .route("/api/whitelist", get(whitelist))
        .route("/api/whitelist/manual", post(create_whitelist))
        .route("/api/whitelist/:id/approve", post(approve_whitelist_request))
        .route("/api/whitelist/:id/reject", post(reject_whitelist_request))
        .route("/api/whitelist/:id/restore", post(restore_whitelist_request))
        .route("/api/whitelist/:id/revoke", post(revoke_whitelist_request))
        .route("/api/whitelist/:id/refresh-steam-name", post(refresh_single_steam_name))
        .route("/api/whitelist/refresh-steam-names", post(refresh_all_steam_names))
        .route("/api/bans", get(bans).post(create_ban))
        .route("/api/bans/:id", put(update_ban).delete(delete_ban))
        .route("/api/bans/:id/unban", post(unban_ban))
        .route("/api/plugin/bans", post(create_plugin_ban))
        .route("/api/plugin/bans/poll", post(poll_plugin_bans))
        .route("/api/plugin/bans/poll/incremental", post(poll_plugin_bans_incremental))
        .route("/api/plugin/bans/unban", post(unban_plugin_ban))
        .route("/api/plugin/bans/check", post(check_plugin_ban))
        .route("/api/plugin/access/check", post(check_plugin_access))
        .route("/api/plugin/access/snapshot", post(plugin_access_snapshot))
        .route("/api/plugin/server-status", post(report_server_status))
        .route("/api/plugin/offline/sync", post(sync_offline_operations))
        .route("/api/audit/logs", get(list_audit_logs))
        .route("/api/users", get(users).post(create_user))
        .route("/api/users/:id", put(update_user).delete(delete_user))
        .route("/api/users/:id/password", put(update_user_password))
        .route("/api/users/:id/toggle-enabled", post(toggle_user_enabled))
        .route("/api/auth/users/:user_id/sessions", delete(revoke_user_sessions))
        .route("/api/logs", get(logs))
        .route("/api/docs/endpoints", get(api_endpoint_docs))
        .route("/api/player-access/rules", get(player_access_rules).post(create_player_access_rule))
        .route("/api/player-access/rules/:id", put(update_player_access_rule).delete(delete_player_access_rule))
        .route("/api/public/whitelist", get(public_whitelist).post(submit_whitelist))
        .route("/api/public/bans", get(public_bans))
        .route("/api/public/steam/resolve", post(resolve_steam))
        .route("/api/public/global-bans/:steamid64", get(get_global_bans))
        .route("/api/public/global-bans/batch", post(get_global_bans_batch))
        .with_state(AppCtx {
            config,
            db,
            access_snapshot,
            global_bans_cache: Arc::new(RwLock::new(HashMap::new())),
            server_config_cache,
        })
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct WhitelistBody {
    steam_input: String,
    nickname: String,
    operator_name: Option<String>,
}

#[derive(Deserialize)]
struct RejectWhitelistBody {
    reason: String,
    operator_name: Option<String>,
}

#[derive(Deserialize)]
struct WhitelistActionBody {
    operator_name: Option<String>,
}

#[derive(Deserialize)]
struct CreateUserBody {
    username: String,
    password: String,
    role: String,
    steam_id: Option<String>,
    remark: Option<String>,
}

#[derive(Deserialize)]
struct UpdateUserBody {
    username: String,
    role: Option<String>,
    steam_id: Option<String>,
    remark: Option<String>,
}

#[derive(Deserialize)]
struct UpdatePasswordBody {
    password: String,
}

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
        self.search.as_ref().map(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() { return None; }
            Some(format!("%{}%", trimmed.replace('%', "\\%").replace('_', "\\_")))
        }).flatten()
    }
}

#[derive(serde::Serialize)]
pub struct PaginatedResponse<T: serde::Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Deserialize)]
struct BanBody {
    player: Option<String>,
    steam_id: String,
    ban_type: String,
    ip_address: Option<String>,
    reason: String,
}

#[derive(Deserialize)]
struct PluginBanBody {
    report_token: String,
    port: i32,
    ban_type: String,
    steam_id: Option<String>,
    ip_address: Option<String>,
    player: Option<String>,
    duration_minutes: i32,
    reason: String,
    operator_name: String,
}

#[derive(Deserialize)]
struct PluginBanPollBody {
    report_token: String,
    port: i32,
}

#[derive(Deserialize)]
struct PluginUnbanBody {
    report_token: String,
    port: i32,
    target: String,
    reason: Option<String>,
    operator_name: String,
    operator_steamid: Option<String>,
}

#[derive(Deserialize)]
struct PluginBanCheckBody {
    report_token: String,
    port: i32,
    steam_id: Option<String>,
    ip_address: Option<String>,
    player: Option<String>,
    server_port: Option<i32>,
}

#[derive(Deserialize)]
struct PluginAccessCheckBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    server_port: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct PluginAccessSnapshotBody {
    report_token: String,
    port: i32,
}

async fn login(
    State(ctx): State<AppCtx>,
    Json(body): Json<LoginBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let resp = auth_service::login(&ctx.db, &body.username, &body.password, ctx.config.session_ttl_hours)
        .await
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error":"invalid credentials"})),
            )
        })?;
    Ok(Json(serde_json::json!({"session": resp.session})))
}

async fn logout(State(ctx): State<AppCtx>, headers: HeaderMap) -> StatusCode {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        if let Ok(token) = Uuid::parse_str(token) {
            let _ = auth_service::logout(&ctx.db, token).await;
        }
    }
    StatusCode::NO_CONTENT
}

async fn me(State(ctx): State<AppCtx>, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token = Uuid::parse_str(token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let session = auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(Json(serde_json::json!({"session": session})))
}

#[derive(Deserialize)]
struct LogoutAllBody {
    current_token: Uuid,
}

/// 登出当前用户的所有其他设备（保留当前 session）
async fn logout_all_devices(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<LogoutAllBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 验证当前 session 有效
    let session = auth_service::current_session(&ctx.db, body.current_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let count = auth_service::logout_all_for_user(&ctx.db, session.user_id, Some(body.current_token))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "revoked_count": count })))
}

/// 管理员强制登出指定用户的所有设备
async fn revoke_user_sessions(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let count = auth_service::logout_all_for_user(&ctx.db, user_id, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "操作失败"}))))?;

    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "强制登出", &user_id.to_string(), &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({ "revoked_count": count })))
}

async fn dashboard(State(ctx): State<AppCtx>) -> Result<Json<serde_json::Value>, StatusCode> {
    let data = dashboard_service::get_metrics(&ctx.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"data": data})))
}

async fn community_servers(State(ctx): State<AppCtx>) -> Result<Json<serde_json::Value>, StatusCode> {
    let groups = community_service::list_groups(&ctx.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"groups": groups})))
}

async fn create_community_group(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<community_service::CreateCommunityInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let group = community_service::create_group(&ctx.db, body)
        .await
        .map_err(invalid_request)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"group": group}))))
}

async fn create_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(group_id): Path<Uuid>,
    Json(body): Json<community_service::ServerInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let server = community_service::create_server(&ctx.db, group_id, body)
        .await
        .map_err(invalid_request)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"server": server}))))
}

async fn delete_community_group(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    community_service::delete_group(&ctx.db, group_id)
        .await
        .map_err(invalid_request)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_community_access(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(group_id): Path<Uuid>,
    Json(body): Json<community_service::UpdateCommunityAccessInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let group = community_service::update_community_access(&ctx.db, group_id, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({"group": group})))
}

async fn test_server_rcon(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<community_service::ServerInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let result = community_service::test_server_input(body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({"result": result})))
}

async fn update_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Json(body): Json<community_service::ServerInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let server = community_service::update_server(&ctx.db, server_id, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({"server": server})))
}

async fn delete_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    community_service::delete_server(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn report_plugin_online_players(
    State(ctx): State<AppCtx>,
    Json(body): Json<community_service::OnlinePlayersReportInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match community_service::report_online_players(&ctx.db, body).await {
        Ok(result) => Ok(Json(serde_json::json!({ "result": result }))),
        Err(error) if error.to_string().contains("不匹配") => Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": error.to_string() })),
        )),
        Err(error) => Err(invalid_request(error)),
    }
}

async fn player_api_players(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let items = player_api_service::list_players(&ctx.db)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"查询玩家信息失败"}))))?;
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn get_player_api_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_api_config(&actor) {
        return Err(forbidden());
    }

    let config = player_api_service::get_config(&ctx.db)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"读取玩家 API 配置失败"}))))?;
    Ok(Json(serde_json::json!({ "config": config })))
}

async fn update_player_api_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<player_api_service::PlayerApiConfigInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_api_config(&actor) {
        return Err(forbidden());
    }

    let config = player_api_service::save_config(&ctx.db, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "config": config })))
}

async fn webhook_public(
    State(ctx): State<AppCtx>,
    Path(public_path): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let payload = player_api_service::fetch_webhook_payload_by_path(&ctx.db, &public_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Webhook 不存在"}))))?;
    Ok(Json(serde_json::json!(payload)))
}

async fn get_server_report_token(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_server_report_token(&actor) {
        return Err(forbidden());
    }

    let token = community_service::get_report_token(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "token": token })))
}

async fn reset_server_report_token(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_server_report_token(&actor) {
        return Err(forbidden());
    }

    let token = community_service::reset_report_token(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "token": token })))
}

#[derive(Deserialize)]
struct RconCommandBody {
    command: String,
}

async fn execute_rcon(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Json(body): Json<RconCommandBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }

    let response = community_service::execute_rcon_command(&ctx.db, server_id, &body.command)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "response": response })))
}

async fn get_online_players(
    State(ctx): State<AppCtx>,
    Path(server_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = community_service::list_online_players(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({"players": result.players, "details": result.details})))
}

async fn whitelist(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = whitelist_service::list_whitelist(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

async fn create_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let resolver = whitelist_service::steam_resolver(&ctx.config);
    let item = whitelist_service::create_manual_whitelist(
        &ctx.db,
        whitelist_service::ManualWhitelistInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
        },
        &operator_name,
        &resolver,
    )
    .await
    .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "手动添加白名单", &item.nickname, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

async fn approve_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::approve_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "通过白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

async fn reject_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectWhitelistBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::reject_whitelist(&ctx.db, id, &body.reason, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "拒绝白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

async fn restore_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::restore_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "恢复白名单通过", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

async fn revoke_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_revoke_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::revoke_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "删除白名单审核", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

/// 刷新单条白名单记录的Steam名称
async fn refresh_single_steam_name(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let resolver = whitelist_service::steam_resolver(&ctx.config);
    let steam_name = whitelist_service::update_steam_persona_name(&ctx.db, id, &resolver)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "steam_persona_name": steam_name })))
}

#[derive(Deserialize)]
struct RefreshSteamNamesBody {
    status: Option<String>,
}

/// 批量刷新白名单记录的Steam名称
async fn refresh_all_steam_names(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<RefreshSteamNamesBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let resolver = whitelist_service::steam_resolver(&ctx.config);
    let updated_count = whitelist_service::refresh_all_steam_persona_names(&ctx.db, &resolver, body.status.as_deref())
        .await
        .map_err(invalid_request)?;
    let operator_name = actor.display_name.clone();
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "批量刷新Steam名称", &format!("更新了{}条记录", updated_count), &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({ "updated_count": updated_count })))
}

async fn bans(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = ban_service::list_bans(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

async fn create_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<BanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let operator_name = actor.display_name.clone();
    let item = ban_service::create_ban(
        &ctx.db,
        &ctx.config,
        ban_service::CreateBanInput {
            player: body.player,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            ban_type: body.ban_type,
            reason: body.reason,
            operator_name: operator_name.clone(),
        },
    )
    .await
    .map_err(invalid_request)?;

    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    let client_ip = extract_client_ip(&headers);
    let log_ip = item.ip_address.as_deref().unwrap_or(&client_ip);
    let _ = log_service::create_log(&ctx.db, &operator_name, "封禁管理", "添加封禁", log_target, log_ip).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "item": item }))))
}

async fn update_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<BanBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    let item = ban_service::update_ban(
        &ctx.db,
        &ctx.config,
        id,
        ban_service::UpdateBanInput {
            player: body.player,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            ban_type: body.ban_type,
            reason: body.reason,
        },
    )
    .await
    .map_err(invalid_request)?;
    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "封禁管理", "编辑封禁", log_target, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({ "item": item })))
}

async fn delete_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    ban_service::delete_ban(&ctx.db, id).await.map_err(invalid_request)?;
    let log_target = record.player.as_deref().unwrap_or(&record.steam_id);
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "封禁管理", "删除封禁", log_target, &extract_client_ip(&headers)).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn unban_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let record = ban_service::find_ban(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_unban_record(&actor, &record) {
        return Err(forbidden());
    }

    let item = ban_service::unban(&ctx.db, id, &actor.display_name).await.map_err(invalid_request)?;
    let log_target = item.player.as_deref().unwrap_or(&item.steam_id);
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "封禁管理", "解封", log_target, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({ "item": item })))
}

async fn create_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let item = plugin_ban_service::create_plugin_ban(
        &ctx.db,
        plugin_ban_service::PluginBanInput {
            report_token: body.report_token,
            port: body.port,
            ban_type: body.ban_type,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            player: body.player,
            duration_minutes: body.duration_minutes,
            reason: body.reason,
            operator_name: body.operator_name,
        },
    )
    .await
    .map_err(invalid_request)?;

    let kick_message = if item.duration_minutes == 0 {
        format!("你已被永久封禁，原因：{}", item.reason)
    } else {
        format!("你已被封禁，原因：{}，到期时间：{}", item.reason, item.expires_at.as_deref().unwrap_or("未知"))
    };
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "item": item, "kick_message": kick_message }))))
}

async fn poll_plugin_bans(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanPollBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let items = plugin_ban_service::poll_active_bans(
        &ctx.db,
        plugin_ban_service::PluginBanPollInput {
            report_token: body.report_token,
            port: body.port,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Deserialize)]
struct PluginBanPollIncrementalBody {
    report_token: String,
    port: i32,
    cursor: Option<String>,
    limit: Option<i32>,
}

async fn poll_plugin_bans_incremental(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanPollIncrementalBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = plugin_ban_service::poll_active_bans_incremental(
        &ctx.db,
        plugin_ban_service::PluginBanPollInput {
            report_token: body.report_token,
            port: body.port,
        },
        body.cursor,
        body.limit,
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "cursor": result.cursor,
        "has_more": result.has_more,
        "total_count": result.total_count
    })))
}

async fn unban_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginUnbanBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let item = plugin_ban_service::unban_plugin_target(
        &ctx.db,
        plugin_ban_service::PluginUnbanInput {
            report_token: body.report_token,
            port: body.port,
            target: body.target,
            reason: body.reason,
            operator_name: body.operator_name,
            operator_steamid: body.operator_steamid,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

async fn check_plugin_ban(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginBanCheckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = plugin_ban_service::check_plugin_ban(
        &ctx.db,
        &ctx.access_snapshot,
        plugin_ban_service::PluginBanCheckInput {
            report_token: body.report_token,
            port: body.port,
            steam_id: body.steam_id,
            ip_address: body.ip_address,
            player: body.player,
            server_port: body.server_port,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!(result)))
}

async fn check_plugin_access(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessCheckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = access_service::check_access(
        &ctx.db,
        &ctx.config,
        &ctx.access_snapshot,
        &ctx.server_config_cache,
        access_service::AccessCheckInput {
            report_token: body.report_token,
            port: body.port,
            steam_id64: body.steam_id64,
            ip_address: body.ip_address,
            player: body.player,
            server_port: body.server_port,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}


async fn plugin_access_snapshot(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessSnapshotBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let snapshot = match ctx.access_snapshot.read_snapshot().await.map_err(internal_error)? {
        Some(snapshot) => snapshot,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    let item = access_snapshot_service::snapshot_for_plugin(
        &snapshot,
        &body.report_token,
        body.port,
        Utc::now(),
    )
    .map_err(invalid_request_status)?;

    Ok(Json(serde_json::json!({ "item": item })))
}

#[derive(Deserialize)]
struct ServerStatusBody {
    report_token: String,
    port: i32,
    fps: f32,
    cpu_usage: f32,
    tickrate: f32,
    #[serde(default)]
    in_rate: f32,
    #[serde(default)]
    out_rate: f32,
    #[serde(default)]
    uptime_seconds: i64,
    #[serde(default)]
    players_count: i32,
    #[serde(default)]
    max_players: i32,
    #[serde(default)]
    current_map: String,
}

async fn report_server_status(
    State(ctx): State<AppCtx>,
    Json(body): Json<ServerStatusBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = server_status_service::report_server_status(
        &ctx.db,
        server_status_service::ServerStatusInput {
            report_token: body.report_token,
            port: body.port,
            fps: body.fps,
            cpu_usage: body.cpu_usage,
            tickrate: body.tickrate,
            in_rate: body.in_rate,
            out_rate: body.out_rate,
            uptime_seconds: body.uptime_seconds,
            players_count: body.players_count,
            max_players: body.max_players,
            current_map: body.current_map,
        },
    )
    .await
    .map_err(invalid_request_status)?;

    Ok(Json(serde_json::json!({ "server_id": result.server_id })))
}

#[derive(Deserialize)]
struct OfflineSyncBody {
    report_token: String,
    port: i32,
    operations: Vec<OfflineOperationBody>,
}

#[derive(Deserialize)]
struct OfflineOperationBody {
    operation: String,
    target: String,
    target_type: String,
    player_name: Option<String>,
    reason: Option<String>,
    duration_minutes: Option<i32>,
    operator_name: String,
    operator_steamid: Option<String>,
    created_at_unix: i64,
    idempotency_key: String,
}

async fn sync_offline_operations(
    State(ctx): State<AppCtx>,
    Json(body): Json<OfflineSyncBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = offline_sync_service::sync_offline_operations(
        &ctx.db,
        offline_sync_service::OfflineSyncInput {
            report_token: body.report_token,
            port: body.port,
            operations: body.operations.into_iter().map(|op| {
                offline_sync_service::OfflineOperationInput {
                    operation: op.operation,
                    target: op.target,
                    target_type: op.target_type,
                    player_name: op.player_name,
                    reason: op.reason,
                    duration_minutes: op.duration_minutes,
                    operator_name: op.operator_name,
                    operator_steamid: op.operator_steamid,
                    created_at_unix: op.created_at_unix,
                    idempotency_key: op.idempotency_key,
                }
            }).collect(),
        },
    )
    .await
    .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({
        "received": result.received,
        "applied": result.applied,
        "skipped": result.skipped,
        "errors": result.errors
    })))
}

#[derive(Deserialize)]
struct AuditLogQueryParams {
    server_id: Option<Uuid>,
    operation: Option<String>,
    operator_name: Option<String>,
    target: Option<String>,
    source: Option<String>,
    success: Option<bool>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list_audit_logs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }

    let (items, total) = audit_service::list_audit_logs(
        &ctx.db,
        &audit_service::AuditLogQuery {
            server_id: query.server_id,
            operation: query.operation,
            operator_name: query.operator_name,
            target: query.target,
            source: query.source,
            success: query.success,
            page: query.page.unwrap_or(1).max(1),
            page_size: query.page_size.unwrap_or(20).clamp(1, 100),
        },
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "message": e.to_string() }))))?;

    Ok(Json(serde_json::json!({
        "items": items,
        "total": total,
        "page": query.page.unwrap_or(1).max(1),
        "page_size": query.page_size.unwrap_or(20).clamp(1, 100)
    })))
}

fn invalid_request_status(error: anyhow::Error) -> StatusCode {
    let _ = error;
    StatusCode::BAD_REQUEST
}

fn internal_error(error: anyhow::Error) -> StatusCode {
    let _ = error;
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn users(State(ctx): State<AppCtx>, headers: HeaderMap, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let actor = current_operator(&ctx, &headers).await.map_err(|_| StatusCode::UNAUTHORIZED)?;
    let result = user_service::list_users(&ctx.db, &actor, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

async fn create_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_admin_user(&actor) {
        return Err(forbidden());
    }

    let item = user_service::create_user(
        &ctx.db,
        user_service::CreateUserInput {
            username: body.username,
            password: body.password,
            role: body.role,
            steam_id: body.steam_id,
            remark: body.remark,
        },
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "新增管理员", &item.username, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

async fn update_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_manage_user(&actor, &target) {
        return Err(forbidden());
    }

    let keep_role = actor.role == "normal" || !permission_service::can_change_user_role(&actor, &target);
    let item = user_service::update_user(
        &ctx.db,
        id,
        user_service::UpdateUserInput {
            username: body.username,
            role: body.role,
            steam_id: body.steam_id,
            remark: body.remark,
        },
        keep_role,
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "修改管理员信息", &item.username, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

async fn update_user_password(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePasswordBody>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_manage_user(&actor, &target) {
        return Err(forbidden());
    }

    user_service::update_password(&ctx.db, id, &body.password)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "修改管理员密码", &target.username, &extract_client_ip(&headers)).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_delete_user(&actor, &target) {
        return Err(forbidden());
    }

    user_service::delete_user(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "删除管理员", &target.username, &extract_client_ip(&headers)).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_user_enabled(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_toggle_user_enabled(&actor, &target) {
        return Err(forbidden());
    }

    let item = user_service::toggle_enabled(&ctx.db, id)
        .await
        .map_err(invalid_request)?;

    let action = if item.enabled { "启用账号" } else { "禁用账号" };
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", action, &item.username, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

async fn logs(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = log_service::list_logs(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

async fn api_endpoint_docs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    Ok(Json(serde_json::json!({ "items": docs_service::list_endpoints() })))
}

async fn public_whitelist(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_whitelist(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

async fn submit_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let resolver = whitelist_service::steam_resolver(&ctx.config);
    let item = whitelist_service::create_public_whitelist_request(
        &ctx.db,
        whitelist_service::PublicWhitelistRequestInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
        },
        &resolver,
    )
    .await
    .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, "guest", "公共展示页", "提交白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

async fn public_bans(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = public_service::list_public_bans(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stats = public_service::ban_stats(&ctx.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size, "stats": stats })))
}

// ============ 玩家进服权限规则 ============

async fn player_access_rules(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let rules = player_access_rule_service::list_rules(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": rules })))
}

#[derive(Deserialize)]
struct CreatePlayerAccessRuleBody {
    steamid64: String,
    nickname: String,
    allowed_communities: Option<Vec<Uuid>>,
    blocked_communities: Option<Vec<Uuid>>,
    allowed_servers: Option<Vec<Uuid>>,
    blocked_servers: Option<Vec<Uuid>>,
}

async fn create_player_access_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<CreatePlayerAccessRuleBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }

    let rule = player_access_rule_service::create_rule(
        &ctx.db,
        player_access_rule_service::CreatePlayerAccessRuleInput {
            steamid64: body.steamid64,
            nickname: body.nickname,
            allowed_communities: body.allowed_communities,
            blocked_communities: body.blocked_communities,
            allowed_servers: body.allowed_servers,
            blocked_servers: body.blocked_servers,
        },
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家进服设置",
        "创建权限规则",
        &format!("{} ({})", rule.nickname, rule.steamid64),
        &extract_client_ip(&headers),
    )
    .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "item": rule }))))
}

#[derive(Deserialize)]
struct UpdatePlayerAccessRuleBody {
    nickname: Option<String>,
    allowed_communities: Option<Vec<Uuid>>,
    blocked_communities: Option<Vec<Uuid>>,
    allowed_servers: Option<Vec<Uuid>>,
    blocked_servers: Option<Vec<Uuid>>,
}

async fn update_player_access_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePlayerAccessRuleBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }

    let rule = player_access_rule_service::update_rule(
        &ctx.db,
        id,
        player_access_rule_service::UpdatePlayerAccessRuleInput {
            nickname: body.nickname,
            allowed_communities: body.allowed_communities,
            blocked_communities: body.blocked_communities,
            allowed_servers: body.allowed_servers,
            blocked_servers: body.blocked_servers,
        },
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家进服设置",
        "更新权限规则",
        &format!("{} ({})", rule.nickname, rule.steamid64),
        &extract_client_ip(&headers),
    )
    .await;

    Ok(Json(serde_json::json!({ "item": rule })))
}

async fn delete_player_access_rule(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_revoke_whitelist(&actor) {
        return Err(forbidden());
    }

    let rule = player_access_rule_service::find_rule_by_id(&ctx.db, id)
        .await
        .map_err(invalid_request)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "规则不存在" }))))?;

    player_access_rule_service::delete_rule(&ctx.db, id)
        .await
        .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家进服设置",
        "删除权限规则",
        &format!("{} ({})", rule.nickname, rule.steamid64),
        &extract_client_ip(&headers),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn current_operator(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<User, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "missing token" }))))?;

    let token = Uuid::parse_str(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid token" }))))?;

    let session = auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))))?;

    let user = sqlx::query_as::<_, User>(
        r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, enabled, created_at FROM users WHERE id = $1"#,
    )
    .bind(session.user_id)
    .fetch_one(&ctx.db.pool)
    .await
    .map_err(|_| (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))))?;

    if !user.enabled {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "账号已禁用" }))));
    }

    Ok(user)
}

fn forbidden() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "权限不足" })))
}

fn invalid_request(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    let error_msg = error.to_string();
    let friendly_msg = translate_db_error(&error_msg);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": friendly_msg })),
    )
}

fn translate_db_error(msg: &str) -> String {
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

fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()).map(str::trim))
        .unwrap_or("127.0.0.1")
        .to_string()
}

#[derive(Deserialize)]
struct ResolveSteamBody {
    steam_input: String,
}

#[derive(serde::Serialize)]
struct SteamResolveResponse {
    steamid64: String,
    steamid: Option<String>,
    steamid3: Option<String>,
    profile_url: Option<String>,
    persona_name: Option<String>,
}

async fn resolve_steam(
    State(ctx): State<AppCtx>,
    Json(body): Json<ResolveSteamBody>,
) -> Result<Json<SteamResolveResponse>, (StatusCode, Json<serde_json::Value>)> {
    let resolver = whitelist_service::steam_resolver(&ctx.config);

    // 解析 Steam 标识符
    let parsed = match resolver.resolve(&body.steam_input).await {
        Ok(p) => p,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
    };

    // 尝试获取 Steam 资料（5秒超时，超时则让玩家手动填写）
    let persona_name = match tokio::time::timeout(
        Duration::from_secs(5),
        resolver.fetch_profile(&parsed.steamid64),
    ).await {
        Ok(Ok(Some(profile))) => Some(profile.persona_name),
        _ => None,
    };

    Ok(Json(SteamResolveResponse {
        steamid64: parsed.steamid64,
        steamid: parsed.steamid,
        steamid3: parsed.steamid3,
        profile_url: parsed.profile_url,
        persona_name,
    }))
}

/// 查询全球封禁记录（代理 API，解决 CORS 问题，带缓存）
async fn get_global_bans(
    State(ctx): State<AppCtx>,
    Path(steamid64): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 检查缓存（缓存30分钟，减少外部 API 调用）
    {
        let cache = ctx.global_bans_cache.read().await;
        if let Some((data, timestamp)) = cache.get(&steamid64) {
            if Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                return Ok(Json(data.clone()));
            }
        }
    }

    let data = fetch_global_bans_from_api(&steamid64).await
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "查询失败" }))))?;

    // 写入缓存
    {
        let mut cache = ctx.global_bans_cache.write().await;
        cache.insert(steamid64.clone(), (data.clone(), Utc::now()));
        // 清理过期缓存（保留最近1000条）
        if cache.len() > 1000 {
            let now = Utc::now();
            cache.retain(|_, (_, ts)| now - *ts < chrono::Duration::minutes(30));
        }
    }

    Ok(Json(data))
}

/// 批量查询全球封禁记录（减少请求次数）
#[derive(Deserialize)]
struct GlobalBansBatchBody {
    steamids: Vec<String>,
}

async fn get_global_bans_batch(
    State(ctx): State<AppCtx>,
    Json(body): Json<GlobalBansBatchBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 限制单次最多查询 30 个 ID
    let steamids: Vec<String> = body.steamids.into_iter().take(30).collect();
    let mut results: HashMap<String, serde_json::Value> = HashMap::new();
    let mut to_fetch: Vec<String> = Vec::new();

    // 先检查缓存
    {
        let cache = ctx.global_bans_cache.read().await;
        for steamid64 in &steamids {
            if let Some((data, timestamp)) = cache.get(steamid64) {
                if Utc::now() - *timestamp < chrono::Duration::minutes(30) {
                    results.insert(steamid64.clone(), data.clone());
                } else {
                    to_fetch.push(steamid64.clone());
                }
            } else {
                to_fetch.push(steamid64.clone());
            }
        }
    }

    // 批量请求（限制并发数，最多同时 8 个外部请求）
    if !to_fetch.is_empty() {
        let fetch_ids = to_fetch;
        let fetch_count = fetch_ids.len();
        let fetch_ids_for_timeout = fetch_ids.clone();
        let results_vec = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            async {
                let stream = futures::stream::iter(
                    fetch_ids.into_iter().map(|id| async move {
                        let result = fetch_global_bans_from_api(&id).await;
                        (id, result)
                    })
                );
                stream.buffer_unordered(8).collect::<Vec<_>>().await
            },
        )
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("global bans batch query timed out after 15s");
            fetch_ids_for_timeout.into_iter().map(|s| (s, Err(()))).collect()
        });

        // 写入缓存和结果
        let mut cache = ctx.global_bans_cache.write().await;
        for (steamid64, result) in results_vec {
            match result {
                Ok(data) => {
                    results.insert(steamid64.clone(), data.clone());
                    cache.insert(steamid64, (data, Utc::now()));
                }
                Err(_) => {
                    results.insert(steamid64, serde_json::json!({ "data": [], "count": 0 }));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({ "results": results })))
}

/// 从第三方 API 获取封禁记录
async fn fetch_global_bans_from_api(
    steamid64: &str,
) -> Result<serde_json::Value, ()> {
    use crate::http_client::HTTP_CLIENT;

    let timeout = std::time::Duration::from_secs(5);

    // 主 API（KZTimerGlobal）
    let primary_url = format!(
        "https://kztimerglobal.com/api/v2.0/bans?steamid64={}&limit=30&offset=0",
        steamid64
    );

    // 备用 API（GOKZ.TOP）
    let fallback_url = format!(
        "https://api.gokz.top/api/v1/bans?steamid64={}&is_expired=false&limit=100",
        steamid64
    );

    // 先尝试主 API
    if let Ok(response) = tokio::time::timeout(timeout, HTTP_CLIENT.get(&primary_url).send()).await {
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(Ok(data)) = tokio::time::timeout(timeout, response.json::<serde_json::Value>()).await {
                    return Ok(data);
                }
            }
        }
    }

    // 主 API 失败，尝试备用 API
    if let Ok(response) = tokio::time::timeout(timeout, HTTP_CLIENT.get(&fallback_url).send()).await {
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(Ok(data)) = tokio::time::timeout(timeout, response.json::<serde_json::Value>()).await {
                    return Ok(data);
                }
            }
        }
    }

    Err(())
}

#[cfg(test)]
mod tests {
    use super::router;
    use crate::{config::Config, db::Database, services::access_snapshot_service::SnapshotStore};
    use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn test_snapshot_store() -> SnapshotStore {
        SnapshotStore::new(std::env::temp_dir().join(format!("manger-test-snapshot-{}.json", Uuid::new_v4())))
    }

    fn schema_url(base_url: &str, schema: &str) -> String {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        format!("{base_url}{separator}options=-csearch_path%3D{schema}")
    }

    async fn create_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new().max_connections(1).connect(base_url).await.unwrap();
        sqlx::query(&format!(r#"CREATE SCHEMA "{schema}""#)).execute(&pool).await.unwrap();
        pool.close().await;
    }

    async fn drop_schema(base_url: &str, schema: &str) {
        let pool = PgPoolOptions::new().max_connections(1).connect(base_url).await.unwrap();
        sqlx::query(&format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#)).execute(&pool).await.unwrap();
        pool.close().await;
    }

    async fn with_test_app(test: impl AsyncFnOnce(Database, Config) -> anyhow::Result<()>) {
        let config = Config::from_env();
        let base_url = config.database_url.clone();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        let scoped_url = schema_url(&base_url, &schema);
        create_schema(&base_url, &schema).await;

        let result = async {
            let db = Database::connect(&scoped_url).await?;
            db.migrate().await?;
            db.seed(&config).await?;
            test(db, config).await
        }
        .await;

        drop_schema(&base_url, &schema).await;
        result.unwrap();
    }

    async fn ensure_test_user_exists(db: &Database, user_id: &str) -> anyhow::Result<()> {
        let exists: bool = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = $1::uuid)"#,
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;
        if exists {
            return Ok(());
        }

        let (username, display_name, role, password) = match user_id {
            "11111111-1111-1111-1111-111111111111" => ("alex", "Alex", "admin", "admin123"),
            "33333333-3333-3333-3333-333333333333" => ("james", "James", "normal", "normal123"),
            "22222222-2222-2222-2222-222222222222" => ("devadmin", "DevAdmin", "developer", "dev123"),
            _ => anyhow::bail!("unknown test user id: {user_id}"),
        };

        sqlx::query(
            r#"INSERT INTO users (id, username, display_name, password_hash, role)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::parse_str(user_id).unwrap_or(Uuid::new_v4()))
        .bind(username)
        .bind(display_name)
        .bind(password)
        .bind(role)
        .execute(&db.pool)
        .await?;

        Ok(())
    }

    async fn create_session_for_user(db: &Database, user_id: &str) -> anyhow::Result<Uuid> {
        ensure_test_user_exists(db, user_id).await?;

        let user = sqlx::query_as::<_, crate::models::User>(
            r#"SELECT id, username, display_name, password_hash, role, steam_id, remark, created_at FROM users WHERE id = $1::uuid"#,
        )
        .bind(user_id)
        .fetch_one(&db.pool)
        .await?;

        let session = crate::auth::session::build_session(&user, 24);
        sqlx::query(
            r#"INSERT INTO sessions (token, user_id, role, display_name, role_label, expires_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(session.token)
        .bind(session.user_id)
        .bind(&session.role)
        .bind(&session.display_name)
        .bind(&session.role_label)
        .bind(session.expires_at)
        .bind(session.created_at)
        .execute(&db.pool)
        .await?;

        Ok(session.token)
    }

    async fn insert_whitelist(db: &Database, status: &str) -> Uuid {
        let id = Uuid::new_v4();
        let steamid64 = format!("7656119{:010}", id.as_u128() % 10_000_000_000);
        let steamid = format!("STEAM_0:1:{}", (id.as_u128() % 100000) as u64);
        let steamid3 = format!("[U:1:{}]", (id.as_u128() % 100000) as u64);
        let profile_url = format!("https://steamcommunity.com/profiles/{steamid64}");
        sqlx::query(
            r#"
            INSERT INTO whitelist_requests (
                id, steam_id, steamid64, steamid, steamid3, profile_url, nickname, status,
                applied_at, approved_at, approved_by, rejected_at, rejected_by,
                rejection_reason, source, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, '测试玩家', $7,
                    now(),
                    CASE WHEN $7 = 'approved' THEN now() ELSE NULL END,
                    CASE WHEN $7 = 'approved' THEN 'Alex' ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN now() ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN 'Alex' ELSE NULL END,
                    CASE WHEN $7 = 'rejected' THEN '资料不完整' ELSE NULL END,
                    'public', now())
            "#,
        )
        .bind(id)
        .bind(&steamid)
        .bind(&steamid64)
        .bind(&steamid)
        .bind(&steamid3)
        .bind(&profile_url)
        .bind(status)
        .execute(&db.pool)
        .await
        .unwrap();
        id
    }

    async fn insert_community_with_server(db: &Database, name: &str) -> (Uuid, Uuid) {
        let community_id = Uuid::new_v4();
        let server_id = Uuid::new_v4();

        sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, $2)"#)
            .bind(community_id)
            .bind(name)
            .execute(&db.pool)
            .await
            .unwrap();

        sqlx::query(
            r#"
            INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'online', $8)
            "#,
        )
        .bind(server_id)
        .bind(community_id)
        .bind("一号服")
        .bind("127.0.0.1")
        .bind(25575_i32)
        .bind("secret")
        .bind("plugin-token")
        .bind(Vec::<String>::new())
        .execute(&db.pool)
        .await
        .unwrap();

        (community_id, server_id)
    }

    async fn spawn_fake_rcon_server() -> (u16, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let (request_id, _, _) = read_rcon_packet(&mut stream).await?;
            write_rcon_packet(&mut stream, request_id, 0, "").await?;
            write_rcon_packet(&mut stream, request_id, 2, "").await?;
            let (request_id, _, _) = read_rcon_packet(&mut stream).await?;
            write_rcon_packet(&mut stream, request_id, 0, "玩家甲").await?;
            Ok(())
        });
        (port, handle)
    }

    async fn spawn_webhook_server(status: &'static str) -> (String, tokio::task::JoinHandle<anyhow::Result<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut buffer = vec![0_u8; 8192];
            let n = stream.read(&mut buffer).await?;
            let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
            let response = format!("HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            stream.write_all(response.as_bytes()).await?;
            Ok(request)
        });
        (url, handle)
    }

    async fn read_rcon_packet(stream: &mut tokio::net::TcpStream) -> std::io::Result<(i32, i32, String)> {
        let mut size_bytes = [0_u8; 4];
        stream.read_exact(&mut size_bytes).await?;
        let size = i32::from_le_bytes(size_bytes);
        let mut payload = vec![0_u8; size as usize];
        stream.read_exact(&mut payload).await?;
        let mut request_id_bytes = [0_u8; 4];
        request_id_bytes.copy_from_slice(&payload[0..4]);
        let mut packet_type_bytes = [0_u8; 4];
        packet_type_bytes.copy_from_slice(&payload[4..8]);
        Ok((
            i32::from_le_bytes(request_id_bytes),
            i32::from_le_bytes(packet_type_bytes),
            String::from_utf8_lossy(&payload[8..payload.len() - 2]).into_owned(),
        ))
    }

    async fn write_rcon_packet(stream: &mut tokio::net::TcpStream, request_id: i32, packet_type: i32, body: &str) -> std::io::Result<()> {
        let size = body.len() + 10;
        let mut packet = Vec::with_capacity(size + 4);
        packet.extend_from_slice(&(size as i32).to_le_bytes());
        packet.extend_from_slice(&request_id.to_le_bytes());
        packet.extend_from_slice(&packet_type.to_le_bytes());
        packet.extend_from_slice(body.as_bytes());
        packet.extend_from_slice(&[0, 0]);
        stream.write_all(&packet).await
    }

    #[tokio::test]
    async fn community_server_stores_report_token() {
        with_test_app(async |db, _config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, 'Token社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
                   VALUES ($1, $2, 'Token服', '127.0.0.1', 27015, 'secret', 'plugin-token', 'online', $3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let groups = crate::services::community_service::list_groups(&db).await?;
            assert_eq!(groups[0].servers[0].report_token.as_deref(), Some("plugin-token"));
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn community_servers_include_access_control_fields() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '准入社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;

            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   )
                   VALUES ($1, $2, '限制服', '127.0.0.1', 27015, 'secret', 'access-token', 'online', $3, true, 1200, 10, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/community/servers")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let server = &payload["groups"][0]["servers"][0];
            assert_eq!(server["access_restriction_enabled"], true);
            assert_eq!(server["min_rating"], 1200);
            assert_eq!(server["min_steam_level"], 10);
            assert_eq!(server["whitelist_mode_enabled"], true);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_create_server_with_access_control_config() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '创建准入社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            let (rcon_port, rcon_server) = spawn_fake_rcon_server().await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db.clone(), test_snapshot_store());

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/community/groups/{community_id}/servers"))
                        .header("authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            json!({
                                "name": "准入服",
                                "ip": "127.0.0.1",
                                "port": rcon_port,
                                "rcon_password": "secret",
                                "report_token": "access-token-create",
                                "note": "限制开启",
                                "access_restriction_enabled": true,
                                "min_rating": 1500,
                                "min_steam_level": 12,
                                "whitelist_mode_enabled": true
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            rcon_server.await.unwrap()?;
            assert_eq!(response.status(), StatusCode::CREATED);

            let saved = sqlx::query_as::<_, (bool, i32, i32, bool)>(
                r#"SELECT access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   FROM servers WHERE report_token = $1"#,
            )
            .bind("access-token-create")
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved, (true, 1500, 12, true));
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn community_servers_counts_players_from_report_details() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "真实人数统计").await;
            sqlx::query(r#"UPDATE servers SET players = $2 WHERE id = $1"#)
                .bind(server_id)
                .bind(vec!["残留玩家".to_string()])
                .execute(&db.pool)
                .await?;

            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());

            let request = Request::builder()
                .method("GET")
                .uri("/api/community/servers")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let server = &payload["groups"][0]["servers"][0];
            assert_eq!(server["online_player_count"], 0);
            assert_eq!(server["players"].as_array().unwrap().len(), 0);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_cannot_delete_community_group() {
        with_test_app(async |db, config| {
            let (group_id, _) = insert_community_with_server(&db, "普通管理员不可删除").await;
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
            let app = router(config, db, test_snapshot_store());

            let request = Request::builder()
                .method("DELETE")
                .uri(format!("/api/community/groups/{group_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_delete_community_group_and_cascade_servers() {
        with_test_app(async |db, config| {
            let (group_id, server_id) = insert_community_with_server(&db, "系统管理员可删除").await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db.clone(), test_snapshot_store());

            let request = Request::builder()
                .method("DELETE")
                .uri(format!("/api/community/groups/{group_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);

            let community_count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM communities WHERE id = $1"#)
                .bind(group_id)
                .fetch_one(&db.pool)
                .await?;
            let server_count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM servers WHERE id = $1"#)
                .bind(server_id)
                .fetch_one(&db.pool)
                .await?;

            assert_eq!(community_count.0, 0);
            assert_eq!(server_count.0, 0);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_view_and_reset_server_report_token() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "Token 管理").await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db.clone(), test_snapshot_store());

            let request = Request::builder()
                .method("GET")
                .uri(format!("/api/community/servers/{server_id}/report-token"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["token"]["report_token"], "plugin-token");

            let request = Request::builder()
                .method("POST")
                .uri(format!("/api/community/servers/{server_id}/report-token/reset"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_ne!(payload["token"]["report_token"], "plugin-token");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_cannot_view_server_report_token() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "Token 权限").await;
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
            let app = router(config, db, test_snapshot_store());

            let request = Request::builder()
                .method("GET")
                .uri(format!("/api/community/servers/{server_id}/report-token"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn access_check_rejects_banned_player_before_other_rules() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            let server_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '封禁优先社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   )
                   VALUES ($1, $2, '封禁优先服', '127.0.0.1', 27015, 'secret', 'access-token-ban', 'online', $3, false, 0, 0, false)"#,
            )
            .bind(server_id)
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            sqlx::query(
                r#"INSERT INTO ban_records (id, player, steam_id, ban_type, reason, duration_minutes, status, operator_name, source)
                   VALUES ($1, 'bad-player', '76561198000000001', 'steam', '作弊', 0, 'active', 'ConsoleAdmin', 'manual')"#,
            )
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            let app = router(config, db, test_snapshot_store());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-ban","port":27015,"steam_id64":"76561198000000001"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你已被封禁，无法进入服务器。");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn access_check_allows_when_no_access_modes_enabled() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '自由社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players)
                   VALUES ($1, $2, '自由服', '127.0.0.1', 27015, 'secret', 'access-token-open', 'online', $3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let app = router(config, db, test_snapshot_store());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-open","port":27015,"steam_id64":"76561198000000002"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], true);
            assert_eq!(payload["result"]["message"], "允许进入服务器。");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn access_check_whitelist_mode_requires_approved_global_whitelist() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '白名单社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (id, community_id, name, ip, port, rcon_password, report_token, status, players, whitelist_mode_enabled)
                   VALUES ($1, $2, '白名单服', '127.0.0.1', 27015, 'secret', 'access-token-whitelist', 'online', $3, true)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;
            let app = router(config, db.clone(), test_snapshot_store());

            let rejected = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-whitelist","port":27015,"steam_id64":"76561198000000003"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(rejected.status(), StatusCode::OK);
            let body = to_bytes(rejected.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你尚未通过白名单审核，无法进入服务器。");

            insert_whitelist_for_steamid64(&db, "76561198000000003", "approved").await?;
            let approved = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-whitelist","port":27015,"steam_id64":"76561198000000003"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(approved.status(), StatusCode::OK);
            let body = to_bytes(approved.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], true);
            assert_eq!(payload["result"]["message"], "已通过白名单审核，允许进入服务器。");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn access_check_restriction_uses_success_cache_and_rejects_low_values() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '限制社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level, whitelist_mode_enabled
                   )
                   VALUES ($1, $2, '限制服', '127.0.0.1', 27015, 'secret', 'access-token-restrict', 'online', $3, true, 2000, 20, false)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;
            sqlx::query(
                r#"INSERT INTO player_access_cache (steamid64, rating, steam_level, expires_at)
                   VALUES ($1, 1999, 30, now() + interval '24 hours')"#,
            )
            .bind("76561198000000004")
            .execute(&db.pool)
            .await?;

            let app = router(config, db, test_snapshot_store());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-restrict","port":27015,"steam_id64":"76561198000000004"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "你的 GOKZ rating 未达到服务器最低要求。");
            Ok(())
        }).await;
    }

    async fn insert_whitelist_for_steamid64(db: &Database, steamid64: &str, status: &str) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO whitelist_requests (
                id, nickname, steam_id, steamid64, steamid, steamid3, profile_url, status, applied_at, updated_at
               )
               VALUES ($1, $2, $3, $3, $4, $5, $6, $7, now(), now())"#,
        )
        .bind(id)
        .bind(format!("玩家{steamid64}"))
        .bind(steamid64)
        .bind("STEAM_0:1:1")
        .bind("[U:1:1]")
        .bind(format!("https://steamcommunity.com/profiles/{steamid64}"))
        .bind(status)
        .execute(&db.pool)
        .await?;
        Ok(id)
    }

    #[tokio::test]
    async fn access_check_rejects_when_profile_lookup_fails_and_does_not_cache_failure() {
        with_test_app(async |db, config| {
            let community_id = Uuid::new_v4();
            sqlx::query(r#"INSERT INTO communities (id, name) VALUES ($1, '查询失败社区')"#)
                .bind(community_id)
                .execute(&db.pool)
                .await?;
            sqlx::query(
                r#"INSERT INTO servers (
                    id, community_id, name, ip, port, rcon_password, report_token, status, players,
                    access_restriction_enabled, min_rating, min_steam_level
                   )
                   VALUES ($1, $2, '查询失败拒绝服', '127.0.0.1', 27015, 'secret', 'access-token-fail-closed', 'online', $3, true, 9999, 99)"#,
            )
            .bind(Uuid::new_v4())
            .bind(community_id)
            .bind(Vec::<String>::new())
            .execute(&db.pool)
            .await?;

            let app = router(config, db.clone(), test_snapshot_store());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/plugin/access/check")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"report_token":"access-token-fail-closed","port":27015,"steam_id64":"76561198000000005"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["result"]["allowed"], false);
            assert_eq!(payload["result"]["message"], "访问控制服务暂时不可用，请稍后再试。");

            let cache_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM player_access_cache WHERE steamid64 = $1")
                .bind("76561198000000005")
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(cache_count.0, 0);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_list_real_player_api_rows_and_configure_distribution_limit() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let (_, server_id) = insert_community_with_server(&db, "玩家信息 API 服").await;
            sqlx::query(
                r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, reported_at)
                   VALUES ($1, 'Alice', '76561198000000001', '203.0.113.10', 28, 25575, now())"#,
            )
            .bind(server_id)
            .execute(&db.pool)
            .await?;

            let app = router(config.clone(), db.clone(), test_snapshot_store());
            let players_request = Request::builder()
                .method("GET")
                .uri("/api/player-api/players")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let players_response = app.oneshot(players_request).await.unwrap();
            assert_eq!(players_response.status(), StatusCode::OK);
            let bytes = to_bytes(players_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["items"][0]["player"], "Alice");
            assert_eq!(payload["items"][0]["steam_id64"], "76561198000000001");
            assert_eq!(payload["items"][0]["server_name"], "一号服");
            assert_eq!(payload["items"][0]["server_port"], 25575);

            let app = router(config.clone(), db.clone(), test_snapshot_store());
            let config_request = Request::builder()
                .method("PUT")
                .uri("/api/player-api/config")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "max_api_count": 1,
                    "interval_seconds": 45,
                    "items": [
                        {"public_path":"my-hook","webhook_url":"https://api.example.com/a","secret":null,"server_ids":[server_id]}
                    ]
                }).to_string()))
                .unwrap();
            let config_response = app.oneshot(config_request).await.unwrap();
            assert_eq!(config_response.status(), StatusCode::OK);
            let bytes = to_bytes(config_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["config"]["max_api_count"], 1);
            assert_eq!(payload["config"]["interval_seconds"], 45);
            assert_eq!(payload["config"]["items"][0]["public_path"], "my-hook");
            assert_eq!(payload["config"]["items"][0]["webhook_url"], "https://api.example.com/a");

            let app = router(config, db, test_snapshot_store());
            let over_limit_request = Request::builder()
                .method("PUT")
                .uri("/api/player-api/config")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "max_api_count": 1,
                    "interval_seconds": 45,
                    "items": [
                        {"webhook_url":"https://api.example.com/a","secret":null,"server_ids":[]},
                        {"webhook_url":"https://api.example.com/b","secret":null,"server_ids":[]}
                    ]
                }).to_string()))
                .unwrap();
            let over_limit_response = app.oneshot(over_limit_request).await.unwrap();
            assert_eq!(over_limit_response.status(), StatusCode::BAD_REQUEST);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn player_api_dispatch_posts_webhook_and_records_status() {
        with_test_app(async |db, _config| {
            let (_, server_id) = insert_community_with_server(&db, "Webhook 分发服").await;
            sqlx::query(
                r#"INSERT INTO server_online_players (server_id, name, steam_id64, ip, ping, server_port, reported_at)
                   VALUES ($1, 'Alice', '76561198000000001', '203.0.113.10', 28, 25575, now())"#,
            )
            .bind(server_id)
            .execute(&db.pool)
            .await?;

            let (url, server) = spawn_webhook_server("200 OK").await;
            let saved = crate::services::player_api_service::save_config(
                &db,
                crate::services::player_api_service::PlayerApiConfigInput {
                    max_api_count: 1,
                    interval_seconds: 30,
                    items: vec![crate::services::player_api_service::PlayerApiWebhookInput {
                        public_path: "test-webhook".to_string(),
                        webhook_url: url,
                        secret: Some("dispatch-secret".to_string()),
                        server_ids: vec![server_id],
                    }],
                },
            )
            .await?;

            crate::services::player_api_service::dispatch_once(&db, &reqwest::Client::new()).await?;
            let request = server.await.unwrap()?;
            assert!(request.contains("POST /"));
            assert!(request.to_ascii_lowercase().contains("x-manger-secret: dispatch-secret"));

            let row: (Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT last_status, last_error, last_dispatched_at FROM player_api_webhooks WHERE id = $1"#,
            )
            .bind(saved.items[0].id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(row.0.as_deref(), Some("success"));
            assert!(row.1.is_none());
            assert!(row.2.is_some());
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn player_api_dispatch_records_failed_webhook_status() {
        with_test_app(async |db, _config| {
            let (_, server_id) = insert_community_with_server(&db, "Webhook 失败服").await;
            let (url, server) = spawn_webhook_server("500 Internal Server Error").await;
            let saved = crate::services::player_api_service::save_config(
                &db,
                crate::services::player_api_service::PlayerApiConfigInput {
                    max_api_count: 1,
                    interval_seconds: 30,
                    items: vec![crate::services::player_api_service::PlayerApiWebhookInput {
                        public_path: "test-webhook".to_string(),
                        webhook_url: url,
                        secret: None,
                        server_ids: vec![server_id],
                    }],
                },
            )
            .await?;

            crate::services::player_api_service::dispatch_once(&db, &reqwest::Client::new()).await?;
            let _ = server.await.unwrap()?;

            let row: (Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT last_status, last_error, last_dispatched_at FROM player_api_webhooks WHERE id = $1"#,
            )
            .bind(saved.items[0].id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(row.0.as_deref(), Some("failed"));
            assert_eq!(row.1.as_deref(), Some("HTTP 500 Internal Server Error"));
            assert!(row.2.is_some());
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_cannot_manage_player_api_config() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("PUT")
                .uri("/api/player-api/config")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"max_api_count":1,"interval_seconds":30,"items":[]} ).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_report_updates_online_players_when_token_and_port_match() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "插件上报").await;
            let app = router(config, db.clone(), test_snapshot_store());

            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/online-players/report")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "players": [
                        {
                            "name": "Alice",
                            "steam_id64": "76561198000000001",
                            "ip": "203.0.113.10",
                            "ping": 28,
                            "server_port": 25575
                        }
                    ]
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let server: (String, Vec<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT status, players, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(server.0, "online");
            assert_eq!(server.1, vec!["Alice".to_string()]);
            assert!(server.2.is_some());

            let player: (String, String, String, i32, i32) = sqlx::query_as(
                r#"SELECT name, steam_id64, ip, ping, server_port FROM server_online_players WHERE server_id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(player.0, "Alice");
            assert_eq!(player.1, "76561198000000001");
            assert_eq!(player.2, "203.0.113.10");
            assert_eq!(player.3, 28);
            assert_eq!(player.4, 25575);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_report_accepts_legacy_payload_with_steam2_id() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "旧插件上报").await;
            let app = router(config, db.clone(), test_snapshot_store());

            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/online-players/report")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "players": [
                        {
                            "name": "LegacyAlice",
                            "steam_id": "STEAM_0:1:1",
                            "team": "2",
                            "score": 15,
                            "ping": 42,
                            "connected_seconds": 120
                        }
                    ]
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let server: (String, Vec<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                r#"SELECT status, players, last_reported_at FROM servers WHERE id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(server.0, "online");
            assert_eq!(server.1, vec!["LegacyAlice".to_string()]);
            assert!(server.2.is_some());

            let player: (String, String, String, i32, i32) = sqlx::query_as(
                r#"SELECT name, steam_id64, ip, ping, server_port FROM server_online_players WHERE server_id = $1"#,
            )
            .bind(server_id)
            .fetch_one(&db.pool)
            .await?;
            assert_eq!(player.0, "LegacyAlice");
            assert_eq!(player.1, "76561197960265731");
            assert_eq!(player.2, "unknown");
            assert_eq!(player.3, 42);
            assert_eq!(player.4, 25575);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_report_rejects_matching_token_with_wrong_port() {
        with_test_app(async |db, config| {
            let _ = insert_community_with_server(&db, "插件上报拒绝").await;
            let app = router(config, db, test_snapshot_store());

            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/online-players/report")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 27016,
                    "players": []
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn submit_whitelist_returns_json_body() {
        with_test_app(async |db, config| {
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/public/whitelist")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "steam_input": "76561197960290419",
                    "nickname": "测试玩家"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["status"], "pending");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_list_api_endpoint_docs() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("GET")
                .uri("/api/docs/endpoints")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let items = payload["items"].as_array().unwrap();
            assert!(items.iter().any(|item| item["endpoint"] == "/api/docs/endpoints"));
            assert!(items.iter().any(|item| item["endpoint"] == "/api/bans" && item["method"] == "POST"));
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_cannot_list_api_endpoint_docs() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("GET")
                .uri("/api/docs/endpoints")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn approve_whitelist_with_authenticated_operator() {
        with_test_app(async |db, config| {
            let id = insert_whitelist(&db, "pending").await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri(format!("/api/whitelist/{id}/approve"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["status"], "approved");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn restore_whitelist_with_authenticated_operator() {
        with_test_app(async |db, config| {
            let id = insert_whitelist(&db, "rejected").await;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri(format!("/api/whitelist/{id}/restore"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["status"], "approved");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_create_manual_ip_ban_with_missing_player_and_ip() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db.clone(), test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": null,
                    "steam_id": "76561198000000000",
                    "ban_type": "ip",
                    "ip_address": null,
                    "reason": "重复违规"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);

            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(payload["item"]["player"].is_null());
            assert_eq!(payload["item"]["steam_id"], "76561198000000000");
            assert_eq!(payload["item"]["ban_type"], "ip");
            assert!(payload["item"]["ip_address"].is_null());
            assert_eq!(payload["item"]["reason"], "重复违规");
            assert_eq!(payload["item"]["duration_minutes"], 0);
            assert!(payload["item"]["expires_at"].is_null());
            assert_eq!(payload["item"]["source"], "manual");
            assert!(payload["item"]["server_id"].is_null());
            assert!(payload["item"]["server_port"].is_null());

            let saved = sqlx::query_as::<_, (Option<String>, String, String, Option<String>, String, i32, Option<chrono::DateTime<chrono::Utc>>, String)>(
                r#"SELECT player, steam_id, ban_type, ip_address, reason, duration_minutes, expires_at, source FROM ban_records WHERE steam_id = $1"#,
            )
            .bind("76561198000000000")
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved.0, None);
            assert_eq!(saved.1, "76561198000000000");
            assert_eq!(saved.2, "ip");
            assert_eq!(saved.3, None);
            assert_eq!(saved.4, "重复违规");
            assert_eq!(saved.5, 0);
            assert!(saved.6.is_none());
            assert_eq!(saved.7, "manual");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_ban_check_completes_missing_manual_ban_details() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "补齐封禁信息服").await;
            let ban_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, NULL, '76561198000000000', NULL, NULL, 'steam',
                           0, NULL, '重复违规', 'active', 'Alex', 'manual', NULL, NULL)"#,
            )
            .bind(ban_id)
            .execute(&db.pool)
            .await?;

            let app = router(config, db.clone(), test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans/check")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "steam_id": "76561198000000000",
                    "ip_address": "192.168.1.55",
                    "player": "LatePlayer",
                    "server_port": 25575
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let saved = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, Option<Uuid>, Option<i32>)>(
                r#"SELECT player, ip_address, server_name, server_id, server_port FROM ban_records WHERE id = $1"#,
            )
            .bind(ban_id)
            .fetch_one(&db.pool)
            .await?;

            assert_eq!(saved.0.as_deref(), Some("LatePlayer"));
            assert_eq!(saved.1.as_deref(), Some("192.168.1.55"));
            assert_eq!(saved.2.as_deref(), Some("一号服"));
            assert_eq!(saved.3, Some(server_id));
            assert_eq!(saved.4, Some(25575));
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_can_create_timed_ban() {
        with_test_app(async |db, config| {
            let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
            let app = router(config, db.clone(), test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "ban_type": "steam",
                    "steam_id": "STEAM_1:1:12345",
                    "ip_address": "192.168.1.20",
                    "player": "BadPlayer",
                    "duration_minutes": 30,
                    "reason": "作弊",
                    "operator_name": "ConsoleAdmin"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["source"], "game_plugin");
            assert_eq!(payload["item"]["duration_minutes"], 30);
            assert!(payload["item"]["expires_at"].is_string());
            assert_eq!(payload["kick_message"], "你已被封禁，原因：作弊，到期时间：".to_string() + payload["item"]["expires_at"].as_str().unwrap());
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_ban_rejects_invalid_token() {
        with_test_app(async |db, config| {
            let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "wrong-token",
                    "port": 25575,
                    "ban_type": "steam",
                    "steam_id": "STEAM_1:1:12345",
                    "ip_address": "192.168.1.20",
                    "player": "BadPlayer",
                    "duration_minutes": 0,
                    "reason": "作弊",
                    "operator_name": "ConsoleAdmin"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_can_check_and_unban_target() {
        with_test_app(async |db, config| {
            let (_, _) = insert_community_with_server(&db, "插件封禁服").await;
            let app = router(config.clone(), db.clone(), test_snapshot_store());
            let create_request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "ban_type": "ip",
                    "steam_id": null,
                    "ip_address": "192.168.1.21",
                    "player": "IpPlayer",
                    "duration_minutes": 0,
                    "reason": "恶意行为",
                    "operator_name": "ConsoleAdmin"
                }).to_string()))
                .unwrap();
            let create_response = app.oneshot(create_request).await.unwrap();
            assert_eq!(create_response.status(), StatusCode::CREATED);

            let app = router(config.clone(), db.clone(), test_snapshot_store());
            let check_request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans/check")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "steam_id": "STEAM_1:1:999",
                    "ip_address": "192.168.1.21"
                }).to_string()))
                .unwrap();
            let check_response = app.oneshot(check_request).await.unwrap();
            assert_eq!(check_response.status(), StatusCode::OK);
            let bytes = to_bytes(check_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["banned"], true);

            let app = router(config, db, test_snapshot_store());
            let unban_request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans/unban")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575,
                    "target": "192.168.1.21",
                    "reason": "申诉通过",
                    "operator_name": "ConsoleAdmin"
                }).to_string()))
                .unwrap();
            let unban_response = app.oneshot(unban_request).await.unwrap();
            assert_eq!(unban_response.status(), StatusCode::OK);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn plugin_can_poll_active_bans_for_server() {
        with_test_app(async |db, config| {
            let (_, server_id) = insert_community_with_server(&db, "插件轮询封禁服").await;
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, 'BadPlayer', '76561197960290419', '192.168.1.30', '一号服', 'steam',
                           0, NULL, '作弊', 'active', 'Alex', 'manual', $2, 25575),
                          ($3, 'OldPlayer', '76561197960290421', '192.168.1.31', '一号服', 'steam',
                           0, NULL, '已解封', 'inactive', 'Alex', 'manual', $2, 25575)"#,
            )
            .bind(Uuid::new_v4())
            .bind(server_id)
            .bind(Uuid::new_v4())
            .execute(&db.pool)
            .await?;

            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/plugin/bans/poll")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "report_token": "plugin-token",
                    "port": 25575
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["items"].as_array().unwrap().len(), 1);
            assert_eq!(payload["items"][0]["steam_id"], "76561197960290419");
            assert_eq!(payload["items"][0]["ip_address"], "192.168.1.30");
            assert_eq!(payload["items"][0]["reason"], "作弊");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn create_manual_ban_rejects_missing_reason() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": "Alex",
                    "steam_id": "76561198000000001",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "   "
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["error"], "封禁理由不能为空");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_update_and_delete_manual_ban() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let ban_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO ban_records (
                       id, player, steam_id, ip_address, server_name, ban_type,
                       duration_minutes, expires_at, reason, status, operator_name, source,
                       server_id, server_port
                   )
                   VALUES ($1, 'OldName', '76561197960290419', '192.168.1.5', NULL, 'steam',
                           0, NULL, '旧原因', 'active', 'Alex', 'manual', NULL, NULL)"#,
            )
            .bind(ban_id)
            .execute(&db.pool)
            .await?;

            let app = router(config.clone(), db.clone(), test_snapshot_store());
            let update_request = Request::builder()
                .method("PUT")
                .uri(format!("/api/bans/{ban_id}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": "NewName",
                    "steam_id": "STEAM_0:1:12345",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.6",
                    "reason": "新原因"
                }).to_string()))
                .unwrap();
            let update_response = app.oneshot(update_request).await.unwrap();
            assert_eq!(update_response.status(), StatusCode::OK);
            let bytes = to_bytes(update_response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["player"], "NewName");
            assert_eq!(payload["item"]["steam_id"], "76561197960290419");
            assert_eq!(payload["item"]["ip_address"], "192.168.1.6");
            assert_eq!(payload["item"]["reason"], "新原因");

            let app = router(config, db.clone(), test_snapshot_store());
            let delete_request = Request::builder()
                .method("DELETE")
                .uri(format!("/api/bans/{ban_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let delete_response = app.oneshot(delete_request).await.unwrap();
            assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

            let count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM ban_records WHERE id = $1"#)
                .bind(ban_id)
                .fetch_one(&db.pool)
                .await?;
            assert_eq!(count.0, 0);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn create_manual_ban_normalizes_steamid2_to_steamid64() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db.clone(), test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": "Alex",
                    "steam_id": "STEAM_0:1:12345",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "作弊"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["item"]["steam_id"], "76561197960290419");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn create_manual_ban_rejects_missing_steamid64() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": "Alex",
                    "steam_id": "   ",
                    "ban_type": "steam",
                    "ip_address": "192.168.1.5",
                    "reason": "作弊"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["error"], "SteamID64 不能为空");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn create_manual_ban_rejects_invalid_ban_type() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/bans")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "player": "Alex",
                    "steam_id": "76561198000000002",
                    "ban_type": "hardware",
                    "ip_address": "192.168.1.6",
                    "reason": "作弊"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["error"], "封禁属性无效");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_only_sees_self_in_users_list() {
        with_test_app(async |db, config| {
            let app = router(config, db.clone(), test_snapshot_store());
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;

            let request = Request::builder()
                .method("GET")
                .uri("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(payload["items"].as_array().unwrap().len(), 1);
            assert_eq!(payload["items"][0]["role"], "normal");
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_cannot_update_developer_user() {
        with_test_app(async |db, config| {
            let app = router(config, db.clone(), test_snapshot_store());
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;

            let request = Request::builder()
                .method("PUT")
                .uri("/api/users/22222222-2222-2222-2222-222222222222")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "username": "devadmin2",
                    "role": "developer",
                    "steam_id": "76561198000000000",
                    "remark": "should fail"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn normal_admin_can_approve_but_cannot_revoke_whitelist() {
        with_test_app(async |db, config| {
            let pending_id = insert_whitelist(&db, "pending").await;
            let approved_id = insert_whitelist(&db, "approved").await;
            let token = create_session_for_user(&db, "33333333-3333-3333-3333-333333333333").await?;
            let app = router(config, db, test_snapshot_store());

            let approve_request = Request::builder()
                .method("POST")
                .uri(format!("/api/whitelist/{pending_id}/approve"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let approve_response = app.clone().oneshot(approve_request).await.unwrap();
            assert_eq!(approve_response.status(), StatusCode::OK);

            let revoke_request = Request::builder()
                .method("POST")
                .uri(format!("/api/whitelist/{approved_id}/revoke"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let revoke_response = app.oneshot(revoke_request).await.unwrap();
            assert_eq!(revoke_response.status(), StatusCode::FORBIDDEN);
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_create_user_without_steam_id() {
        with_test_app(async |db, config| {
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "username": "new_admin_without_steam",
                    "password": "secret123",
                    "role": "normal",
                    "steam_id": null,
                    "remark": "无 steamid"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(payload["item"]["steam_id"].is_null());
            Ok(())
        }).await;
    }

    #[tokio::test]
    async fn admin_can_clear_user_steam_id() {
        with_test_app(async |db, config| {
            ensure_test_user_exists(&db, "33333333-3333-3333-3333-333333333333").await?;
            let token = create_session_for_user(&db, "11111111-1111-1111-1111-111111111111").await?;
            let app = router(config, db, test_snapshot_store());
            let request = Request::builder()
                .method("PUT")
                .uri("/api/users/33333333-3333-3333-3333-333333333333")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "username": "james",
                    "role": "normal",
                    "steam_id": null,
                    "remark": "清空 steamid"
                }).to_string()))
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(payload["item"]["steam_id"].is_null());
            Ok(())
        }).await;
    }
}

