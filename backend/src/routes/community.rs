use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{
    community_rcon, community_service, log_service, permission_service,
    rate_limit_service::extract_client_ip,
};

#[derive(Deserialize)]
pub(crate) struct RconCommandBody {
    pub(crate) command: String,
}

pub(crate) async fn community_servers(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

    let groups = community_service::list_groups(&ctx.db).await.map_err(|e| {
        tracing::error!(error = %e, "加载社区服务器失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载社区服务器失败" })),
        )
    })?;
    Ok(Json(serde_json::json!({"groups": groups})))
}

pub(crate) async fn create_community_group(
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
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "创建社区组",
        &group.name,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"group": group})),
    ))
}

pub(crate) async fn create_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(group_id): Path<Uuid>,
    Json(body): Json<community_service::ServerInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let rcon_timeouts = community_rcon::RconTimeouts::from_config(&ctx.config);
    let server = community_service::create_server(&ctx.db, group_id, body, rcon_timeouts)
        .await
        .map_err(invalid_request)?;
    // 新服务器创建后刷新缓存
    ctx.server_config_cache.invalidate_all().await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "添加游戏服务器",
        &format!("{} ({}:{}))", server.name, server.ip, server.port),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"server": server})),
    ))
}

pub(crate) async fn delete_community_group(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    // 获取社区组名称用于日志
    let group_name = community_service::find_group_name(&ctx.db, group_id)
        .await
        .unwrap_or_else(|_| group_id.to_string());
    community_service::delete_group(&ctx.db, group_id)
        .await
        .map_err(invalid_request)?;
    // 删除社区组后刷新全部缓存（因为组下所有服务器都被删除）
    ctx.server_config_cache.invalidate_all().await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "删除社区组",
        &group_name,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn update_community_access(
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
    // 社区访问设置更新后刷新全部缓存（影响组下所有服务器的 effective_* 计算）
    ctx.server_config_cache.invalidate_all().await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "更新社区访问限制",
        &format!(
            "{} (白名单模式: {}, Rating≥{}, Steam等级≥{})",
            group.name,
            if group.whitelist_mode_enabled {
                "开"
            } else {
                "关"
            },
            group.min_rating,
            group.min_steam_level
        ),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({"group": group})))
}

pub(crate) async fn test_server_rcon(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<community_service::ServerInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let result = community_rcon::test_server_input_with_timeouts(
        body,
        community_rcon::RconTimeouts::from_config(&ctx.config),
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({"result": result})))
}

pub(crate) async fn update_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Json(body): Json<community_service::ServerInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    let rcon_timeouts = community_rcon::RconTimeouts::from_config(&ctx.config);
    let server = community_service::update_server(&ctx.db, server_id, body, rcon_timeouts)
        .await
        .map_err(invalid_request)?;
    // 服务器配置更新后失效缓存
    ctx.server_config_cache.invalidate(server_id).await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "编辑游戏服务器",
        &format!("{} ({}:{}))", server.name, server.ip, server.port),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({"server": server})))
}

pub(crate) async fn delete_community_server(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }
    // 获取服务器信息用于日志
    let server_info = community_service::find_server_info(&ctx.db, server_id).await;
    community_service::delete_server(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    // 服务器删除后失效缓存
    ctx.server_config_cache.invalidate(server_id).await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "删除游戏服务器",
        &server_info.unwrap_or_else(|| server_id.to_string()),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn report_plugin_online_players(
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

pub(crate) async fn get_online_players(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

    let result = community_service::list_online_players(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(
        serde_json::json!({"players": result.players, "details": result.details}),
    ))
}

pub(crate) async fn get_server_report_token(
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

pub(crate) async fn reset_server_report_token(
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
    // report_token 更新后失效缓存
    ctx.server_config_cache.invalidate(server_id).await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "社区组管理",
        "重置服务器Token",
        &server_id.to_string(),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "token": token })))
}

pub(crate) async fn execute_rcon(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(server_id): Path<Uuid>,
    Json(body): Json<RconCommandBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }

    let response = community_rcon::execute_rcon_command(
        &ctx.db,
        server_id,
        &body.command,
        community_rcon::RconTimeouts::from_config(&ctx.config),
    )
    .await
    .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "RCON命令",
        "执行RCON命令",
        &format!("服务器 {} → {}", server_id, body.command),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "response": response })))
}
