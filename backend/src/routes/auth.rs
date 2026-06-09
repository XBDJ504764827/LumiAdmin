use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::routes::{current_operator, forbidden, AppCtx};
use crate::services::{auth_service, log_service, rate_limit_service::extract_client_ip};

#[derive(Deserialize)]
pub(crate) struct LoginBody {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub(crate) struct LogoutAllBody {
    pub current_token: Uuid,
}

pub(crate) async fn login(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let resp = auth_service::login(
        &ctx.db,
        &body.username,
        &body.password,
        ctx.config.session_ttl_hours,
    )
    .await
    .map_err(|err| {
        tracing::warn!(username = %body.username, error = %err, "login failed");
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"用户名或密码错误"})),
        )
    })?;
    if let Err(e) = log_service::create_log(
        &ctx.db,
        &body.username,
        "认证",
        "登录成功",
        &body.username,
        &extract_client_ip(&headers),
    )
    .await
    {
        tracing::warn!(%e, "日志写入失败");
    }
    Ok(Json(serde_json::json!({"session": resp.session})))
}

pub(crate) async fn logout(State(ctx): State<AppCtx>, headers: HeaderMap) -> StatusCode {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        if let Ok(token) = Uuid::parse_str(token) {
            let _ = auth_service::logout(&ctx.db, token).await;
        }
    }
    StatusCode::NO_CONTENT
}

pub(crate) async fn me(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token = Uuid::parse_str(token).map_err(|e| {
        tracing::warn!(error = %e, "token parse failed");
        StatusCode::UNAUTHORIZED
    })?;
    let session = auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "session lookup failed");
            StatusCode::UNAUTHORIZED
        })?;

    let enabled: (bool,) = sqlx::query_as("SELECT enabled FROM users WHERE id = $1")
        .bind(session.user_id)
        .fetch_optional(&ctx.db.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "查询用户启用状态失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !enabled.0 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Json(serde_json::json!({"session": session})))
}

/// 登出当前用户的所有其他设备（保留当前 session）
pub(crate) async fn logout_all_devices(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<LogoutAllBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let token_str = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token = Uuid::parse_str(token_str).map_err(|e| {
        tracing::warn!(error = %e, "token parse failed");
        StatusCode::UNAUTHORIZED
    })?;
    if body.current_token != token {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let session = auth_service::current_session(&ctx.db, token)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "session lookup failed");
            StatusCode::UNAUTHORIZED
        })?;

    let count = auth_service::logout_all_for_user(&ctx.db, session.user_id, Some(token))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "登出所有设备失败");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({ "revoked_count": count })))
}

/// 管理员强制登出指定用户的所有设备
pub(crate) async fn revoke_user_sessions(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let actor = current_operator(&ctx, &headers).await?;
    if !matches!(actor.role.as_str(), "admin" | "developer") {
        return Err(forbidden());
    }

    let count = auth_service::logout_all_for_user(&ctx.db, user_id, None)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user_id = %user_id, "强制登出用户失败");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "操作失败"})),
            )
        })?;

    let _ = log_service::create_log(
        &ctx.db,
        &actor.display_name,
        "网站用户管理",
        "强制登出",
        &user_id.to_string(),
        &extract_client_ip(&headers),
    )
    .await;
    Ok(Json(serde_json::json!({ "revoked_count": count })))
}
