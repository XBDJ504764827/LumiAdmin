use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

use crate::routes::{current_operator, AppCtx, AppError};
use crate::services::rate_limit_service::extract_client_ip;
use crate::services::{audit_service, global_ban_service, permission_service};

fn optional_client_ip(headers: &HeaderMap) -> Option<String> {
    let ip = extract_client_ip(headers);
    (!ip.trim().is_empty()).then_some(ip)
}

/// 获取全球封禁列表（读取后台同步维护的本地缓存）
pub(crate) async fn list_global_bans(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&actor) {
        return Err(AppError::forbidden());
    }

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);

    let result = global_ban_service::fetch_live_global_bans(&ctx.db, page, page_size)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(serde_json::json!({
        "items": result.items,
        "page": result.page,
        "page_size": result.page_size,
        "has_more": result.has_more,
        "source": result.source,
        "warning": result.warning,
    })))
}

/// 获取全球封禁同步状态
pub(crate) async fn sync_status(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&actor) {
        return Err(AppError::forbidden());
    }

    let status = global_ban_service::sync_status(&ctx.db, ctx.config.global_ban_sync_interval_secs)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(serde_json::json!({ "status": status })))
}

/// 按 SteamID 搜索玩家全球封禁
pub(crate) async fn search_player_bans(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<SearchBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&_actor) {
        return Err(AppError::forbidden());
    }

    let result =
        global_ban_service::search_player_bans(&ctx.db, &ctx.steam_resolver, &body.steam_input)
            .await
            .map_err(AppError::internal)?;

    Ok(Json(serde_json::json!({
        "items": result.items,
        "source": result.source,
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchBody {
    pub steam_input: String,
}

/// 手动解封：解除全球封禁对应的本地封禁（不影响 KZTimer 全球封禁）
pub(crate) async fn manual_unban_global_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(kzt_ban_id): Path<i64>,
) -> Result<axum::http::StatusCode, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(AppError::forbidden());
    }

    global_ban_service::manual_unban(
        &ctx.db,
        &ctx.active_ban_cache,
        kzt_ban_id,
        &actor.display_name,
    )
    .await
    .map_err(AppError::bad_request)?;

    if let Err(e) = audit_service::write_audit_log_with_context(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "global_ban_unban".to_string(),
            target: kzt_ban_id.to_string(),
            target_type: "global_ban".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!(
                "后台手动解除全球封禁对应的本地封禁，KZTimer ID: {kzt_ban_id}"
            )),
            idempotency_key: None,
        },
        optional_client_ip(&headers),
        Some(serde_json::json!({
            "action": "manual_unban_global_ban",
            "kzt_ban_id": kzt_ban_id,
            "operator_username": actor.username,
            "operator_role": actor.role,
        })),
    )
    .await
    {
        tracing::warn!(%e, "global ban manual unban audit log write failed");
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// 手动触发同步（仅 developer/admin）
pub(crate) async fn trigger_sync(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(AppError::forbidden());
    }

    let result = global_ban_service::sync_global_bans(&ctx.db)
        .await
        .map_err(AppError::bad_request)?;

    if let Err(e) = audit_service::write_audit_log_with_context(
        &ctx.db,
        audit_service::AuditLogInput {
            operation: "global_ban_sync".to_string(),
            target: "kztimer_global_bans".to_string(),
            target_type: "global_ban".to_string(),
            player_name: None,
            reason: None,
            duration_minutes: None,
            operator_name: actor.display_name.clone(),
            operator_steamid: None,
            source: "web".to_string(),
            server_id: None,
            server_name: None,
            server_port: None,
            success: true,
            message: Some(format!(
                "手动同步全球封禁：获取 {}，新增 {}，修复 {}",
                result.total_fetched, result.new_bans, result.repaired_local_bans
            )),
            idempotency_key: None,
        },
        optional_client_ip(&headers),
        Some(serde_json::json!({
            "action": "sync_global_bans",
            "total_fetched": result.total_fetched,
            "new_bans": result.new_bans,
            "repaired_local_bans": result.repaired_local_bans,
            "expired": result.expired,
            "re_banned": result.re_banned,
            "operator_username": actor.username,
            "operator_role": actor.role,
        })),
    )
    .await
    {
        tracing::warn!(%e, "global ban sync audit log write failed");
    }

    Ok(Json(serde_json::json!({ "result": result })))
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}
