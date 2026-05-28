use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::external_ban_api_service;

#[derive(Deserialize)]
pub(crate) struct TargetBody {
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub default_ban_type: String,
    pub auto_sync: bool,
    pub notes_template: String,
    pub stats_template: Option<String>,
}

fn ensure_admin(
    actor: &crate::models::Operator,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    Ok(())
}

fn input_from_body(body: TargetBody) -> external_ban_api_service::ExternalBanApiTargetInput {
    external_ban_api_service::ExternalBanApiTargetInput {
        name: body.name,
        enabled: body.enabled,
        base_url: body.base_url,
        bearer_token: body.bearer_token,
        default_ban_type: body.default_ban_type,
        auto_sync: body.auto_sync,
        notes_template: body.notes_template,
        stats_template: body.stats_template,
    }
}

pub(crate) async fn list_targets(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let items = external_ban_api_service::list_targets(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

pub(crate) async fn create_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Json(body): Json<TargetBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let item = external_ban_api_service::create_target(&ctx.db, input_from_body(body))
        .await
        .map_err(invalid_request)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

pub(crate) async fn update_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<TargetBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let item = external_ban_api_service::update_target(&ctx.db, id, input_from_body(body))
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn delete_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    external_ban_api_service::delete_target(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn test_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let result = external_ban_api_service::test_target(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}

pub(crate) async fn sync_ban(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let result = external_ban_api_service::sync_ban(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}

pub(crate) async fn sync_ban_to_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path((ban_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let result = external_ban_api_service::sync_ban_to_target(&ctx.db, ban_id, target_id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}
