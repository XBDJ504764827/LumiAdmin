//! Steam OpenID 认证（供公共页面白名单申请使用）
//!
//! 流程：
//! 1. 前端调用 GET /api/public/steam/auth/login 获取回调 URL
//! 2. 前端构建 Steam OpenID URL 并重定向用户到 Steam 登录
//! 3. Steam 回调到 GET /api/public/steam/auth/callback
//! 4. 后端验证 OpenID 响应，创建会话，重定向回前端并附带 token
//! 5. 前端调用 GET /api/public/steam/auth/session?token=xxx 获取已验证的 Steam 信息

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use super::AppCtx;

const SESSION_TTL_HOURS: i64 = 1;

// ---------------------------------------------------------------------------
// GET /api/public/steam/auth/login
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub(crate) struct LoginQuery {
    /// 前端实际访问的 origin（如 http://192.168.0.138:5173）
    origin: Option<String>,
}

/// 从请求中推导前端实际访问的 origin，优先级：
/// 1. 前端显式传入的 origin 查询参数
/// 2. Origin 请求头
/// 3. Referer 请求头
/// 4. X-Forwarded-Proto + Host（反向代理场景）
/// 5. CORS_ORIGIN 配置
fn resolve_frontend_origin(
    query: &LoginQuery,
    headers: &HeaderMap,
    cors_origin: Option<&str>,
) -> String {
    query
        .origin
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("origin")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            headers
                .get("referer")
                .and_then(|v| v.to_str().ok())
                .and_then(|referer| {
                    // 只保留 scheme://host[:port]，去掉路径
                    let end = referer
                        .find('/')
                        .and_then(|i| referer[i + 2..].find('/').map(|j| i + 2 + j))
                        .unwrap_or(referer.len());
                    let origin = &referer[..end];
                    if origin.starts_with("http://") || origin.starts_with("https://") {
                        Some(origin.to_string())
                    } else {
                        None
                    }
                })
        })
        .or_else(|| {
            let proto = headers
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("http");
            headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .filter(|h| !h.is_empty())
                .map(|host| format!("{}://{}", proto, host))
        })
        .or_else(|| cors_origin.map(|s| s.trim_end_matches('/').to_string()))
        .unwrap_or_else(|| "http://localhost:5173".to_string())
}

pub(crate) async fn steam_auth_login(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // realm 和 callback_url 必须与前端实际访问的地址一致，
    // 否则 Steam 登录成功后回调会跳到错误地址（如 localhost）导致登录失败。
    let cors_origin = ctx.config.cors_origin.clone();
    let mut realm = resolve_frontend_origin(&query, &headers, cors_origin.as_deref());
    realm = realm.trim_end_matches('/').to_string();

    // 若配置了 CORS_ORIGIN（生产环境必填），校验推导出的 origin 必须与之一致，
    // 避免被恶意来源利用重定向。
    if let Some(allowed) = cors_origin.as_deref().map(|s| s.trim_end_matches('/')) {
        if realm != allowed {
            tracing::warn!(realm = %realm, allowed = %allowed, "Steam 登录来源与 CORS_ORIGIN 不一致");
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "不支持的来源" })),
            ));
        }
    }

    let callback_url = format!("{}/api/public/steam/auth/callback", realm);

    Ok(Json(serde_json::json!({
        "realm": realm,
        "callback_url": callback_url,
    })))
}

// ---------------------------------------------------------------------------
// GET /api/public/steam/auth/callback
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct CallbackParams {
    #[serde(rename = "openid.ns")]
    openid_ns: Option<String>,
    #[serde(rename = "openid.mode")]
    openid_mode: Option<String>,
    #[serde(rename = "openid.op_endpoint")]
    openid_op_endpoint: Option<String>,
    #[serde(rename = "openid.claimed_id")]
    openid_claimed_id: Option<String>,
    #[serde(rename = "openid.identity")]
    openid_identity: Option<String>,
    #[serde(rename = "openid.return_to")]
    openid_return_to: Option<String>,
    #[serde(rename = "openid.response_nonce")]
    openid_response_nonce: Option<String>,
    #[serde(rename = "openid.assoc_handle")]
    openid_assoc_handle: Option<String>,
    #[serde(rename = "openid.signed")]
    openid_signed: Option<String>,
    #[serde(rename = "openid.sig")]
    openid_sig: Option<String>,
}

