//! HTTP adapter for event operations. Logic lives in `crate::ops`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use magpie_core::{AckKind, Event, EventDraft, EventId};
use serde::Deserialize;
use uuid::Uuid;

use crate::ops::{self, EventDetail, EventView, ListQuery};
use crate::{ApiError, AppState};

pub async fn create(
    State(state): State<AppState>,
    Json(draft): Json<EventDraft>,
) -> Result<(StatusCode, Json<Event>), ApiError> {
    let event = ops::create_event(&state, draft).await?;
    Ok((StatusCode::CREATED, Json(event)))
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<EventView>>, ApiError> {
    Ok(Json(ops::list_events(&state, q).await?))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EventDetail>, ApiError> {
    Ok(Json(ops::event_detail(&state, EventId(id)).await?))
}

#[derive(Debug, Deserialize)]
pub struct AckBody {
    #[serde(flatten)]
    kind: AckKind,
    via: String,
}

pub async fn ack(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AckBody>,
) -> Result<StatusCode, ApiError> {
    ops::ack_event(&state, EventId(id), body.kind, &body.via).await?;
    Ok(StatusCode::NO_CONTENT)
}
