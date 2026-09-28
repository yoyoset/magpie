//! Application operations shared by every front door.
//!
//! The HTTP routes and the desktop UI's Tauri commands are thin adapters over
//! these functions, so validation and the "wake the runtime" signal live in
//! exactly one place.

use chrono::{DateTime, NaiveDate, Utc};
use magpie_core::planning::{plan_day, Calibration};
use magpie_core::{
    Ack, AckKind, Delivery, Event, EventDraft, EventId, Item, ItemChange, ItemDraft, ItemError,
    ItemId, ItemKind, ItemStatus, Plan, Reminder, ReminderId, Schedule,
};
use magpie_store::{DayFilter, EventQuery, EventRecord, ItemQuery};
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

// ---------- items (todos and ideas) ----------

impl From<ItemError> for ApiError {
    fn from(e: ItemError) -> Self {
        Self::Invalid(e.to_string())
    }
}

/// Item list filter. At most one of `day` / `before` / `from`+`to` / `unscheduled`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemListQuery {
    pub status: Option<ItemStatus>,
    pub kind: Option<ItemKind>,
    pub day: Option<NaiveDate>,
    pub before: Option<NaiveDate>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    #[serde(default)]
    pub unscheduled: bool,
    pub limit: Option<u32>,
}

impl ItemListQuery {
    fn day_filter(&self) -> Result<DayFilter, ApiError> {
        let forms = [
            self.day.is_some(),
            self.before.is_some(),
            self.from.is_some() || self.to.is_some(),
            self.unscheduled,
        ];
        if forms.iter().filter(|x| **x).count() > 1 {
            return Err(ApiError::Invalid(
                "use only one of day / before / from+to / unscheduled".into(),
            ));
        }
        Ok(match (self.day, self.before, self.from, self.to) {
            (Some(d), ..) => DayFilter::On(d),
            (_, Some(d), ..) => DayFilter::Before(d),
            (_, _, Some(a), Some(b)) if a <= b => DayFilter::Between(a, b),
            (_, _, Some(_), Some(_)) => {
                return Err(ApiError::Invalid("`from` must not be after `to`".into()))
            }
            (_, _, Some(_), None) | (_, _, None, Some(_)) => {
                return Err(ApiError::Invalid("`from` and `to` go together".into()))
            }
            _ if self.unscheduled => DayFilter::Unscheduled,
            _ => DayFilter::Any,
        })
    }
}

pub async fn create_item(state: &AppState, draft: ItemDraft) -> Result<Item, ApiError> {
    let item = draft.validate(ItemId::new(), state.clock.now())?;
    let stored = item.clone();
    state.with_store(move |s| Ok(s.save_item(&stored)?)).await?;
    state.changed.notify_one();
    Ok(item)
}

pub async fn list_items(state: &AppState, q: ItemListQuery) -> Result<Vec<Item>, ApiError> {
    let query = ItemQuery {
        status: q.status,
        kind: q.kind,
        day: q.day_filter()?,
        limit: q.limit.unwrap_or(MAX_PAGE).clamp(1, MAX_PAGE),
    };
    state.with_store(move |s| Ok(s.list_items(&query)?)).await
}

pub async fn get_item(state: &AppState, id: ItemId) -> Result<Item, ApiError> {
    state
        .with_store(move |s| {
            s.get_item(id)?
                .ok_or_else(|| ApiError::NotFound(format!("item {id} not found")))
        })
        .await
}

/// Applies one change atomically with respect to other writers (same lock).
pub async fn change_item(
    state: &AppState,
    id: ItemId,
    change: ItemChange,
) -> Result<Item, ApiError> {
    let now = state.clock.now();
    let item = state
        .with_store(move |s| {
            let current = s
                .get_item(id)?
                .ok_or_else(|| ApiError::NotFound(format!("item {id} not found")))?;
            let next = change.apply(current, now)?;
            s.save_item(&next)?;
            Ok(next)
        })
        .await?;
    state.changed.notify_one();
    Ok(item)
}

// ---------- planning ----------

/// Everything the planner may consider for `day`: open todos on or before it,
/// plus unscheduled ones. And the done history for calibration.
fn planning_inputs(
    s: &magpie_store::Store,
    day: NaiveDate,
) -> Result<(Vec<Item>, Vec<Item>), ApiError> {
    let open_todo = |day: DayFilter| ItemQuery {
        status: Some(ItemStatus::Open),
        kind: Some(ItemKind::Todo),
        day,
        limit: MAX_PAGE,
    };
    let mut items = s.list_items(&open_todo(DayFilter::Before(
        day + chrono::TimeDelta::days(1),
    )))?;
    items.extend(s.list_items(&open_todo(DayFilter::Unscheduled))?);
    let history = s.list_items(&ItemQuery {
        status: Some(ItemStatus::Done),
        limit: MAX_PAGE,
        ..ItemQuery::default()
    })?;
    Ok((items, history))
}

/// A proposal for `day`. Changes nothing.
pub async fn plan(state: &AppState, day: NaiveDate) -> Result<Plan, ApiError> {
    let settings = state.planner;
    state
        .with_store(move |s| {
            let (items, history) = planning_inputs(s, day)?;
            Ok(plan_day(
                &items,
                day,
                settings,
                &Calibration::from_history(&history),
            ))
        })
        .await
}

/// Recomputes the plan and applies it in one go under the store lock, so a
/// stale proposal from a client can never overwrite newer edits.
pub async fn apply_plan(state: &AppState, day: NaiveDate) -> Result<Plan, ApiError> {
    let settings = state.planner;
    let now = state.clock.now();
    let plan = state
        .with_store(move |s| {
            let (items, history) = planning_inputs(s, day)?;
            let plan = plan_day(&items, day, settings, &Calibration::from_history(&history));
            for (id, change) in plan.changes(&items) {
                let current = items
                    .iter()
                    .find(|i| i.id == id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal(format!("planned item {id} vanished")))?;
                s.save_item(&change.apply(current, now)?)?;
            }
            Ok(plan)
        })
        .await?;
    state.changed.notify_one();
    Ok(plan)
}