pub(crate) async fn steam_auth_callback(
    State(ctx): State<AppCtx>,
    headers: axum::http::HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    // 优先从 X-Forwarded-Proto 和 X-Forwarded-Host 获取前端基础地址
    // 这在反向代理（如 nginx）后面运行时尤为重要
    let forwarded_proto = headers
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let forwarded_host = headers
        .get("X-Forwarded-Host")
        .and_then(|v| v.to_str().ok());

    // 从 return_to 中解析前端 origin，保证回调后跳回用户实际使用的地址
    let frontend_base = params
        .openid_return_to
        .as_deref()
        .and_then(|url| {
            // return_to 格式: http://host:port/api/public/steam/auth/callback
            url.strip_suffix("/api/public/steam/auth/callback")
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // 如果 X-Forwarded-Host 可用，用它构建正确的地址
            if let Some(host) = forwarded_host {
                Some(format!(
                    "{}://{}",
                    forwarded_proto,
                    host.trim_end_matches('/')
                ))
            } else {
                None
            }
        })
        .or_else(|| {
            ctx.config
                .cors_origin
                .as_deref()
                .map(|s| s.trim_end_matches('/').to_string())
        })
        .unwrap_or_else(|| "http://localhost:5173".to_string());

    // 构建回调出错的跳转地址（错误信息通过 query 参数带往前端）
    let error_redirect = |reason: &str| {
        Redirect::to(&format!(
            "{frontend_base}/public/apply?steam_auth=error&reason={}",
            reason
        ))
    };

    // 检查是否是取消
    let mode = params.openid_mode.as_deref().unwrap_or("");
    if mode == "cancel" {
        return Ok(Redirect::to(&format!(
            "{frontend_base}/public/apply?steam_auth=cancelled"
        )));
    }

    // 验证必需参数
    if mode != "id_res" {
        return Ok(error_redirect("invalid_mode"));
    }

    let claimed_id = match params.openid_claimed_id.as_deref() {
        Some(id) => id,
        None => return Ok(error_redirect("missing_claimed_id")),
    };

    // 从 claimed_id 提取 SteamID64
    let steamid64 = match extract_steamid64_from_claimed_id(claimed_id) {
        Some(id) => id,
        None => return Ok(error_redirect("invalid_claimed_id")),
    };

    // 验证 OpenID 签名——向 Steam 进行验证请求
    let valid = verify_steam_openid(&params).await.unwrap_or(false);
    if !valid {
        tracing::error!("Steam OpenID 验证失败");
        return Ok(error_redirect("verification_failed"));
    }

    // 获取 Steam 资料
    let persona_name = ctx
        .steam_resolver
        .fetch_profile(&steamid64)
        .await
        .ok()
        .flatten()
        .map(|p| p.persona_name);

    // 获取 Steam 等级
    let steam_level = ctx
        .steam_resolver
        .fetch_steam_level(&steamid64)
        .await
        .ok()
        .flatten();

    // 计算其他 Steam 标识符
    let (steamid, steamid3) = {
        let parsed = ctx.steam_resolver.parse_local(&steamid64);
        match parsed {
            Ok(p) => (p.steamid, p.steamid3),
            Err(_) => (None, None),
        }
    };
    let profile_url = format!("https://steamcommunity.com/profiles/{steamid64}");

    // 创建会话
    let session_id = Uuid::new_v4();
    let expires_at = Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);

    sqlx::query(
        r#"INSERT INTO public_steam_auth_sessions (id, steamid64, steamid, steamid3, profile_url, persona_name, steam_level, expires_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(session_id)
    .bind(&steamid64)
    .bind(&steamid)
    .bind(&steamid3)
    .bind(&profile_url)
    .bind(&persona_name)
    .bind(steam_level.map(|l| l as i32))
    .bind(expires_at)
    .execute(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "创建 Steam 认证会话失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "创建会话失败"})),
        )
    })?;

    // 重定向回前端，附带 session token
    Ok(Redirect::to(&format!(
        "{frontend_base}/public/apply?steam_token={session_id}"
    )))
}

