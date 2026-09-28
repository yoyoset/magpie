//! Items: things a human means to do (todos) or might do (ideas).
//!
//! Distinct from events: an event is "something happened, look at it"; an item
//! is "something you (will) do", with a day, an estimate and a done state.
//! Days are local calendar dates, because "today" and "tomorrow" are local notions.

use std::fmt;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::ValidationError;

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_TEXT_CHARS: usize = 4_000;
pub const MAX_SHORT_CHARS: usize = 60;
/// A single item over a working day is a project, not a todo: split it.
pub const MAX_ESTIMATE_MIN: u32 = 8 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub Uuid);

impl ItemId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    /// In the idea pool: not committed to, not scheduled.
    Idea,
    /// Committed to; may or may not have a day yet.
    Todo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Open,
    Done,
    /// Decided not to do. Kept for the record, never resurfaced.
    Dropped,
}

/// Untrusted input from the API, CLI or an AI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDraft {
    #[serde(default = "default_kind")]
    pub kind: ItemKind,
    /// Starts with a verb: "定价系数拍板", not "定价".
    pub title: String,
    /// Why it matters, in a sentence. The part that gets forgotten.
    #[serde(default)]
    pub why: String,
    /// What the human concretely has to do (vs. what an AI or script does).
    #[serde(default)]
    pub action: String,
    /// Where it came from: a project, a conversation, a scheduled task.
    #[serde(default)]
    pub source: String,
    /// Coarse life area, e.g. 工作 / 家里 / 孩子.
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub day: Option<NaiveDate>,
    #[serde(default)]
    pub at: Option<NaiveTime>,
    #[serde(default)]
    pub deadline: Option<NaiveDate>,
    #[serde(default)]
    pub estimate_min: Option<u32>,
    /// Other work waits on this one; the planner puts it first.
    #[serde(default)]
    pub blocking: bool,
}

fn default_kind() -> ItemKind {
    ItemKind::Todo
}

impl Default for ItemKind {
    fn default() -> Self {
        default_kind()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: ItemKind,
    pub status: ItemStatus,
    pub title: String,
    pub why: String,
    pub action: String,
    pub source: String,
    pub area: String,
    pub day: Option<NaiveDate>,
    pub at: Option<NaiveTime>,
    pub deadline: Option<NaiveDate>,
    pub estimate_min: Option<u32>,
    pub actual_min: Option<u32>,
    pub blocking: bool,
    /// How many times it was pushed to a later day. Visible nagging signal.
    pub deferrals: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
}

fn text(field: &'static str, value: &str, limit: usize) -> Result<String, ValidationError> {
    let v = value.trim();
    let actual = v.chars().count();
    if actual > limit {
        return Err(ValidationError::TooLong {
            field,
            actual,
            limit,
        });
    }
    Ok(v.to_owned())
}

fn estimate(minutes: Option<u32>) -> Result<Option<u32>, ItemError> {
    match minutes {
        Some(0) => Err(ItemError::Invalid(
            "estimate must be at least 1 minute".into(),
        )),
        Some(m) if m > MAX_ESTIMATE_MIN => Err(ItemError::Invalid(format!(
            "estimate {m} min is over {MAX_ESTIMATE_MIN}; split it into smaller items"
        ))),
        other => Ok(other),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("{0}")]
    Invalid(String),
}

impl ItemDraft {
    pub fn validate(self, id: ItemId, now: DateTime<Utc>) -> Result<Item, ItemError> {
        let title = text("title", &self.title, MAX_TITLE_CHARS)?;
        if title.is_empty() {
            return Err(ValidationError::Empty { field: "title" }.into());
        }
        if self.kind == ItemKind::Idea && (self.day.is_some() || self.at.is_some()) {
            return Err(ItemError::Invalid(
                "an idea has no day; make it a todo to schedule it".into(),
            ));
        }
        if self.at.is_some() && self.day.is_none() {
            return Err(ItemError::Invalid("a time needs a day".into()));
        }
        Ok(Item {
            id,
            kind: self.kind,
            status: ItemStatus::Open,
            title,
            why: text("why", &self.why, MAX_TEXT_CHARS)?,
            action: text("action", &self.action, MAX_TEXT_CHARS)?,
            source: text("source", &self.source, MAX_SHORT_CHARS * 2)?,
            area: text("area", &self.area, MAX_SHORT_CHARS)?,
            day: self.day,
            at: self.at,
            deadline: self.deadline,
            estimate_min: estimate(self.estimate_min)?,
            actual_min: None,
            blocking: self.blocking,
            deferrals: 0,
            created_at: now,
            updated_at: now,
            done_at: None,
        })
    }
}

/// Every way an item can change after creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ItemChange {
    /// Done; `actual_min` feeds the time ledger.
    Done {
        #[serde(default)]
        actual_min: Option<u32>,
    },
    /// Put on a day (and optionally a time). Turns an idea into a todo.
    Schedule {
        day: NaiveDate,
        #[serde(default)]
        at: Option<NaiveTime>,
    },
    /// Move to a later day; counts as a deferral.
    Defer {
        to: NaiveDate,
    },
    /// Back to the pool / no day.
    Unschedule,
    Estimate {
        minutes: u32,
    },
    Drop,
    /// Undo done/drop.
    Reopen,
}

