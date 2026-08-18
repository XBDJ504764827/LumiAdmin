use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};

use crate::{
    routes::{current_operator, forbidden, AppCtx},
    services::{lumi_bot_service, observability_service},
};

pub(crate) async fn overview(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    Ok(Json(serde_json::json!({
        "data": observability_service::overview(&ctx.db, &ctx.config)
    })))
}

pub(crate) async fn lumi_bot_status(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let status = lumi_bot_service::status(&ctx.db, &ctx.config)
        .await
        .map_err(|error| {
            tracing::error!(%error, "读取 LumiBot 状态失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "读取 LumiBot 状态失败" })),
            )
        })?;

    Ok(Json(serde_json::json!({ "data": status })))
}
