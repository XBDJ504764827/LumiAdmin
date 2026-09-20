use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;

use crate::routes::{current_operator, AppCtx, AppError};
use crate::services::{
    log_service, permission_service, rate_limit_service, whitelist_auto_approve_service,
};

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct AutoApproveConfigBody {
    pub enabled: bool,
    pub hours: Option<i64>,
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
