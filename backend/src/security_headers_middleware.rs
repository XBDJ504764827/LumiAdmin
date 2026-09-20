//! 统一安全响应头中间件
//!
//! 为所有响应附加基础安全头，降低点击劫持 / MIME 嗅探 / 信息泄露风险：
//! - `X-Content-Type-Options: nosniff`：禁止 MIME 嗅探
//! - `X-Frame-Options: DENY` + `Content-Security-Policy: frame-ancestors 'none'`：禁止被 iframe 嵌入
//! - `Referrer-Policy: no-referrer`：不泄露来源
//! - `Permissions-Policy`：禁用相机/麦克风/定位等浏览器能力（本系统不需要）
//! - `Strict-Transport-Security`：仅在 `APP_ENV=production`（假定经 TLS 反代）时附加
//!
//! 注：浏览器 HTML 页面（React 构建产物）由 nginx 静态托管，需在 nginx 层
//! 为 `index.html` 配置同样的头（见 README 部署一节，本中间件覆盖 API / webhook 响应）。

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue},
    middleware::Next,
    response::Response,
};

#[derive(Clone, Copy)]
pub struct SecurityHeadersState {
    /// 生产环境附加 HSTS（假定部署在 TLS 反代之后）
    pub hsts_enabled: bool,
}

pub async fn security_headers_middleware(
    State(state): State<SecurityHeadersState>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        axum::http::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    if state.hsts_enabled {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    response
}
