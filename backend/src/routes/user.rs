use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{AppCtx, ListQuery, current_operator, forbidden, invalid_request};
use crate::services::{log_service, permission_service, rate_limit_service::extract_client_ip, user_service};

#[derive(Deserialize)]
pub(crate) struct CreateUserBody {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) role: String,
    pub(crate) steam_id: Option<String>,
    pub(crate) remark: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpdateUserBody {
    pub(crate) username: String,
    pub(crate) role: Option<String>,
    pub(crate) steam_id: Option<String>,
    pub(crate) remark: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpdatePasswordBody {
    pub(crate) password: String,
}

pub(crate) async fn users(State(ctx): State<AppCtx>, headers: HeaderMap, Query(query): Query<ListQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let actor = current_operator(&ctx, &headers).await.map_err(|_| StatusCode::UNAUTHORIZED)?;
    let result = user_service::list_users(&ctx.db, &actor, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "items": result.items, "total": result.total, "page": result.page, "page_size": result.page_size })))
}

pub(crate) async fn create_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_create_admin_user(&actor) {
        return Err(forbidden());
    }

    let item = user_service::create_user(
        &ctx.db,
        user_service::CreateUserInput {
            username: body.username,
            password: body.password,
            role: body.role,
            steam_id: body.steam_id,
            remark: body.remark,
        },
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "新增管理员", &item.username, &extract_client_ip(&headers)).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"item": item}))))
}

pub(crate) async fn update_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_manage_user(&actor, &target) {
        return Err(forbidden());
    }

    let keep_role = actor.role == "normal" || !permission_service::can_change_user_role(&actor, &target);
    let item = user_service::update_user(
        &ctx.db,
        id,
        user_service::UpdateUserInput {
            username: body.username,
            role: body.role,
            steam_id: body.steam_id,
            remark: body.remark,
        },
        keep_role,
    )
    .await
    .map_err(invalid_request)?;

    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "修改管理员信息", &item.username, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}

pub(crate) async fn update_user_password(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePasswordBody>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_manage_user(&actor, &target) {
        return Err(forbidden());
    }

    user_service::update_password(&ctx.db, id, &body.password)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "修改管理员密码", &target.username, &extract_client_ip(&headers)).await;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn delete_user(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_delete_user(&actor, &target) {
        return Err(forbidden());
    }

    user_service::delete_user(&ctx.db, id)
        .await
        .map_err(invalid_request)?;
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", "删除管理员", &target.username, &extract_client_ip(&headers)).await;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn toggle_user_enabled(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    let target = user_service::find_user(&ctx.db, id).await.map_err(invalid_request)?;
    if !permission_service::can_toggle_user_enabled(&actor, &target) {
        return Err(forbidden());
    }

    let item = user_service::toggle_enabled(&ctx.db, id)
        .await
        .map_err(invalid_request)?;

    let action = if item.enabled { "启用账号" } else { "禁用账号" };
    let _ = log_service::create_log(&ctx.db, &actor.display_name, "网站用户管理", action, &item.username, &extract_client_ip(&headers)).await;
    Ok(Json(serde_json::json!({"item": item})))
}
