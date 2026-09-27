//! HTTP adapter for reminder operations. Logic lives in `crate::ops`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use magpie_core::{Reminder, ReminderId};
use uuid::Uuid;

use crate::ops::{self, NewReminder};
use crate::{ApiError, AppState};

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewReminder>,
) -> Result<(StatusCode, Json<Reminder>), ApiError> {
    let reminder = ops::create_reminder(&state, body).await?;
    Ok((StatusCode::CREATED, Json(reminder)))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Reminder>>, ApiError> {
    Ok(Json(ops::list_reminders(&state).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ops::delete_reminder(&state, ReminderId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