impl ItemChange {
    /// Applies the change, returning the new item. Pure: `now` is injected.
    pub fn apply(self, mut item: Item, now: DateTime<Utc>) -> Result<Item, ItemError> {
        let closed = item.status != ItemStatus::Open;
        let needs_open = !matches!(self, Self::Reopen);
        if closed && needs_open {
            return Err(ItemError::Invalid(format!(
                "item is {}; reopen it first",
                if item.status == ItemStatus::Done {
                    "done"
                } else {
                    "dropped"
                }
            )));
        }
        match self {
            Self::Done { actual_min } => {
                if actual_min == Some(0) {
                    return Err(ItemError::Invalid(
                        "actual time must be at least 1 minute".into(),
                    ));
                }
                item.status = ItemStatus::Done;
                item.actual_min = actual_min;
                item.done_at = Some(now);
            }
            Self::Schedule { day, at } => {
                item.kind = ItemKind::Todo;
                item.day = Some(day);
                item.at = at;
            }
            Self::Defer { to } => {
                if item.day.is_some_and(|d| to <= d) {
                    return Err(ItemError::Invalid("defer must move to a later day".into()));
                }
                item.kind = ItemKind::Todo;
                item.day = Some(to);
                item.at = None;
                item.deferrals += 1;
            }
            Self::Unschedule => {
                item.day = None;
                item.at = None;
            }
            Self::Estimate { minutes } => item.estimate_min = estimate(Some(minutes))?,
            Self::Drop => item.status = ItemStatus::Dropped,
            Self::Reopen => {
                item.status = ItemStatus::Open;
                item.done_at = None;
                item.actual_min = None;
            }
        }
        item.updated_at = now;
        Ok(item)
    }
}

impl Item {
    /// Open, scheduled, and its day is already past.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.status == ItemStatus::Open && self.day.is_some_and(|d| d < today)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    fn todo() -> Item {
        ItemDraft {
            title: " 发小红书第一条 ".into(),
            why: "起号期 30 天要稳定发 ≥12 条".into(),
            action: "20:00 前她看一遍文案口吻，然后点发布".into(),
            source: "viiyd 营销".into(),
            area: "工作".into(),
            day: Some(d(28)),
            at: NaiveTime::from_hms_opt(20, 0, 0),
            estimate_min: Some(10),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap()
    }

