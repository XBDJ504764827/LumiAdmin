use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::routes::{
    current_operator, forbidden, internal_error, invalid_request, invalid_request_status, AppCtx,
    AppError,
};
use crate::services::{
    access_log_service, access_service, access_snapshot_service, community_service,
    permission_service,
};

#[derive(Deserialize)]
pub(crate) struct PluginAccessCheckBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    server_port: Option<i32>,
    /// 游戏插件上报：玩家是否为 CS 优先账户（Prime）。缺失表示插件暂未确认。
    #[serde(default)]
    is_cs_prime: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PluginAccessSnapshotBody {
    report_token: String,
    port: i32,
    /// 可选：插件上次收到的快照版本。命中时后端省略 item，仅返回 unchanged=true。
    #[serde(default)]
    etag: Option<String>,
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
        &ctx.active_ban_cache,
        &ctx.whitelist_cache,
        &ctx.gokz_cache,
        access_service::AccessCheckInput {
            report_token: body.report_token.clone(),
            port: body.port,
            steam_id64: body.steam_id64.clone(),
            ip_address: body.ip_address.clone(),
            player: body.player.clone(),
            server_port: body.server_port,
            is_cs_prime: body.is_cs_prime,
        },
    )
    .await
    .map_err(invalid_request)?;

    // 记录进服日志（无论成功/失败）
    if let Ok(Some(server)) = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
    {
        let access_method_str = result
            .access_method
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let community_name = community_service::find_group_name(&ctx.db, server.community_id)
            .await
            .ok();
        let access_method = access_log_service::AccessMethod::from_str(&access_method_str);
        let reject_reason = if result.allowed {
            None
        } else {
            result
                .audit_message
                .as_deref()
                .or(Some(result.message.as_str()))
        };
        if let Err(e) = access_log_service::create_access_log(
            &ctx.db,
            &body.steam_id64,
            body.player.as_deref(),
            body.ip_address.as_deref(),
            server.id,
            &server.name,
            server.port,
            server.community_id,
            community_name.as_deref(),
            result.allowed,
            &access_method,
            result.failure_code.as_deref(),
            reject_reason,
            result.rating,
            result.steam_level,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                steam_id64 = %body.steam_id64,
                server_port = server.port,
                "写入进服日志失败（access/check）"
            );
        }
    }

    Ok(Json(serde_json::json!({ "result": result })))
}

/// 游戏插件本地裁决结果上报。
///
/// LumiAuth 本地自治（Data Plane）后，进服主链路由插件读取本地快照同步裁决，
/// 不再逐个玩家调用 `/api/plugin/access/check`。插件在本地裁决完成后调用本接口
/// 上报结果，供管理后台「进服监控」展示（成功与拒绝原因）。
#[derive(Deserialize)]
pub(crate) struct PluginAccessRecordBody {
    report_token: String,
    port: i32,
    steam_id64: String,
    ip_address: Option<String>,
    player: Option<String>,
    /// 是否放行
    allowed: bool,
    /// 进服方式（access_method 字符串，如 whitelist / banned / risk_blocked 等）
    access_method: Option<String>,
    /// 失败码（可选，如 banned / whitelist_rejected 等）
    failure_code: Option<String>,
    /// 拒绝原因（可选，拒绝时展示）
    reject_reason: Option<String>,
    rating: Option<i32>,
    steam_level: Option<i32>,
}

pub(crate) async fn record_plugin_access(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessRecordBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let server = ctx
        .server_config_cache
        .get_by_token_port(&ctx.db, &body.report_token, body.port)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::bad_request(anyhow::anyhow!("服务器令牌或端口不匹配")))?;

    let community_name = community_service::find_group_name(&ctx.db, server.community_id)
        .await
        .ok();
    let access_method_str = body
        .access_method
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if body.allowed {
            "unrestricted"
        } else {
            "unknown"
        });
    let access_method = access_log_service::AccessMethod::from_str(access_method_str);
    // 空字符串归一为 NULL，避免成功记录里出现空的失败原因
    let failure_code = crate::services::normalize_optional_text(body.failure_code.as_deref());
    let reject_reason = crate::services::normalize_optional_text(body.reject_reason.as_deref());

    access_log_service::create_access_log(
        &ctx.db,
        &body.steam_id64,
        body.player.as_deref(),
        body.ip_address.as_deref(),
        server.id,
        &server.name,
        server.port,
        server.community_id,
        community_name.as_deref(),
        body.allowed,
        &access_method,
        failure_code.as_deref(),
        reject_reason.as_deref(),
        body.rating,
        body.steam_level,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            error = %e,
            steam_id64 = %body.steam_id64,
            server_port = server.port,
            "写入进服日志失败（access/record）"
        );
        AppError::internal(e)
    })?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn plugin_access_snapshot(
    State(ctx): State<AppCtx>,
    Json(body): Json<PluginAccessSnapshotBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let snapshot = match ctx
        .access_snapshot
        .read_snapshot()
        .await
        .map_err(internal_error)?
    {
        Some(snapshot) => snapshot,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    // etag 增量：插件回传的版本与当前快照一致时跳过全量传输。
    // 先校验服务器归属（snapshot_for_plugin 会检查 token+port），再决定是否省略 item。
    let item = access_snapshot_service::snapshot_for_plugin(
        &snapshot,
        &body.report_token,
        body.port,
        Utc::now(),
    )
    .map_err(invalid_request_status)?;

    let unchanged = body
        .etag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| value == snapshot.version);

    if unchanged {
        return Ok(Json(serde_json::json!({
            "unchanged": true,
            "version": snapshot.version,
        })));
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn list_access_logs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(params): Query<access_log_service::AccessLogQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&actor) {
        return Err(forbidden());
    }

    let page = params.page();
    let page_size = params.page_size();

    match access_log_service::query_access_logs(&ctx.db, &params).await {
        Ok((items, total)) => Ok(Json(serde_json::json!({
            "items": items,
            "total": total,
            "page": page,
            "page_size": page_size,
        }))),
        Err(e) => {
            tracing::error!(%e, "查询进服记录失败");
            Ok(Json(serde_json::json!({
                "error": "查询进服记录失败",
                "items": [],
                "total": 0,
                "page": page,
                "page_size": page_size,
            })))
        }
    }
}
