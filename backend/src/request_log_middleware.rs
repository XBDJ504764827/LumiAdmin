use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::info;

use crate::services::observability_service;

/// 请求日志中间件：记录方法、路径、状态码、耗时
pub async fn request_log_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status();
    let elapsed_ms = start.elapsed().as_millis();
    observability_service::record_http_request(status.as_u16(), elapsed_ms as u64);

    if should_log_request(&path, status.as_u16(), elapsed_ms) {
        info!(
            method = %method,
            path = %path,
            status = status.as_u16(),
            duration_ms = elapsed_ms,
            "request"
        );
    }

    response
}

fn should_log_request(path: &str, status: u16, elapsed_ms: u128) -> bool {
    if status >= 400 || elapsed_ms >= 1_000 {
        return true;
    }

    !matches!(
        path,
        "/api/review-counts" | "/api/notifications/unread-count"
    ) && !path.starts_with("/api/player-detail/internal/")
}
