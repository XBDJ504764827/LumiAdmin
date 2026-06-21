use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};

use crate::{
    routes::{current_operator, forbidden, AppCtx},
    services::observability_service,
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
