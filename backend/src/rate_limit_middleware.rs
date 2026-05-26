use axum::{
    extract::ConnectInfo,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use crate::services::rate_limit_service::{RateLimitError, RateLimiters};

/// 限流中间件状态
#[derive(Clone)]
pub struct RateLimitState {
    pub limiters: Arc<RateLimiters>,
}

/// 限流中间件
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitResponse> {
    let path = request.uri().path();
    let method = request.method().clone();
    let headers = request.headers();
    let peer_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());
    let rate_limit_ip = client_ip_for_rate_limit(headers, peer_ip);

    // 根据路径选择不同的限流策略
    if path.starts_with("/api/public/") {
        if method == Method::GET {
            state.limiters.public_read_api.check(&rate_limit_ip).await?;
        } else {
            state.limiters.public_api.check(&rate_limit_ip).await?;
        }
    } else if path.starts_with("/api/auth/login") || path.starts_with("/api/auth/logout") {
        state.limiters.auth_api.check(&rate_limit_ip).await?;
    } else if path.starts_with("/api/plugin/") {
        state.limiters.plugin_api.check(&rate_limit_ip).await?;
    } else if path.starts_with("/api/") {
        let user_key = extract_user_key(headers, &rate_limit_ip);
        state.limiters.admin_api.check(&user_key).await?;
    }

    Ok(next.run(request).await)
}

fn client_ip_for_rate_limit(headers: &HeaderMap, peer_ip: Option<IpAddr>) -> String {
    let fallback = peer_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if !peer_ip.is_some_and(|ip| ip.is_loopback()) {
        return fallback;
    }

    forwarded_client_ip(headers).unwrap_or(fallback)
}

fn forwarded_client_ip(headers: &HeaderMap) -> Option<String> {
    let real_ip = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim());
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim());

    real_ip
        .into_iter()
        .chain(forwarded)
        .find(|candidate| candidate.parse::<IpAddr>().is_ok())
        .map(ToString::to_string)
}

/// 从请求头提取用户标识
fn extract_user_key(headers: &HeaderMap, fallback_ip: &str) -> String {
    if let Some(auth) = headers.get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token.to_string();
            }
            return auth_str.to_string();
        }
    }
    fallback_ip.to_string()
}

/// 限流错误响应
#[derive(Debug)]
pub struct RateLimitResponse {
    pub retry_after: u64,
    pub limit_name: String,
}

impl From<RateLimitError> for RateLimitResponse {
    fn from(err: RateLimitError) -> Self {
        Self {
            retry_after: err.retry_after,
            limit_name: err.limit_name,
        }
    }
}

impl IntoResponse for RateLimitResponse {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": "too_many_requests",
            "message": format!("请求过于频繁，请 {} 秒后重试", self.retry_after),
            "retry_after": self.retry_after,
            "limit": self.limit_name,
        });

        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Content-Type", "application/json"),
                ("Retry-After", &self.retry_after.to_string()),
                ("X-RateLimit-Limit", &self.limit_name),
            ],
            body.to_string(),
        )
            .into_response()
    }
}
