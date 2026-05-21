use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(ErrorBody { error: self.message })).into_response()
    }
}

impl AppError {
    pub fn unauthorized(message: impl Into<String>) -> Self { Self { status: StatusCode::UNAUTHORIZED, message: message.into() } }
    pub fn forbidden(message: impl Into<String>) -> Self { Self { status: StatusCode::FORBIDDEN, message: message.into() } }
    pub fn bad_request(message: impl Into<String>) -> Self { Self { status: StatusCode::BAD_REQUEST, message: message.into() } }
    pub fn internal(message: impl Into<String>) -> Self { Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() } }
}
