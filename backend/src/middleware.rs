use axum::{extract::State, http::{header, HeaderMap, StatusCode}, response::Response, middleware::Next};

use crate::{routes::AppCtx, services::auth_service};

pub async fn auth(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    next.run(req).await
}

pub async fn require_auth(State(ctx): State<AppCtx>, headers: HeaderMap) -> Result<(), StatusCode> {
    let token = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?;
    let token = uuid::Uuid::parse_str(token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    auth_service::current_session(&ctx.db, token).await.map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(())
}
