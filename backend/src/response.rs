use axum::{http::StatusCode, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Serialize)]
pub struct ApiListResponse<T> {
    pub items: Vec<T>,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

pub fn ok<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse { data })
}

pub fn list<T: Serialize>(items: Vec<T>) -> Json<ApiListResponse<T>> {
    Json(ApiListResponse { items })
}

pub fn err(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into() }))
}