// ---------------------------------------------------------------------------
// GET /api/public/steam/auth/session
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct SessionQuery {
    token: String,
}

pub(crate) async fn steam_auth_session(
    State(ctx): State<AppCtx>,
    Query(query): Query<SessionQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let session_id: Uuid = Uuid::parse_str(&query.token).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "无效的 token"})),
        )
    })?;

    let row: Option<(
        uuid::Uuid,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<i32>,
    )> = sqlx::query_as(
        r#"SELECT id, steamid64, steamid, steamid3, profile_url, persona_name, steam_level
               FROM public_steam_auth_sessions
               WHERE id = $1 AND expires_at > now()"#,
    )
    .bind(session_id)
    .fetch_optional(&ctx.db.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "查询 Steam 认证会话失败");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "查询会话失败"})),
        )
    })?;

    let (_id, steamid64, steamid, steamid3, profile_url, persona_name, steam_level) = row
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "会话不存在或已过期"})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "steamid64": steamid64,
        "steamid": steamid,
        "steamid3": steamid3,
        "profile_url": profile_url,
        "persona_name": persona_name,
        "steam_level": steam_level,
        "token": query.token,
    })))
}

// ---------------------------------------------------------------------------
// 提交白名单时验证 Steam 会话（供 public::submit_whitelist 调用）
// ---------------------------------------------------------------------------

/// 验证 Steam 认证会话，返回已验证的 SteamID64
pub(crate) async fn verify_steam_session(
    db: &crate::db::Database,
    token: &str,
) -> Result<String, anyhow::Error> {
    let session_id = Uuid::parse_str(token)?;

    let row: Option<(String,)> = sqlx::query_as(
        r#"SELECT steamid64 FROM public_steam_auth_sessions
           WHERE id = $1 AND expires_at > now()"#,
    )
    .bind(session_id)
    .fetch_optional(&db.pool)
    .await?;

    row.map(|(s,)| s)
        .ok_or_else(|| anyhow::anyhow!("Steam 认证会话不存在或已过期，请重新登录"))
}

// ——————————————————————————————————————————————————————————————————————————————
// 辅助函数
// ——————————————————————————————————————————————————————————————————————————————

/// 从 Steam OpenID claimed_id 提取 SteamID64
/// 格式: https://steamcommunity.com/openid/id/7656119XXXXXXXXXX
fn extract_steamid64_from_claimed_id(claimed_id: &str) -> Option<String> {
    let last = claimed_id.rsplit('/').next()?;
    if last.len() == 17 && last.chars().all(|c| c.is_ascii_digit()) {
        Some(last.to_string())
    } else {
        None
    }
}

/// 向 Steam 验证 OpenID 签名
async fn verify_steam_openid(params: &CallbackParams) -> Result<bool, anyhow::Error> {
    use crate::http_client;

    let mut verify_params: Vec<(&str, &str)> = vec![
        ("openid.ns", "http://specs.openid.net/auth/2.0"),
        ("openid.mode", "check_authentication"),
    ];

    // 将接收到的全部参数回传给 Steam 验证
    if let Some(ref v) = params.openid_op_endpoint {
        verify_params.push(("openid.op_endpoint", v));
    }
    if let Some(ref v) = params.openid_claimed_id {
        verify_params.push(("openid.claimed_id", v));
    }
    if let Some(ref v) = params.openid_identity {
        verify_params.push(("openid.identity", v));
    }
    if let Some(ref v) = params.openid_return_to {
        verify_params.push(("openid.return_to", v));
    }
    if let Some(ref v) = params.openid_response_nonce {
        verify_params.push(("openid.response_nonce", v));
    }
    if let Some(ref v) = params.openid_assoc_handle {
        verify_params.push(("openid.assoc_handle", v));
    }
    if let Some(ref v) = params.openid_signed {
        verify_params.push(("openid.signed", v));
    }
    if let Some(ref v) = params.openid_sig {
        verify_params.push(("openid.sig", v));
    }

    let response = http_client::http_client()
        .post("https://steamcommunity.com/openid/login")
        .form(&verify_params)
        .send()
        .await?;

    let body = response.text().await?;

    Ok(body.contains("is_valid:true"))
}
