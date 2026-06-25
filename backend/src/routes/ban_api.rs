use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx, ListQuery};
use crate::services::{
    ban_api_service, external_ban_api_service, log_service, permission_service,
    rate_limit_service::extract_client_ip,
};

#[derive(Deserialize)]
pub(crate) struct CreateKeyBody {
    pub name: String,
}

#[derive(Deserialize)]
pub(crate) struct IntegrationBanBody {
    pub player: Option<String>,
    pub steam_id: String,
    pub ban_type: String,
    pub ip_address: Option<String>,
    pub reason: String,
    pub duration_minutes: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct IntegrationCheckBody {
    pub steam_id: Option<String>,
    pub ip_address: Option<String>,
}

fn extract_api_token(headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    if let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(value.to_string());
    }

    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing api key" })),
        ))
}

async fn current_api_key(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<ban_api_service::BanApiKeyAuth, (StatusCode, Json<serde_json::Value>)> {
    let token = extract_api_token(headers)?;
    ban_api_service::authenticate_key(&ctx.db, &token)
        .await
        .map_err(|error| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
        })
}

pub(crate) async fn list_keys(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    let items = ban_api_service::list_keys(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn create_key(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<CreateKeyBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    let result = ban_api_service::create_key(
        &ctx.db,
        actor.id,
        ban_api_service::CreateBanApiKeyInput {
            name: body.name.clone(),
        },
    )
    .await
    .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "API密钥管理",
        "创建封禁API密钥",
        &body.name,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": result.key, "token": result.token })),
    ))
}

pub(crate) async fn delete_key(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    // 获取密钥名称用于日志
    let key_name = ban_api_service::find_key_name(&ctx.db, id)
        .await
        .unwrap_or_else(|| id.to_string());
    ban_api_service::delete_key(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "API密钥管理",
        "删除封禁API密钥",
        &key_name,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn integration_bans(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _key = current_api_key(&ctx, &headers).await?;
    let result = ban_api_service::list_integration_bans(&ctx.db, &query)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size
    })))
}

pub(crate) async fn create_integration_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<IntegrationBanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let key = current_api_key(&ctx, &headers).await?;
    let item = ban_api_service::create_integration_ban(
        &ctx.db,
        &ctx.config,
        &key,
        ban_api_service::IntegrationCreateBanInput {
            player: body.player.clone(),
            steam_id: body.steam_id.clone(),
            ban_type: body.ban_type.clone(),
            ip_address: body.ip_address.clone(),
            reason: body.reason.clone(),
            duration_minutes: body.duration_minutes,
        },
    )
    .await
    .map_err(invalid_request)?;
    external_ban_api_service::sync_ban_if_enabled(&ctx.db, &item).await;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &format!("API:{}", key.name),
        "封禁API",
        "通过API创建封禁",
        &format!(
            "{} ({})",
            body.player.as_deref().unwrap_or(&body.steam_id),
            body.steam_id
        ),
        "",
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn check_integration_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<IntegrationCheckBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _key = current_api_key(&ctx, &headers).await?;
    let result = ban_api_service::check_integration_ban(
        &ctx.db,
        ban_api_service::IntegrationCheckInput {
            steam_id: body.steam_id,
            ip_address: body.ip_address,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!(result)))
}
