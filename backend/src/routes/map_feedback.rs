use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

use crate::routes::{current_operator, invalid_request, AppCtx, ListQuery};
use crate::services::{
    log_service, map_feedback_service, notification_service, rate_limit_service::extract_client_ip,
};

/// 管理员查询地图反馈列表
pub(crate) async fn list_feedback(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let result = map_feedback_service::list_feedback(&ctx.db, &query)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({
        "items": result.items,
        "total": result.total,
        "page": result.page,
        "page_size": result.page_size,
    })))
}

/// 管理员审核地图反馈
pub(crate) async fn review_feedback(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<map_feedback_service::ReviewMapFeedbackInput>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let item = map_feedback_service::review_feedback(&ctx.db, id, &actor.display_name, body)
        .await
        .map_err(invalid_request)?;

    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "地图反馈",
        "审核反馈",
        &format!("{} → {}", item.id, item.status),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    Ok(Json(serde_json::json!({ "item": item })))
}

/// 公开页面提交地图反馈
pub(crate) async fn submit_feedback(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<map_feedback_service::CreateMapFeedbackInput>,
) -> Result<
    (axum::http::StatusCode, Json<serde_json::Value>),
    (axum::http::StatusCode, Json<serde_json::Value>),
> {
    let item = map_feedback_service::create_feedback(&ctx.db, &ctx.config, body)
        .await
        .map_err(invalid_request)?;

    let _ = log_service::create_log(
        &ctx.db,
        "guest",
        "公共展示页",
        "提交地图反馈",
        &format!(
            "{:?}: {}",
            item.feedback_type,
            &item.detail.chars().take(80).collect::<String>()
        ),
        &extract_client_ip(&headers),
    )
    .await;

    if let Err(e) = notification_service::notify_all_admins(
        &ctx.db,
        &ctx.notification_hub,
        None,
        "map_feedback",
        "新地图反馈",
        &format!("收到一条新的地图反馈（{:?}）", item.feedback_type),
        Some("/map-feedback"),
    )
    .await
    {
        tracing::warn!(%e, "地图反馈通知失败");
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "item": item })),
    ))
}

/// 公开页面按 SteamID 查询反馈状态
pub(crate) async fn query_feedback_status(
    State(ctx): State<AppCtx>,
    Json(body): Json<QueryFeedbackBody>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let resolver = &ctx.steam_resolver;
    let parsed = resolver.resolve(&body.steam_input).await.map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let items = map_feedback_service::query_feedback_by_steam_id(&ctx.db, &parsed.steamid64)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询地图反馈状态失败");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "查询失败" })),
            )
        })?;

    Ok(Json(serde_json::json!({
        "steamid64": parsed.steamid64,
        "feedback": items,
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueryFeedbackBody {
    pub steam_input: String,
}
