//! 统一 CORS 策略中间件
//!
//! 按路径分流：
//! - `/webhook/*`：公开 API（玩家信息 API 端点，供其他网站/系统调用，
//!   靠端点 token 鉴权），放行所有来源
//! - 其余路径：仅允许 CORS_ORIGIN 白名单来源（管理后台等）

use axum::{
    extract::{Request, State},
    http::{
        header::{self, HeaderValue},
        StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct CorsState {
    pub allowed_origins: Arc<Vec<String>>,
}

const ALLOW_METHODS: &str = "GET, POST, PUT, DELETE, OPTIONS";
const ALLOW_HEADERS: &str = "Authorization, Content-Type";

pub async fn cors_middleware(
    State(state): State<CorsState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let is_public_webhook = path.starts_with("/webhook/");

    let origin = request.headers().get(header::ORIGIN).cloned();
    let is_preflight = request.method() == axum::http::Method::OPTIONS
        && origin.is_some()
        && request
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);

    // 计算允许的 origin：公开 webhook 回显任意来源，其余仅白名单
    let allowed_origin = origin.and_then(|origin_value| {
        if is_public_webhook {
            Some(origin_value)
        } else {
            let origin_str = origin_value.to_str().ok()?;
            state
                .allowed_origins
                .iter()
                .any(|allowed| allowed == origin_str)
                .then_some(origin_value)
        }
    });

    // 预检请求（OPTIONS + Origin + Access-Control-Request-Method）
    if is_preflight {
        return match allowed_origin {
            Some(origin_value) => {
                let mut response = StatusCode::OK.into_response();
                let headers = response.headers_mut();
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_value);
                headers.insert(header::VARY, HeaderValue::from_static("Origin"));
                headers.insert(
                    header::ACCESS_CONTROL_ALLOW_METHODS,
                    HeaderValue::from_static(ALLOW_METHODS),
                );
                headers.insert(
                    header::ACCESS_CONTROL_ALLOW_HEADERS,
                    HeaderValue::from_static(ALLOW_HEADERS),
                );
                headers.insert(
                    header::ACCESS_CONTROL_MAX_AGE,
                    HeaderValue::from_static("86400"),
                );
                response
            }
            None => StatusCode::FORBIDDEN.into_response(),
        };
    }

    let mut response = next.run(request).await;
    if let Some(origin_value) = allowed_origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_value);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}
