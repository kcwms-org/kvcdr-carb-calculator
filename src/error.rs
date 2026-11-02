use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unknown engine: {0}")]
    UnknownEngine(String),

    #[error("AI engine error: {0}")]
    EngineError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Multipart error: {0}")]
    MultipartError(String),

    #[error("JSON parse error: {0}")]
    JsonParseError(String),

    #[error("HTTP client error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::UnknownEngine(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::MultipartError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::JsonParseError(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            AppError::EngineError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::HttpError(e) if e.is_timeout() => (
                StatusCode::GATEWAY_TIMEOUT,
                "AI provider did not respond in time".to_string(),
            ),
            AppError::HttpError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        };

        // Log the full error (including internal details hidden from the client).
        if status.is_server_error() {
            tracing::error!(status = status.as_u16(), error = %self, "request failed");
        } else {
            tracing::warn!(status = status.as_u16(), error = %self, "request rejected");
        }

        (status, Json(json!({ "error": message }))).into_response()
    }
}
