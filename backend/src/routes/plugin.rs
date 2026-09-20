use axum::http::StatusCode;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};

use crate::routes::{current_operator, forbidden, invalid_request, invalid_request_status, AppCtx};
use crate::services::{
    log_service, offline_sync_service, permission_service, player_api_service,
    plugin_identify_service, server_status_service,
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

#[derive(Deserialize)]
pub(crate) struct IdentifyServerBody {
    port: i32,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    game: Option<String>,
    #[serde(default)]
    install_id: Option<String>,
}

/// 插件免配置自识别：插件只填面板地址，由面板按「来源 IP + 端口」（或已绑定的安装实例）
/// 返回本服 report_token。成功后插件会把 token 缓存到本地，后续请求与旧版完全一致。
pub(crate) async fn identify_plugin_server(
    State(ctx): State<AppCtx>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<IdentifyServerBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(expected) = ctx.config.plugin_install_key.as_deref() {
        let provided = headers
            .get("x-lumi-install-key")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .unwrap_or_default();
        if provided.is_empty() || provided != expected {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "安装密钥无效" })),
            ));
        }
    }

    let client_ip =
        resolve_identify_client_ip(peer.ip(), &headers, ctx.config.plugin_trust_proxy_headers);

    let result = plugin_identify_service::identify(
        &ctx.db,
        &plugin_identify_service::IdentifyInput {
            port: body.port,
            hostname: body.hostname.clone(),
            game: body.game.clone(),
            install_id: body.install_id.clone(),
            client_ip,
        },
        ctx.config.plugin_auto_bind,
        ctx.config.plugin_rebind_after_secs,
    )
    .await
    .map_err(|error| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
    })?;

    Ok(Json(serde_json::json!({ "server": result })))
}

/// 确定插件请求的真实来源 IP。
///
/// 仅当连接来自回环地址（通常为同机 Nginx）或显式开启 `PLUGIN_TRUST_PROXY_HEADERS`
/// 时才采信转发头，避免直连场景下伪造 X-Forwarded-For 冒充服务器。
fn resolve_identify_client_ip(peer: IpAddr, headers: &HeaderMap, trust_proxy: bool) -> String {
    let ip = if trust_proxy || peer.is_loopback() {
        forwarded_client_ip(headers).unwrap_or(peer)
    } else {
        peer
    };
    plugin_identify_service::normalize_client_ip(&ip.to_string())
}

fn forwarded_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    // Cloudflare 场景优先 CF-Connecting-IP
    let candidates = ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"];
    for name in candidates {
        let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        // X-Forwarded-For 形如 "client, proxy1, proxy2"，取最左侧的客户端地址
        for candidate in value.split(',') {
            let candidate = candidate.trim();
            if let Ok(ip) = candidate.parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    None
}

pub(crate) async fn player_api_players(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let items = player_api_service::list_players(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询玩家信息失败");
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

    let config = player_api_service::get_config(&ctx.db).await.map_err(|e| {
        tracing::error!(error = %e, "读取玩家 API 配置失败");
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
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "玩家信息API",
        "更新Webhook配置",
        &format!(
            "{} 个 Webhook 端点, 间隔 {}s",
            config.items.len(),
            config.interval_seconds
        ),
        "",
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
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
