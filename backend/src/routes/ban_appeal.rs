use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{AppCtx, ListQuery, current_operator, invalid_request};
use crate::services::{ban_appeal_service, log_service};
use crate::services::rate_limit_service::extract_client_ip;

pub(crate) async fn list_appeals(
    State(ctx): State<AppCtx>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = ban_appeal_service::list_appeals(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

#[derive(Deserialize)]
pub(crate) struct ReviewBody {
    pub review_note: Option<String>,
}

pub(crate) async fn approve_appeal(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;

    let item = ban_appeal_service::approve_appeal(
        &ctx.db,
        id,
        &actor.display_name,
        body.review_note,
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!("{} ({})", item.player_name, item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁申诉",
        "通过申诉并解封",
        &log_target,
        &extract_client_ip(&headers),
    ).await {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "item": item }))))
}

pub(crate) async fn reject_appeal(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;

    let item = ban_appeal_service::reject_appeal(
        &ctx.db,
        id,
        &actor.display_name,
        body.review_note,
    )
    .await
    .map_err(invalid_request)?;

    let log_target = format!("{} ({})", item.player_name, item.steam_id);
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "封禁申诉",
        "驳回申诉",
        &log_target,
        &extract_client_ip(&headers),
    ).await {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "item": item }))))
}
