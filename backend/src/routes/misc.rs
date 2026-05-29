use axum::http::StatusCode;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, AppCtx, ListQuery};
use crate::services::{
    audit_service, dashboard_service, docs_service, log_service, permission_service,
};

pub(crate) async fn health_check(State(ctx): State<AppCtx>) -> Json<serde_json::Value> {
    let db_ok = sqlx::query("SELECT 1").execute(&ctx.db.pool).await.is_ok();
    Json(serde_json::json!({
        "ok": db_ok,
        "database": db_ok,
    }))
}
pub(crate) async fn dashboard(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let data = dashboard_service::get_metrics(&ctx.db).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载仪表盘失败" })),
        )
    })?;
    Ok(Json(serde_json::json!({"data": data})))
}

pub(crate) async fn review_counts(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let include_reports = matches!(actor.role.as_str(), "admin" | "developer");

    let counts = dashboard_service::get_review_counts(&ctx.db, include_reports)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "加载待审核数量失败" })),
            )
        })?;
    Ok(Json(serde_json::json!(counts)))
}

pub(crate) async fn logs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let result = log_service::list_logs(&ctx.db, &query).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "加载日志失败" })),
        )
    })?;
    Ok(Json(
        serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size }),
    ))
}

#[derive(Deserialize)]
pub(crate) struct AuditLogQueryParams {
    server_id: Option<Uuid>,
    operation: Option<String>,
    operator_name: Option<String>,
    target: Option<String>,
    source: Option<String>,
    success: Option<bool>,
    page: Option<i64>,
    page_size: Option<i64>,
}

pub(crate) async fn list_audit_logs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQueryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_audit_logs(&actor) {
        return Err(forbidden());
    }

    let (items, total) = audit_service::list_audit_logs(
        &ctx.db,
        &audit_service::AuditLogQuery {
            server_id: query.server_id,
            operation: query.operation,
            operator_name: query.operator_name,
            target: query.target,
            source: query.source,
            success: query.success,
            page: query.page.unwrap_or(1).max(1),
            page_size: query.page_size.unwrap_or(20).clamp(1, 100),
        },
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "message": e.to_string() })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "items": items,
        "total": total,
        "page": query.page.unwrap_or(1).max(1),
        "page_size": query.page_size.unwrap_or(20).clamp(1, 100)
    })))
}

pub(crate) async fn api_endpoint_docs(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    Ok(Json(
        serde_json::json!({ "items": docs_service::list_endpoints() }),
    ))
}
