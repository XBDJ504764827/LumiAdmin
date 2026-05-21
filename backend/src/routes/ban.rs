use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{AppCtx, ListQuery, current_operator, forbidden, invalid_request};
use crate::services::{ban_service, log_service, plugin_ban_service, permission_service};
use crate::services::rate_limit_service::extract_client_ip;

#[derive(Deserialize)]
pub(crate) struct BanBody {
    pub player: Option<String>,
    pub steam_id: String,
    pub ban_type: String,
    pub ip_address: Option<String>,
    pub reason: String,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanBody {
    pub report_token: String,
    pub port: i32,
    pub ban_type: String,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub duration_minutes: i32,
    pub reason: String,
    pub operator_name: String,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanPollBody {
    pub report_token: String,
    pub port: i32,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanPollIncrementalBody {
    pub report_token: String,
    pub port: i32,
    pub cursor: Option<String>,
    pub limit: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct PluginUnbanBody {
    pub report_token: String,
    pub port: i32,
    pub target: String,
    pub reason: Option<String>,
    pub operator_name: String,
    pub operator_steamid: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PluginBanCheckBody {
    pub report_token: String,
    pub port: i32,
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
    pub player: Option<String>,
    pub server_port: Option<i32>,
}

pub(crate) async fn bans(
    State(ctx): State<AppCtx>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = ban_service::list_bans(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

pub(crate) async fn create_ban(
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

pub(crate) async fn update_ban(
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

pub(crate) async fn delete_ban(
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

pub(crate) async fn unban_ban(
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

pub(crate) async fn create_plugin_ban(
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

pub(crate) async fn poll_plugin_bans(
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

pub(crate) async fn poll_plugin_bans_incremental(
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

pub(crate) async fn unban_plugin_ban(
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

pub(crate) async fn check_plugin_ban(
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
