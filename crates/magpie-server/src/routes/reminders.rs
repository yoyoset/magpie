use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use magpie_core::{EventDraft, EventId, Reminder, ReminderId, Schedule};
use serde::Deserialize;
use uuid::Uuid;

use crate::{ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    schedule: Schedule,
    template: EventDraft,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Reminder>), ApiError> {
    let now = state.clock.now();
    // Fail now, not at 3 a.m. when the reminder fires with an empty title.
    body.template.clone().validate(EventId::new(), now)?;
    let reminder = Reminder {
        id: ReminderId::new(),
        schedule: body.schedule,
        template: body.template,
        enabled: body.enabled,
        created_at: now,
        last_fired: None,
    };
    let stored = reminder.clone();
    state
        .with_store(move |s| Ok(s.save_reminder(&stored)?))
        .await?;
    state.changed.notify_one();
    Ok((StatusCode::CREATED, Json(reminder)))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Reminder>>, ApiError> {
    Ok(Json(state.with_store(|s| Ok(s.reminders()?)).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let id = ReminderId(id);
    let deleted = state
        .with_store(move |s| Ok(s.delete_reminder(id)?))
        .await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("reminder {id} not found")));
    }
    state.changed.notify_one();
    Ok(StatusCode::NO_CONTENT)
}
