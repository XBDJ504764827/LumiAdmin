use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::map_sync_service;

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateConfigBody {
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,
    pub source_urls: Vec<String>,
    #[serde(default = "default_interval")]
    pub check_interval_secs: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateAgentBody {
    pub name: String,
    pub target_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaimQuery {
    pub limit: Option<i64>,
}

fn default_true() -> bool {
    true
}

fn default_interval() -> i32 {
    3600
}

pub(crate) async fn get_map_sync_overview(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;
    let overview = map_sync_service::overview(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "overview": overview })))
}

pub(crate) async fn update_map_sync_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<UpdateConfigBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let config = map_sync_service::update_config(
        &ctx.db,
        map_sync_service::UpdateMapSyncConfigInput {
            enabled: body.enabled,
            auto_update: body.auto_update,
            source_urls: body.source_urls,
            check_interval_secs: body.check_interval_secs,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "config": config })))
}

pub(crate) async fn create_map_sync_agent(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<CreateAgentBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let agent = map_sync_service::create_agent(
        &ctx.db,
        map_sync_service::CreateMapSyncAgentInput {
            name: body.name,
            target_type: body.target_type,
            enabled: body.enabled,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "agent": agent })))
}

pub(crate) async fn delete_map_sync_agent(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    map_sync_service::delete_agent(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn reset_map_sync_agent_token(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let agent = map_sync_service::reset_agent_token(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "agent": agent })))
}

pub(crate) async fn check_all_maps(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let result = map_sync_service::check_and_enqueue_all(&ctx.db)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}

pub(crate) async fn sync_single_map(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(map_name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }
    let result = map_sync_service::enqueue_single_map(&ctx.db, &map_name)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "result": result })))
}

pub(crate) async fn agent_inventory(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<map_sync_service::AgentInventoryInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_from_headers(&ctx, &headers).await?;
    let count = map_sync_service::report_inventory(&ctx.db, &agent, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "ok": true, "count": count })))
}

pub(crate) async fn agent_tasks(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<ClaimQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_from_headers(&ctx, &headers).await?;
    let tasks = map_sync_service::claim_agent_tasks(&ctx.db, &agent, query.limit.unwrap_or(20))
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "tasks": tasks })))
}

pub(crate) async fn agent_task_report(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(task_id): Path<Uuid>,
    Json(body): Json<map_sync_service::AgentTaskReportInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let agent = agent_from_headers(&ctx, &headers).await?;
    map_sync_service::report_task_result(&ctx.db, &agent, task_id, body)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn agent_from_headers(
    ctx: &AppCtx,
    headers: &HeaderMap,
) -> Result<map_sync_service::MapSyncAgent, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("x-map-agent-token")
        .and_then(|value| value.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "missing map agent token" })),
        ))?;
    map_sync_service::agent_by_token(&ctx.db, token)
        .await
        .map_err(|error| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
        })
}
