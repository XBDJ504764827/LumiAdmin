use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{AppCtx, current_operator, forbidden, invalid_request, invalid_request_status, internal_error};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{access_service, access_snapshot_service, log_service, permission_service, player_access_rule_service};

#[derive(Deserialize)]
pub(crate) struct PluginAccessCheckBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    server_port: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PluginAccessSnapshotBody {
    report_token: String,
    port: i32,
}

pub(crate) async fn check_plugin_access(
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


pub(crate) async fn plugin_access_snapshot(
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

pub(crate) async fn player_access_rules(
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
pub(crate) struct CreatePlayerAccessRuleBody {
    steamid64: String,
    nickname: String,
    allowed_communities: Option<Vec<Uuid>>,
    blocked_communities: Option<Vec<Uuid>>,
    allowed_servers: Option<Vec<Uuid>>,
    blocked_servers: Option<Vec<Uuid>>,
}

pub(crate) async fn create_player_access_rule(
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
pub(crate) struct UpdatePlayerAccessRuleBody {
    nickname: Option<String>,
    allowed_communities: Option<Vec<Uuid>>,
    blocked_communities: Option<Vec<Uuid>>,
    allowed_servers: Option<Vec<Uuid>>,
    blocked_servers: Option<Vec<Uuid>>,
}

pub(crate) async fn update_player_access_rule(
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

pub(crate) async fn delete_player_access_rule(
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
