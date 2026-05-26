use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

use crate::routes::{current_operator, forbidden, invalid_request, invalid_request_status, AppCtx};
use crate::services::{
    offline_sync_service, permission_service, player_api_service, server_status_service,
};

#[derive(Deserialize)]
pub(crate) struct ServerStatusBody {
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

pub(crate) async fn report_server_status(
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
pub(crate) struct OfflineOperationBody {
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

#[derive(Deserialize)]
pub(crate) struct OfflineSyncBody {
    report_token: String,
    port: i32,
    operations: Vec<OfflineOperationBody>,
}

pub(crate) async fn sync_offline_operations(
    State(ctx): State<AppCtx>,
    Json(body): Json<OfflineSyncBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = offline_sync_service::sync_offline_operations(
        &ctx.db,
        offline_sync_service::OfflineSyncInput {
            report_token: body.report_token,
            port: body.port,
            operations: body
                .operations
                .into_iter()
                .map(|op| offline_sync_service::OfflineOperationInput {
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
                })
                .collect(),
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

pub(crate) async fn player_api_players(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let items = player_api_service::list_players(&ctx.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"查询玩家信息失败"})),
            )
        })?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn get_player_api_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_player_api_config(&actor) {
        return Err(forbidden());
    }

    let config = player_api_service::get_config(&ctx.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"读取玩家 API 配置失败"})),
        )
    })?;
    Ok(Json(serde_json::json!({ "config": config })))
}

pub(crate) async fn update_player_api_config(
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

pub(crate) async fn webhook_public(
    State(ctx): State<AppCtx>,
    Path(public_path): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let secret_header = headers.get("X-Manger-Secret").and_then(|v| v.to_str().ok());

    let payload =
        player_api_service::fetch_webhook_payload_by_path(&ctx.db, &public_path, secret_header)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg == "not found" {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"端点不存在"})),
                    )
                } else if msg == "disabled" {
                    (
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({"error":"该端点已禁用"})),
                    )
                } else if msg == "unauthorized" {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({"error":"需要有效的密钥验证"})),
                    )
                } else {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error":"查询失败"})),
                    )
                }
            })?;
    Ok(Json(serde_json::json!(payload)))
}