    #[test]
    fn draft_trims_and_defaults_to_open_todo() {
        let i = todo();
        assert_eq!(i.title, "发小红书第一条");
        assert_eq!((i.kind, i.status), (ItemKind::Todo, ItemStatus::Open));
        assert_eq!(i.deferrals, 0);
    }

    #[test]
    fn draft_rules() {
        let empty = ItemDraft {
            title: "  ".into(),
            ..ItemDraft::default()
        };
        assert!(empty.validate(ItemId::new(), now()).is_err());
        let idea_with_day = ItemDraft {
            kind: ItemKind::Idea,
            title: "做数字产品".into(),
            day: Some(d(30)),
            ..ItemDraft::default()
        };
        assert!(matches!(
            idea_with_day.validate(ItemId::new(), now()),
            Err(ItemError::Invalid(_))
        ));
        let time_without_day = ItemDraft {
            title: "x".into(),
            at: NaiveTime::from_hms_opt(9, 0, 0),
            ..ItemDraft::default()
        };
        assert!(time_without_day.validate(ItemId::new(), now()).is_err());
        let huge = ItemDraft {
            title: "x".into(),
            estimate_min: Some(9 * 60),
            ..ItemDraft::default()
        };
        assert!(huge.validate(ItemId::new(), now()).is_err());
    }

    #[test]
    fn unknown_draft_fields_are_rejected() {
        let json = r#"{"title":"x","dayy":"2026-09-30"}"#;
        assert!(serde_json::from_str::<ItemDraft>(json).is_err());
    }

    #[test]
    fn done_records_actual_time() {
        let i = ItemChange::Done {
            actual_min: Some(25),
        }
        .apply(todo(), now())
        .unwrap();
        assert_eq!(i.status, ItemStatus::Done);
        assert_eq!(i.actual_min, Some(25));
        assert_eq!(i.done_at, Some(now()));
    }

    #[test]
    fn defer_moves_later_and_counts() {
        let i = ItemChange::Defer { to: d(29) }
            .apply(todo(), now())
            .unwrap();
        assert_eq!((i.day, i.at, i.deferrals), (Some(d(29)), None, 1));
        assert!(
            ItemChange::Defer { to: d(28) }.apply(i, now()).is_err(),
            "same or earlier day is not a deferral"
        );
    }

    #[test]
    fn scheduling_an_idea_makes_it_a_todo() {
        let idea = ItemDraft {
            kind: ItemKind::Idea,
            title: "做数字产品".into(),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap();
        let t = ItemChange::Schedule {
            day: d(30),
            at: None,
        }
        .apply(idea, now())
        .unwrap();
        assert_eq!((t.kind, t.day), (ItemKind::Todo, Some(d(30))));
    }

    #[test]
    fn closed_items_must_be_reopened_first() {
        let done = ItemChange::Done { actual_min: None }
            .apply(todo(), now())
            .unwrap();
        assert!(ItemChange::Defer { to: d(30) }
            .apply(done.clone(), now())
            .is_err());
        let reopened = ItemChange::Reopen.apply(done, now()).unwrap();
        assert_eq!(
            (reopened.status, reopened.done_at),
            (ItemStatus::Open, None)
        );
    }

    #[test]
    fn overdue_only_when_open_and_past() {
        let i = todo();
        assert!(!i.is_overdue(d(28)));
        assert!(i.is_overdue(d(29)));
        let done = ItemChange::Done { actual_min: None }
            .apply(i, now())
            .unwrap();
        assert!(!done.is_overdue(d(29)));
    }

    #[test]
    fn change_json_shape() {
        let c: ItemChange = serde_json::from_str(r#"{"op":"defer","to":"2026-09-30"}"#).unwrap();
        assert_eq!(c, ItemChange::Defer { to: d(30) });
        let c: ItemChange = serde_json::from_str(r#"{"op":"done","actual_min":40}"#).unwrap();
        assert_eq!(
            c,
            ItemChange::Done {
                actual_min: Some(40)
            }
        );
    }
}
