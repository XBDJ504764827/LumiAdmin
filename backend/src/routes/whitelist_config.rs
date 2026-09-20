use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;

use crate::routes::{current_operator, AppCtx, AppError};
use crate::services::{
    log_service, permission_service, rate_limit_service, whitelist_auto_approve_service,
    whitelist_qq_service,
};

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct AutoApproveConfigBody {
    pub enabled: bool,
    pub hours: Option<i64>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct QqConfigBody {
    pub enabled: bool,
    pub bot_name: Option<String>,
    pub bot_qq: Option<String>,
    pub max_bindings: Option<i32>,
    pub code_ttl_seconds: Option<i32>,
}

/// 获取白名单自动通过配置（开关 + 等待时长）
pub(crate) async fn get_auto_approve_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let _actor = current_operator(&ctx, &headers).await?;
    let config = whitelist_auto_approve_service::load_config(&ctx.db)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(serde_json::json!({ "config": config })))
}

/// 更新白名单自动通过配置（开关 + 等待时长）
pub(crate) async fn update_auto_approve_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<AutoApproveConfigBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_manually(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let config = whitelist_auto_approve_service::update_config(
        &ctx.db,
        body.enabled,
        body.hours.unwrap_or(3) as i32,
        &operator_name,
    )
    .await
    .map_err(AppError::bad_request)?;
    let _ = log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "修改自动通过设置",
        &format!(
            "自动通过开关：{}，等待时长：{} 小时",
            if config.enabled { "开启" } else { "关闭" },
            config.hours
        ),
        &rate_limit_service::extract_client_ip(&headers),
    )
    .await;
    Ok(Json(serde_json::json!({ "config": config })))
}

/// 获取白名单 QQ 群配置（仅开发管理员可见）。
pub(crate) async fn get_qq_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_qq_config(&actor) {
        return Err(AppError::forbidden());
    }
    let config = whitelist_qq_service::load_config(&ctx.db)
        .await
        .map_err(AppError::internal)?;
    Ok(Json(serde_json::json!({ "config": config })))
}

/// 更新白名单 QQ 群配置（仅开发管理员可改）。
pub(crate) async fn update_qq_config(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<QqConfigBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor = current_operator(&ctx, &headers).await?;
    if !permission_service::can_manage_whitelist_qq_config(&actor) {
        return Err(AppError::forbidden());
    }
    let operator_name = actor.display_name.clone();
    let config = whitelist_qq_service::update_config(
        &ctx.db,
        body.enabled,
        body.bot_name
            .as_deref()
            .unwrap_or(whitelist_qq_service::DEFAULT_BOT_NAME),
        body.bot_qq
            .as_deref()
            .unwrap_or(whitelist_qq_service::DEFAULT_BOT_QQ),
        body.max_bindings
            .unwrap_or(whitelist_qq_service::DEFAULT_MAX_BINDINGS),
        body.code_ttl_seconds
            .unwrap_or(whitelist_qq_service::DEFAULT_CODE_TTL_SECONDS),
        &operator_name,
    )
    .await
    .map_err(AppError::bad_request)?;
    let _ = log_service::log_action(
        &ctx.db,
        &operator_name,
        "白名单管理",
        "修改QQ绑定设置",
        &format!(
            "启用：{}，机器人：{}（QQ {}），单QQ上限：{}，验证码有效期：{}秒",
            if config.enabled { "是" } else { "否" },
            config.bot_name,
            config.bot_qq,
            config.max_bindings,
            config.code_ttl_seconds
        ),
        &rate_limit_service::extract_client_ip(&headers),
    )
    .await;
    Ok(Json(serde_json::json!({ "config": config })))
}
