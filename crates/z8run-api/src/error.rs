//! HTTP error handling.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// API HTTP error.
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.status.as_u16(),
                "message": self.message,
            }
        });
        (self.status, Json(body)).into_response()
    }
}

impl From<z8run_core::Z8Error> for ApiError {
    fn from(e: z8run_core::Z8Error) -> Self {
        Self::internal(e.to_string())
    }
}

impl From<z8run_storage::StorageError> for ApiError {
    fn from(e: z8run_storage::StorageError) -> Self {
        match &e {
            z8run_storage::StorageError::FlowNotFound(_) => Self::not_found(e.to_string()),
            _ => Self::internal(e.to_string()),
        }
    }
}
