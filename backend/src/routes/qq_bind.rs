//! QQ 群绑定路由
//!
//! - 玩家（Steam 已验证）：生成绑定码 / 查询绑定状态
//! - LumiBot（QQ 集成令牌）：用群消息中的绑定码 + 发送者 openid 完成绑定

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use super::{invalid_request, AppCtx};
use crate::services::qq_bind_service;

// ---------------------------------------------------------------------------
// 玩家侧接口
// ---------------------------------------------------------------------------

/// POST /api/public/steam/auth/bind/code
/// 玩家（携带 steam_token）生成一次性 QQ 绑定码
#[derive(Deserialize)]
pub(crate) struct CreateBindCodeBody {
    steam_token: String,
}

pub(crate) async fn create_bind_code(
    State(ctx): State<AppCtx>,
    Json(body): Json<CreateBindCodeBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let steamid64 = crate::routes::steam_auth::verify_steam_session(&ctx.db, &body.steam_token)
        .await
        .map_err(invalid_request)?;

    let (code, expires_at) = qq_bind_service::create_bind_code(&ctx.db, &steamid64)
        .await
        .map_err(invalid_request)?;

    Ok(Json(serde_json::json!({
        "code": code,
        "expires_at": expires_at,
        "steamid64": steamid64,
    })))
}

/// GET /api/public/qq/bind/status?steam_token=xxx
/// 查询玩家绑定状态（是否已绑定 + 当前 pending 码）
#[derive(Deserialize)]
pub(crate) struct BindStatusQuery {
    steam_token: String,
}

pub(crate) async fn bind_status(
    State(ctx): State<AppCtx>,
    Query(query): Query<BindStatusQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let steamid64 = crate::routes::steam_auth::verify_steam_session(&ctx.db, &query.steam_token)
        .await
        .map_err(invalid_request)?;

    let binding = qq_bind_service::get_binding(&ctx.db, &steamid64)
        .await
        .map_err(invalid_request)?;
    let pending_code = qq_bind_service::get_pending_code(&ctx.db, &steamid64)
        .await
        .map_err(invalid_request)?;

    let (code, expires_at) = match pending_code {
        Some((code, expires_at)) => (Some(code), Some(expires_at)),
        None => (None, None),
    };

    Ok(Json(serde_json::json!({
        "steamid64": steamid64,
        "qq_openid": binding.as_ref().map(|b| b.qq_openid.clone()),
        "qq_openid_masked": binding.as_ref().map(|b| mask_openid(&b.qq_openid)),
        "bound": binding.is_some(),
        "pending_code": code,
        "pending_code_expires_at": expires_at,
    })))
}

// ---------------------------------------------------------------------------
// LumiBot 侧接口（QQ 集成令牌鉴权）
// ---------------------------------------------------------------------------

/// POST /api/qq/bind
/// LumiBot 调用：群消息中收到 @机器人 <UUID> 后，携带发送者 openid 绑定
#[derive(Deserialize)]
pub(crate) struct BindByBotBody {
    code: Uuid,
    qq_openid: String,
}

pub(crate) async fn bind_by_bot(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(body): Json<BindByBotBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 验证 QQ 集成令牌（与 public::qq_review_stats 一致的鉴权方式）
    let token = headers
        .get("x-qq-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        });

    let expected_token = ctx.config.qq_integration_token.as_deref();

    let authed = match (token, expected_token) {
        (Some(provided), Some(expected)) => {
            constant_time_eq(provided.as_bytes(), expected.as_bytes())
        }
        _ => false,
    };
    if !authed {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": if expected_token.is_some() {
                    "无效的集成令牌"
                } else {
                    "QQ 集成未启用。请在后端配置 QQ_INTEGRATION_TOKEN 环境变量。"
                }
            })),
        ));
    }

    let result = qq_bind_service::bind_with_code(&ctx.db, body.code, &body.qq_openid)
        .await
        .map_err(invalid_request)?;

    match result {
        qq_bind_service::BindResult::Bound { steamid64 } => Ok(Json(serde_json::json!({
            "success": true,
            "steamid64": steamid64,
            "message": "绑定成功"
        }))),
        qq_bind_service::BindResult::CodeInvalid => Ok(Json(serde_json::json!({
            "success": false,
            "error": "code_invalid",
            "message": "绑定码不存在或已失效"
        }))),
        qq_bind_service::BindResult::CodeUsed => Ok(Json(serde_json::json!({
            "success": false,
            "error": "code_used",
            "message": "绑定码已被使用"
        }))),
        qq_bind_service::BindResult::CodeExpired => Ok(Json(serde_json::json!({
            "success": false,
            "error": "code_expired",
            "message": "绑定码已过期，请重新生成"
        }))),
    }
}

/// 脱敏 openid：保留前 5 位 + 后 2 位（示例：ABCDE****CD）
fn mask_openid(openid: &str) -> String {
    let chars: Vec<char> = openid.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let head: String = chars[..5].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}****{tail}")
}

/// 恒定时间比较（防止时序攻击）
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
