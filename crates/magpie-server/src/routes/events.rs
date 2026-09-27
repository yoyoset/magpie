use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use magpie_core::{Ack, AckKind, Delivery, Event, EventDraft, EventId};
use magpie_store::{EventQuery, EventRecord};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, AppState};

const MAX_PAGE: u32 = 500;

#[derive(Debug, Serialize)]
pub struct EventView {
    #[serde(flatten)]
    event: Event,
    closed_at: Option<DateTime<Utc>>,
}

impl From<EventRecord> for EventView {
    fn from(r: EventRecord) -> Self {
        Self {
            event: r.event,
            closed_at: r.closed_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventDetail {
    #[serde(flatten)]
    view: EventView,
    latest_ack: Option<Ack>,
    deliveries: Vec<Delivery>,
}

pub async fn create(
    State(state): State<AppState>,
    Json(draft): Json<EventDraft>,
) -> Result<(StatusCode, Json<Event>), ApiError> {
    let event = draft.validate(EventId::new(), state.clock.now())?;
    let stored = event.clone();
    state
        .with_store(move |s| Ok(s.insert_event(&stored)?))
        .await?;
    state.changed.notify_one();
    Ok((StatusCode::CREATED, Json(event)))
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    open: bool,
    before: Option<DateTime<Utc>>,
    limit: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<Vec<EventView>>, ApiError> {
    let q = EventQuery {
        open_only: p.open,
        before: p.before,
        limit: p.limit.unwrap_or(100).clamp(1, MAX_PAGE),
    };
    let rows = state.with_store(move |s| Ok(s.list_events(&q)?)).await?;
    Ok(Json(rows.into_iter().map(EventView::from).collect()))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EventDetail>, ApiError> {
    let id = EventId(id);
    let detail = state
        .with_store(move |s| {
            let record = s
                .get_event(id)?
                .ok_or_else(|| ApiError::NotFound(format!("event {id} not found")))?;
            Ok(EventDetail {
                view: record.into(),
                latest_ack: s.latest_ack(id)?,
                deliveries: s.deliveries(id)?,
            })
        })
        .await?;
    Ok(Json(detail))
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
    let now = state.clock.now();
    if let AckKind::Snoozed { until } = body.kind {
        if until <= now {
            return Err(ApiError::Invalid(
                "snooze `until` must be in the future".into(),
            ));
        }
    }
    let via = body.via.trim();
    if via.is_empty() {
        return Err(ApiError::Invalid("via must not be empty".into()));
    }
    let ack = Ack {
        event_id: EventId(id),
        kind: body.kind,
        via: via.to_owned(),
        at: now,
    };
    state.with_store(move |s| Ok(s.record_ack(&ack)?)).await?;
    state.changed.notify_one();
    Ok(StatusCode::NO_CONTENT)
}
