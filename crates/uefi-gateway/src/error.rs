use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    Auth,
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match self {
            AppError::Auth => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHENTICATED",
                "invalid or missing session".into(),
            ),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND", m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "INVALID_ARGUMENT", m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", m),
        };
        let body = json!({ "error": msg, "code": code });
        (status, axum::Json(body)).into_response()
    }
}

impl From<tonic::Status> for AppError {
    fn from(s: tonic::Status) -> Self {
        match s.code() {
            tonic::Code::Unauthenticated => AppError::Auth,
            tonic::Code::NotFound => AppError::NotFound(s.message().into()),
            tonic::Code::InvalidArgument => AppError::BadRequest(s.message().into()),
            _ => AppError::Internal(s.message().into()),
        }
    }
}
