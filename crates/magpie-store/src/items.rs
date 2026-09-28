//! `items` table: todos and ideas.

use chrono::{NaiveDate, NaiveTime};
use magpie_core::{Item, ItemId, ItemKind, ItemStatus};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Row};

use crate::codec::{enum_from_name, enum_name, from_ms, ms, opt_from_ms, uuid};
use crate::{Result, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DayFilter {
    #[default]
    Any,
    On(NaiveDate),
    /// Strictly before: with `status = Open` this is "overdue".
    Before(NaiveDate),
    /// Inclusive range.
    Between(NaiveDate, NaiveDate),
    /// No day yet: the idea pool and unscheduled todos.
    Unscheduled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemQuery {
    pub status: Option<ItemStatus>,
    pub kind: Option<ItemKind>,
    pub day: DayFilter,
    pub limit: u32,
}

impl Default for ItemQuery {
    fn default() -> Self {
        Self {
            status: None,
            kind: None,
            day: DayFilter::Any,
            limit: 500,
        }
    }
}

const COLS: &str = "id, kind, status, title, why, action, source, area, day, at, deadline, \
                    estimate_min, actual_min, blocking, deferrals, created_at, updated_at, done_at";

fn date_text(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn parse_date(what: &'static str, s: Option<String>) -> Result<Option<NaiveDate>> {
    s.map(|s| {
        NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| StoreError::Corrupt {
            what,
            detail: e.to_string(),
        })
    })
    .transpose()
}

fn parse_time(s: Option<String>) -> Result<Option<NaiveTime>> {
    s.map(|s| {
        NaiveTime::parse_from_str(&s, "%H:%M:%S").map_err(|e| StoreError::Corrupt {
            what: "item time",
            detail: e.to_string(),
        })
    })
    .transpose()
}

fn decode(row: &Row<'_>) -> rusqlite::Result<Result<Item>> {
    let id: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let status: String = row.get(2)?;
    let day: Option<String> = row.get(8)?;
    let at: Option<String> = row.get(9)?;
    let deadline: Option<String> = row.get(10)?;
    let created: i64 = row.get(15)?;
    let updated: i64 = row.get(16)?;
    let done: Option<i64> = row.get(17)?;
    let (title, why, action, source, area) = (
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    );
    let (estimate_min, actual_min, blocking, deferrals) =
        (row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?);
    Ok((|| {
        Ok(Item {
            id: ItemId(uuid("item id", &id)?),
            kind: enum_from_name("item kind", &kind)?,
            status: enum_from_name("item status", &status)?,
            title,
            why,
            action,
            source,
            area,
            day: parse_date("item day", day)?,
            at: parse_time(at)?,
            deadline: parse_date("item deadline", deadline)?,
            estimate_min,
            actual_min,
            blocking,
            deferrals,
            created_at: from_ms(created)?,
            updated_at: from_ms(updated)?,
            done_at: opt_from_ms(done)?,
        })
    })())
}

impl Store {
    /// Insert or replace the whole item by id.
    pub fn save_item(&self, i: &Item) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO items ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                 ON CONFLICT (id) DO UPDATE SET
                   kind=excluded.kind, status=excluded.status, title=excluded.title, why=excluded.why,
                   action=excluded.action, source=excluded.source, area=excluded.area, day=excluded.day,
                   at=excluded.at, deadline=excluded.deadline, estimate_min=excluded.estimate_min,
                   actual_min=excluded.actual_min, blocking=excluded.blocking, deferrals=excluded.deferrals,
                   updated_at=excluded.updated_at, done_at=excluded.done_at"
            ),
            params![
                i.id.to_string(),
                enum_name(&i.kind),
                enum_name(&i.status),
                i.title,
                i.why,
                i.action,
                i.source,
                i.area,
                i.day.map(date_text),
                i.at.map(|t| t.format("%H:%M:%S").to_string()),
                i.deadline.map(date_text),
                i.estimate_min,
                i.actual_min,
                i.blocking,
                i.deferrals,
                ms(i.created_at),
                ms(i.updated_at),
                i.done_at.map(ms),
            ],
        )?;
        Ok(())
    }

    pub fn get_item(&self, id: ItemId) -> Result<Option<Item>> {
        self.conn
            .query_row(
                &format!("SELECT {COLS} FROM items WHERE id = ?1"),
                [id.to_string()],
                decode,
            )
            .optional()?
            .transpose()
    }

    /// Ordered by day (unscheduled last), then time (untimed last), then creation.
    pub fn list_items(&self, q: &ItemQuery) -> Result<Vec<Item>> {
        let mut clauses: Vec<&str> = Vec::new();
        let mut args: Vec<SqlValue> = Vec::new();
        if let Some(s) = q.status {
            clauses.push("status = ?");
            args.push(SqlValue::Text(enum_name(&s)));
        }
        if let Some(k) = q.kind {
            clauses.push("kind = ?");
            args.push(SqlValue::Text(enum_name(&k)));
        }
        match q.day {
            DayFilter::Any => {}
            DayFilter::On(d) => {
                clauses.push("day = ?");
                args.push(SqlValue::Text(date_text(d)));
            }
            DayFilter::Before(d) => {
                clauses.push("day < ?");
                args.push(SqlValue::Text(date_text(d)));
            }
            DayFilter::Between(a, b) => {
                clauses.push("day BETWEEN ? AND ?");
                args.push(SqlValue::Text(date_text(a)));
                args.push(SqlValue::Text(date_text(b)));
            }
            DayFilter::Unscheduled => clauses.push("day IS NULL"),
        }
        let where_ = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        args.push(SqlValue::Integer(i64::from(q.limit)));
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM items {where_}
             ORDER BY day IS NULL, day, at IS NULL, at, created_at
             LIMIT ?"
        ))?;
        let rows = stmt.query_map(params_from_iter(args), decode)?;
        rows.map(|r| r?).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use magpie_core::{ItemChange, ItemDraft};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    fn item(title: &str, kind: ItemKind, day: Option<NaiveDate>, at: Option<(u32, u32)>) -> Item {
        ItemDraft {
            kind,
            title: title.into(),
            why: "为什么".into(),
            day,
            at: at.and_then(|(h, m)| NaiveTime::from_hms_opt(h, m, 0)),
            estimate_min: Some(30),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap()
    }

    #[test]
    fn roundtrip_and_upsert() {
        let s = Store::open_in_memory().unwrap();
        let i = item("发小红书", ItemKind::Todo, Some(d(28)), Some((20, 0)));
        s.save_item(&i).unwrap();
        assert_eq!(s.get_item(i.id).unwrap().unwrap(), i);

        let done = ItemChange::Done {
            actual_min: Some(12),
        }
        .apply(i.clone(), now())
        .unwrap();
        s.save_item(&done).unwrap();
        assert_eq!(s.get_item(i.id).unwrap().unwrap(), done);
        assert_eq!(
            s.list_items(&ItemQuery::default()).unwrap().len(),
            1,
            "upsert, not duplicate"
        );
    }

    #[test]
    fn filters_and_ordering() {
        let s = Store::open_in_memory().unwrap();
        let late = item("晚上的事", ItemKind::Todo, Some(d(28)), Some((21, 0)));
        let early = item("早上的事", ItemKind::Todo, Some(d(28)), Some((9, 0)));
        let untimed = item("今天随时", ItemKind::Todo, Some(d(28)), None);
        let yesterday = item("昨天没做", ItemKind::Todo, Some(d(27)), None);
        let tomorrow = item("明天", ItemKind::Todo, Some(d(29)), None);
        let idea = item("做数字产品", ItemKind::Idea, None, None);
        for i in [&late, &early, &untimed, &yesterday, &tomorrow, &idea] {
            s.save_item(i).unwrap();
        }
        let titles = |q: ItemQuery| -> Vec<String> {
            s.list_items(&q)
                .unwrap()
                .into_iter()
                .map(|i| i.title)
                .collect()
        };
        assert_eq!(
            titles(ItemQuery {
                day: DayFilter::On(d(28)),
                ..ItemQuery::default()
            }),
            ["早上的事", "晚上的事", "今天随时"]
        );
        assert_eq!(
            titles(ItemQuery {
                status: Some(ItemStatus::Open),
                day: DayFilter::Before(d(28)),
                ..ItemQuery::default()
            }),
            ["昨天没做"]
        );
        assert_eq!(
            titles(ItemQuery {
                kind: Some(ItemKind::Idea),
                ..ItemQuery::default()
            }),
            ["做数字产品"]
        );
        assert_eq!(
            titles(ItemQuery {
                day: DayFilter::Between(d(28), d(29)),
                ..ItemQuery::default()
            })
            .len(),
            4
        );
        assert_eq!(
            titles(ItemQuery {
                day: DayFilter::Unscheduled,
                ..ItemQuery::default()
            }),
            ["做数字产品"]
        );
        assert_eq!(
            titles(ItemQuery::default()).last().map(String::as_str),
            Some("做数字产品"),
            "undated last"
        );
    }

    #[test]
    fn fresh_database_is_at_v2() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.schema_version().unwrap(), 2);
    }
}
