use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

use crate::routes::{current_operator, forbidden, invalid_request, AppCtx};
use crate::services::{global_ban_service, permission_service};

/// 实时获取全球封禁列表（直接从 KZTimer API 拉取，合并本地状态）
pub(crate) async fn list_global_bans(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_view_access_logs(&actor) {
        return Err(forbidden());
    }

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);

    match global_ban_service::fetch_live_global_bans(&ctx.db, page, page_size).await {
        Ok(result) => Ok(Json(serde_json::json!({
            "items": result.items,
            "page": result.page,
            "page_size": result.page_size,
            "has_more": result.has_more,
        }))),
        Err(e) => {
            tracing::error!(%e, "实时查询全球封禁失败");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("查询失败: {}", e) })),
            ))
        }
    }
}

/// 手动解封：解除全球封禁对应的本地封禁（不影响 KZTimer 全球封禁）
pub(crate) async fn manual_unban_global_ban(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(kzt_ban_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_ban(&actor) {
        return Err(forbidden());
    }

    global_ban_service::manual_unban(
        &ctx.db,
        &ctx.active_ban_cache,
        kzt_ban_id,
        &actor.display_name,
    )
    .await
    .map_err(|e| invalid_request(e))?;

    Ok(StatusCode::NO_CONTENT)
}

/// 手动触发同步（仅 developer/admin）
pub(crate) async fn trigger_sync(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    // 仅 developer/admin 可以手动同步
    if !permission_service::can_manage_community_mutation(&actor) {
        return Err(forbidden());
    }

    match global_ban_service::sync_global_bans(&ctx.db).await {
        Ok(result) => Ok(Json(serde_json::json!({ "result": result }))),
        Err(e) => {
            tracing::error!(%e, "手动同步全球封禁失败");
            Err(invalid_request(e))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}
