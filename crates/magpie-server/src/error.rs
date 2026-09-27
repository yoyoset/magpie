//! API errors: a stable machine-readable `code`, a human message, and no
//! internal detail leaking to the client (that goes to the log).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use magpie_core::{ScheduleError, ValidationError};
use magpie_store::StoreError;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("missing or invalid bearer token")]
    Unauthorized,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("internal error")]
    Internal(String),
}

impl ApiError {
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal(detail.into())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Invalid(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(detail) = &self {
            tracing::error!(%detail, "API internal error");
        }
        let (status, code) = self.status_and_code();
        let body = json!({ "error": { "code": code, "message": self.to_string() } });
        (status, Json(body)).into_response()
    }
}

impl From<ValidationError> for ApiError {
    fn from(e: ValidationError) -> Self {
        Self::Invalid(e.to_string())
    }
}

impl From<ScheduleError> for ApiError {
    fn from(e: ScheduleError) -> Self {
        Self::Invalid(e.to_string())
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::EventNotFound(id) => Self::NotFound(format!("event {id} not found")),
            other => Self::Internal(other.to_string()),
        }
    }
}
