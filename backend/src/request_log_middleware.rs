use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::info;

use crate::services::observability_service;

/// 请求日志中间件：记录方法、路径、状态码、耗时
pub async fn request_log_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status();
    let elapsed_ms = start.elapsed().as_millis();
    observability_service::record_http_request(status.as_u16(), elapsed_ms as u64);

    info!(
        method = %method,
        path = %uri.path(),
        status = status.as_u16(),
        duration_ms = elapsed_ms,
        "request"
    );

    response
}
