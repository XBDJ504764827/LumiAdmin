use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{audit_service, external_ban_api_service, log_service, rate_limit_service::extract_client_ip};

#[derive(Deserialize, Clone)]
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

    let item = external_ban_api_service::create_target(&ctx.db, input_from_body(body.clone()))
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部封禁API",
        "添加同步目标",
        &format!("{} ({})", body.name, body.base_url),
        &extract_client_ip(&headers),
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

pub(crate) async fn update_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<TargetBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let item = external_ban_api_service::update_target(&ctx.db, id, input_from_body(body.clone()))
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部封禁API",
        "编辑同步目标",
        &format!("{} ({})", body.name, body.base_url),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({ "item": item })))
}

pub(crate) async fn delete_target(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    // 获取目标信息用于日志
    let target_info = external_ban_api_service::find_target_info(&ctx.db, id).await;
    external_ban_api_service::delete_target(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部封禁API",
        "删除同步目标",
        &target_info.unwrap_or_else(|| id.to_string()),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
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

    // Write admin_log for manual sync
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部封禁API",
        "手动同步封禁",
        &format!("{} — {}", id, result.message),
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    // Write audit log for manual sync
    if let Err(e) = audit_service::write_audit_log(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "external_ban_sync".to_string(),
            target: id.to_string(),
            target_type: "ban".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "manual_sync".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: result.ok,
            message: Some(result.message.clone()),
            idempotency_key: None,
        },
    )
    .await
    {
        tracing::warn!(%e, "external ban sync audit log write failed");
    }

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

// ---------------------------------------------------------------------------
// Sync history: list sync records, get per-ban sync status
// ---------------------------------------------------------------------------

pub(crate) async fn list_sync_records(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Query(query): Query<external_ban_api_service::SyncRecordQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let (items, total) = external_ban_api_service::list_sync_records(&ctx.db, &query)
        .await
        .map_err(invalid_request)?;

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    Ok(Json(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "page_size": page_size,
    })))
}

pub(crate) async fn get_ban_sync_status(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _actor = current_operator(&ctx, &headers).await?;

    let items = external_ban_api_service::get_ban_sync_status(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "items": items })))
}

// ---------------------------------------------------------------------------
// Batch retry failed syncs
// ---------------------------------------------------------------------------

pub(crate) async fn retry_failed_syncs(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    ensure_admin(&actor)?;

    let result = external_ban_api_service::retry_failed_syncs(&ctx.db)
        .await
        .map_err(invalid_request)?;

    // Admin log for retry
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "外部封禁API",
        "重试失败同步",
        &result.message,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }

    // Audit log for retry
    if let Err(e) = audit_service::write_audit_log(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "external_ban_retry".to_string(),
            target: "batch".to_string(),
            target_type: "sync".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "manual_retry".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: result.ok,
            message: Some(result.message.clone()),
            idempotency_key: None,
        },
    )
    .await
    {
        tracing::warn!(%e, "retry failed syncs audit log write failed");
    }

    Ok(Json(serde_json::json!({ "result": result })))
}
