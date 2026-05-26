use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{community_service, permission_service};

#[derive(Deserialize)]
pub(crate) struct RconCommandBody {
    pub(crate) command: String,
}

pub(crate) async fn community_servers(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

    let groups = community_service::list_groups(&ctx.db).await.map_err(|_| {
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
    let server = community_service::create_server(&ctx.db, group_id, body)
        .await
        .map_err(invalid_request)?;
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
    community_service::delete_group(&ctx.db, group_id)
        .await
        .map_err(invalid_request)?;
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
    let result = community_service::test_server_input(body)
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
    let server = community_service::update_server(&ctx.db, server_id, body)
        .await
        .map_err(invalid_request)?;
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
    community_service::delete_server(&ctx.db, server_id)
        .await
        .map_err(invalid_request)?;
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

    let response = community_service::execute_rcon_command(&ctx.db, server_id, &body.command)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "response": response })))
}
