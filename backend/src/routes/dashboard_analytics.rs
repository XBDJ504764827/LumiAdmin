use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

use crate::routes::{current_operator, forbidden, AppCtx};
use crate::services::dashboard_service;

#[derive(Deserialize)]
pub(crate) struct TrendQuery {
    pub days: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct ActivityQuery {
    pub range: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RankingQuery {
    pub range: Option<String>,
    pub limit: Option<i64>,
}

async fn require_dashboard_role(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(ctx, headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    Ok(())
}

pub(crate) async fn whitelist_trend(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<TrendQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    require_dashboard_role(&ctx, &headers).await?;
    let days = match query.days {
        Some(7) => 7,
        Some(90) => 90,
        _ => 30,
    };
    let data = dashboard_service::get_whitelist_trend(&ctx.db, days)
        .await
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "data": data })))
}

pub(crate) async fn server_activity(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    require_dashboard_role(&ctx, &headers).await?;
    let range = match query.range.as_deref() {
        Some("today") | Some("1d") | Some("7d") | Some("30d") | Some("90d") => {
            query.range.as_deref().unwrap_or("today")
        }
        _ => "today",
    };
    let data = dashboard_service::get_server_activity(&ctx.db, range)
        .await
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "data": data })))
}

pub(crate) async fn server_status(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    require_dashboard_role(&ctx, &headers).await?;
    let data = dashboard_service::get_server_status_distribution(&ctx.db)
        .await
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "data": data })))
}

pub(crate) async fn server_ranking(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<RankingQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    require_dashboard_role(&ctx, &headers).await?;
    let days = match query.range.as_deref() {
        Some("1d") => 1,
        Some("30d") => 30,
        Some("90d") => 90,
        _ => 7,
    };
    let limit = query.limit.unwrap_or(10).clamp(1, 10);
    let data = dashboard_service::get_server_ranking(&ctx.db, days, limit)
        .await
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "data": data })))
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(error = %error, "加载仪表盘统计失败");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "加载仪表盘统计失败" })),
    )
}
