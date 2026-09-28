//! Follow-ups: the assistant nudging about planned items without being asked.
//!
//! Pure: today's items, the local time and what was already sent go in; the
//! events to send (each with a dedup key) and the next moment to look again
//! come out. The runtime persists the keys so nothing is sent twice.

use std::collections::HashSet;
use std::hash::BuildHasher;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};

use crate::event::{EventDraft, Severity};
use crate::item::{Item, ItemKind, ItemStatus};

pub const SOURCE: &str = "magpie:planner";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowUpSettings {
    pub briefing_at: NaiveTime,
    /// Remind this many minutes before a timed item.
    pub lead_min: u32,
    /// Ask "done?" this long after a timed item's start, at least; longer if
    /// its estimate is longer.
    pub check_after_min: u32,
}

impl Default for FollowUpSettings {
    fn default() -> Self {
        Self {
            briefing_at: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
            lead_min: 5,
            check_after_min: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    /// Dedup key, e.g. `due:<item>:<day>`. Persist it once the event is stored.
    pub key: String,
    pub draft: EventDraft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUps {
    pub send: Vec<FollowUp>,
    /// Next local moment something becomes due, if any today.
    pub next: Option<NaiveDateTime>,
}

fn hm(t: NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

fn tags(item: Option<&Item>) -> Vec<String> {
    let mut t = vec!["planner".to_owned()];
    if let Some(i) = item {
        t.push(format!("item:{}", i.id));
    }
    t
}

fn briefing(today: NaiveDate, todays: &[&Item], overdue: usize) -> EventDraft {
    let open: Vec<&&Item> = todays
        .iter()
        .filter(|i| i.status == ItemStatus::Open)
        .collect();
    let minutes: u32 = open.iter().filter_map(|i| i.estimate_min).sum();
    let mut lines: Vec<String> = open
        .iter()
        .map(|i| {
            let at = i.at.map(|t| format!("{} ", hm(t))).unwrap_or_default();
            let est = i
                .estimate_min
                .map(|m| format!("（{m} 分钟）"))
                .unwrap_or_default();
            format!("• {at}{}{est}", i.title)
        })
        .collect();
    if overdue > 0 {
        lines.push(format!("另有 {overdue} 件逾期没做，晚上收尾时一起处理。"));
    }
    let title = if open.is_empty() {
        format!("{} 今天没有排事", today.format("%m-%d"))
    } else {
        format!("今天 {} 件事，约 {} 分钟", open.len(), minutes)
    };
    EventDraft {
        source: SOURCE.into(),
        title,
        body: lines.join("\n"),
        severity: Severity::Normal,
        tags: tags(None),
    }
}

/// What to send at `now` (local). `items` should hold today's todos; `overdue`
/// is the count of open todos from earlier days.
pub fn follow_ups<H: BuildHasher>(
    items: &[Item],
    overdue: usize,
    now: NaiveDateTime,
    sent: &HashSet<String, H>,
    s: FollowUpSettings,
) -> FollowUps {
    let today = now.date();
    let mut due_now = Vec::new();
    let mut next: Option<NaiveDateTime> = None;
    let mut consider = |at: NaiveDateTime, key: String, make: &dyn Fn() -> EventDraft| {
        if sent.contains(&key) {
            return;
        }
        if at <= now {
            due_now.push(FollowUp { key, draft: make() });
        } else {
            next = Some(next.map_or(at, |n| n.min(at)));
        }
    };

    let todays: Vec<&Item> = items
        .iter()
        .filter(|i| {
            i.kind == ItemKind::Todo && i.day == Some(today) && i.status != ItemStatus::Dropped
        })
        .collect();

    consider(
        today.and_time(s.briefing_at),
        format!("brief:{today}"),
        &|| briefing(today, &todays, overdue),
    );

    for item in todays.iter().filter(|i| i.status == ItemStatus::Open) {
        let Some(at) = item.at else { continue };
        let start = today.and_time(at);
        consider(
            start - TimeDelta::minutes(i64::from(s.lead_min)),
            format!("due:{}:{today}", item.id),
            &|| EventDraft {
                source: SOURCE.into(),
                title: format!("{} {}", hm(at), item.title),
                body: [item.action.as_str(), item.why.as_str()]
                    .iter()
                    .filter(|t| !t.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n"),
                severity: Severity::Urgent,
                tags: tags(Some(item)),
            },
        );
        let wait = item.estimate_min.unwrap_or(0).max(s.check_after_min);
        consider(
            start + TimeDelta::minutes(i64::from(wait)),
            format!("check:{}:{today}", item.id),
            &|| {
                EventDraft {
                source: SOURCE.into(),
                title: format!("做完了吗：{}", item.title),
                body: "做完了就在「今天」页勾掉（顺手记下用了多久）；没做就点「明天」挪走，晚上收尾时再排。".into(),
                severity: Severity::Normal,
                tags: tags(Some(item)),
            }
            },
        );
    }
    FollowUps {
        send: due_now,
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemChange, ItemDraft, ItemId};
    use chrono::{DateTime, TimeZone, Utc};

    fn now_utc() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap()
    }
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }
    fn at(h: u32, m: u32) -> NaiveDateTime {
        day().and_hms_opt(h, m, 0).unwrap()
    }
    fn item(title: &str, time: Option<(u32, u32)>, est: Option<u32>) -> Item {
        ItemDraft {
            title: title.into(),
            action: "她先看口吻再发".into(),
            day: Some(day()),
            at: time.and_then(|(h, m)| NaiveTime::from_hms_opt(h, m, 0)),
            estimate_min: est,
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now_utc())
        .unwrap()
    }
    fn keys(f: &FollowUps) -> Vec<&str> {
        f.send
            .iter()
            .map(|x| x.key.split(':').next().unwrap())
            .collect()
    }

    #[test]
    fn briefing_once_after_its_time_with_todays_list() {
        let items = vec![
            item("发小红书", Some((20, 0)), Some(10)),
            item("买纸", None, Some(5)),
        ];
        let early = follow_ups(
            &items,
            2,
            at(8, 0),
            &HashSet::new(),
            FollowUpSettings::default(),
        );
        assert!(early.send.is_empty());
        assert_eq!(early.next, Some(at(9, 0)));

        let f = follow_ups(
            &items,
            2,
            at(9, 30),
            &HashSet::new(),
            FollowUpSettings::default(),
        );
        assert_eq!(keys(&f), ["brief"]);
        let b = &f.send[0].draft;
        assert_eq!(b.title, "今天 2 件事，约 15 分钟");
        assert!(b.body.contains("• 20:00 发小红书（10 分钟）"), "{}", b.body);
        assert!(b.body.contains("2 件逾期"), "{}", b.body);
        assert_eq!(f.next, Some(at(19, 55)), "next: 5 minutes before 20:00");

        let sent: HashSet<String> = f.send.iter().map(|x| x.key.clone()).collect();
        let again = follow_ups(&items, 2, at(10, 0), &sent, FollowUpSettings::default());
        assert!(again.send.is_empty(), "never twice");
    }

    #[test]
    fn timed_item_reminds_before_and_asks_after() {
        let post = item("发小红书", Some((20, 0)), Some(10));
        let brief = HashSet::from([format!("brief:{}", day())]);

        let f = follow_ups(
            std::slice::from_ref(&post),
            0,
            at(19, 56),
            &brief,
            FollowUpSettings::default(),
        );
        assert_eq!(keys(&f), ["due"]);
        assert_eq!(f.send[0].draft.severity, Severity::Urgent);
        assert_eq!(f.send[0].draft.title, "20:00 发小红书");
        assert!(f.send[0].draft.tags.contains(&format!("item:{}", post.id)));
        assert_eq!(f.next, Some(at(20, 30)), "check after max(10, 30) min");

        let mut sent = brief.clone();
        sent.insert(f.send[0].key.clone());
        let later = follow_ups(
            std::slice::from_ref(&post),
            0,
            at(20, 45),
            &sent,
            FollowUpSettings::default(),
        );
        assert_eq!(keys(&later), ["check"]);
        assert!(later.send[0].draft.title.starts_with("做完了吗"));

        let done = ItemChange::Done {
            actual_min: Some(8),
        }
        .apply(post, now_utc())
        .unwrap();
        let none = follow_ups(&[done], 0, at(20, 45), &sent, FollowUpSettings::default());
        assert!(none.send.is_empty(), "done items are left alone");
    }

    #[test]
    fn long_estimates_wait_longer_before_asking() {
        let long = item("剪视频", Some((14, 0)), Some(120));
        let sent = HashSet::from([
            format!("brief:{}", day()),
            format!("due:{}:{}", long.id, day()),
        ]);
        let f = follow_ups(&[long], 0, at(15, 0), &sent, FollowUpSettings::default());
        assert!(f.send.is_empty());
        assert_eq!(f.next, Some(at(16, 0)));
    }

    #[test]
    fn empty_day_still_gets_a_briefing() {
        let f = follow_ups(
            &[],
            0,
            at(9, 1),
            &HashSet::new(),
            FollowUpSettings::default(),
        );
        assert_eq!(f.send[0].draft.title, "09-28 今天没有排事");
    }
}
