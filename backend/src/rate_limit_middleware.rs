use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::services::rate_limit_service::{extract_client_ip, RateLimitError, RateLimiters};

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
    let headers = request.headers();

    // 根据路径选择不同的限流策略
    if path.starts_with("/api/public/") {
        let ip = extract_client_ip(headers);
        state.limiters.public_api.check(&ip).await?;
    } else if path.starts_with("/api/auth/login") || path.starts_with("/api/auth/logout") {
        let ip = extract_client_ip(headers);
        state.limiters.auth_api.check(&ip).await?;
    } else if path.starts_with("/api/plugin/") {
        let ip = extract_client_ip(headers);
        state.limiters.plugin_api.check(&ip).await?;
    } else if path.starts_with("/api/") {
        let user_key = extract_user_key(headers);
        state.limiters.admin_api.check(&user_key).await?;
    }

    Ok(next.run(request).await)
}

/// 从请求头提取用户标识
fn extract_user_key(headers: &HeaderMap) -> String {
    if let Some(auth) = headers.get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Bearer ") {
                return auth_str[7..].to_string();
            }
            return auth_str.to_string();
        }
    }
    extract_client_ip(headers)
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
