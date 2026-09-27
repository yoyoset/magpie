//! Application operations shared by every front door.
//!
//! The HTTP routes and the desktop UI's Tauri commands are thin adapters over
//! these functions, so validation and the "wake the runtime" signal live in
//! exactly one place.

use chrono::{DateTime, Utc};
use magpie_core::{
    Ack, AckKind, Delivery, Event, EventDraft, EventId, Reminder, ReminderId, Schedule,
};
use magpie_store::{EventQuery, EventRecord};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

pub const MAX_PAGE: u32 = 500;
pub const DEFAULT_PAGE: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventView {
    #[serde(flatten)]
    pub event: Event,
    pub closed_at: Option<DateTime<Utc>>,
}

impl From<EventRecord> for EventView {
    fn from(r: EventRecord) -> Self {
        Self {
            event: r.event,
            closed_at: r.closed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDetail {
    #[serde(flatten)]
    pub view: EventView,
    pub latest_ack: Option<Ack>,
    pub deliveries: Vec<Delivery>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub open: bool,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewReminder {
    pub schedule: Schedule,
    pub template: EventDraft,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

pub async fn create_event(state: &AppState, draft: EventDraft) -> Result<Event, ApiError> {
    let event = draft.validate(EventId::new(), state.clock.now())?;
    let stored = event.clone();
    state
        .with_store(move |s| Ok(s.insert_event(&stored)?))
        .await?;
    state.changed.notify_one();
    Ok(event)
}

pub async fn list_events(state: &AppState, q: ListQuery) -> Result<Vec<EventView>, ApiError> {
    let query = EventQuery {
        open_only: q.open,
        before: q.before,
        limit: q.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE),
    };
    let rows = state
        .with_store(move |s| Ok(s.list_events(&query)?))
        .await?;
    Ok(rows.into_iter().map(EventView::from).collect())
}

pub async fn event_detail(state: &AppState, id: EventId) -> Result<EventDetail, ApiError> {
    state
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
        .await
}

pub async fn ack_event(
    state: &AppState,
    id: EventId,
    kind: AckKind,
    via: &str,
) -> Result<(), ApiError> {
    let now = state.clock.now();
    if let AckKind::Snoozed { until } = kind {
        if until <= now {
            return Err(ApiError::Invalid(
                "snooze `until` must be in the future".into(),
            ));
        }
    }
    let via = via.trim();
    if via.is_empty() {
        return Err(ApiError::Invalid("via must not be empty".into()));
    }
    let ack = Ack {
        event_id: id,
        kind,
        via: via.to_owned(),
        at: now,
    };
    state.with_store(move |s| Ok(s.record_ack(&ack)?)).await?;
    state.changed.notify_one();
    Ok(())
}

pub async fn create_reminder(state: &AppState, new: NewReminder) -> Result<Reminder, ApiError> {
    let now = state.clock.now();
    // Fail now, not at 3 a.m. when the reminder fires with an empty title.
    new.template.clone().validate(EventId::new(), now)?;
    let reminder = Reminder {
        id: ReminderId::new(),
        schedule: new.schedule,
        template: new.template,
        enabled: new.enabled,
        created_at: now,
        last_fired: None,
    };
    let stored = reminder.clone();
    state
        .with_store(move |s| Ok(s.save_reminder(&stored)?))
        .await?;
    state.changed.notify_one();
    Ok(reminder)
}

pub async fn list_reminders(state: &AppState) -> Result<Vec<Reminder>, ApiError> {
    state.with_store(|s| Ok(s.reminders()?)).await
}

pub async fn delete_reminder(state: &AppState, id: ReminderId) -> Result<(), ApiError> {
    let deleted = state
        .with_store(move |s| Ok(s.delete_reminder(id)?))
        .await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("reminder {id} not found")));
    }
    state.changed.notify_one();
    Ok(())
}
