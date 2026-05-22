use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{AppCtx, ListQuery, current_operator, forbidden, invalid_request};
use crate::services::{permission_service, whitelist_service, log_service};
use crate::services::rate_limit_service::extract_client_ip;

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistBody {
    pub steam_input: String,
    pub nickname: String,
    pub operator_name: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct RejectWhitelistBody {
    pub reason: String,
    pub operator_name: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct WhitelistActionBody {
    pub operator_name: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RefreshSteamNamesBody {
    pub status: Option<String>,
}

pub(crate) async fn whitelist(State(ctx): State<AppCtx>, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = whitelist_service::list_whitelist(&ctx.db, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

pub(crate) async fn create_whitelist(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<WhitelistBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let resolver = &ctx.steam_resolver;
    let item = whitelist_service::create_manual_whitelist(
        &ctx.db,
        whitelist_service::ManualWhitelistInput {
            nickname: body.nickname,
            steam_input: body.steam_input,
        },
        &operator_name,
        &resolver,
    )
    .await
    .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "手动添加白名单", &item.nickname, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

pub(crate) async fn approve_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::approve_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "通过白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn reject_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectWhitelistBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::reject_whitelist(&ctx.db, id, &body.reason, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "拒绝白名单申请", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn restore_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::restore_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "恢复白名单通过", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn revoke_whitelist_request(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(_body): Json<WhitelistActionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_revoke_whitelist(&actor) {
        return Err(forbidden());
    }
    let operator_name = actor.display_name.clone();
    let item = whitelist_service::revoke_whitelist(&ctx.db, id, &operator_name)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "删除白名单审核", &item.nickname, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

/// 刷新单条白名单记录的Steam名称
pub(crate) async fn refresh_single_steam_name(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let resolver = &ctx.steam_resolver;
    let steam_name = whitelist_service::update_steam_persona_name(&ctx.db, id, &resolver)
        .await
        .map_err(invalid_request)?;
    Ok(Json(serde_json::json!({ "steam_persona_name": steam_name })))
}

/// 批量刷新白名单记录的Steam名称
pub(crate) async fn refresh_all_steam_names(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<RefreshSteamNamesBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_review_whitelist(&actor) {
        return Err(forbidden());
    }
    let resolver = &ctx.steam_resolver;
    let updated_count = whitelist_service::refresh_all_steam_persona_names(&ctx.db, &resolver, body.status.as_deref())
        .await
        .map_err(invalid_request)?;
    let operator_name = actor.display_name.clone();
    let _ = log_service::create_log(&ctx.db, &operator_name, "白名单管理", "批量刷新Steam名称", &format!("更新了{}条记录", updated_count), &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({ "updated_count": updated_count })))
}
