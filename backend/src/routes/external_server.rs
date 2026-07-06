use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{
    external_server_service, log_service, rate_limit_service::extract_client_ip,
};

pub(crate) async fn list_external_servers(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let items = external_server_service::list_servers(&ctx.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询外部服务器失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"查询外部服务器失败"})),
            )
        })?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Deserialize)]
pub(crate) struct CreateBody {
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: i32,
}

fn default_true() -> bool {
    true
}
fn default_poll_interval() -> i32 {
    external_server_service::default_poll_interval()
}

pub(crate) async fn create_external_server(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CreateBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let server = external_server_service::create_server(
        &ctx.db,
        external_server_service::CreateExternalServerInput {
            name: body.name.clone(),
            ip: body.ip.clone(),
            port: body.port,
            rcon_password: body.rcon_password.clone(),
            enabled: body.enabled,
            poll_interval: body.poll_interval,
        },
    )
    .await
    .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部服务器",
        "添加外部服务器",
        &format!("{} ({}:{})", body.name, body.ip, body.port),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "server": server })))
}

#[derive(Deserialize)]
pub(crate) struct UpdateBody {
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: i32,
}

pub(crate) async fn update_external_server(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let server = external_server_service::update_server(
        &ctx.db,
        id,
        external_server_service::UpdateExternalServerInput {
            name: body.name.clone(),
            ip: body.ip.clone(),
            port: body.port,
            rcon_password: body.rcon_password.clone(),
            enabled: body.enabled,
            poll_interval: body.poll_interval,
        },
    )
    .await
    .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部服务器",
        "编辑外部服务器",
        &format!("{} ({}:{})", body.name, body.ip, body.port),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "server": server })))
}

pub(crate) async fn delete_external_server(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    // 获取服务器信息用于日志
    let server_info = external_server_service::find_server_info(&ctx.db, id).await;
    external_server_service::delete_server(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部服务器",
        "删除外部服务器",
        &server_info.unwrap_or_else(|| id.to_string()),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn test_external_server(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let result = external_server_service::test_server(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}
