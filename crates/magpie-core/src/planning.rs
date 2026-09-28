//! Planning a day within a time budget, and explaining every trade-off.
//!
//! Pure: items, the day, the budget and past performance go in; a plan comes
//! out. Nothing is changed until the caller applies [`Plan::changes`]. The AI
//! (or the human) may edit the proposal first; this is the baseline it starts
//! from and the rules it has to argue against.

use std::collections::HashMap;

use chrono::{NaiveDate, TimeDelta};
use serde::{Deserialize, Serialize};

use crate::item::{Item, ItemChange, ItemId, ItemKind, ItemStatus};

/// Past estimates are corrected only with at least this many data points.
pub const MIN_SAMPLES: usize = 3;
/// Items deferred this many times get a nudge to split or drop them.
pub const NUDGE_AFTER_DEFERRALS: u32 = 3;
/// Items deferred this many times jump ahead so they can't starve forever.
pub const BOOST_AFTER_DEFERRALS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannerSettings {
    pub capacity_min: u32,
    pub default_estimate_min: u32,
}

/// How much longer things really take than estimated: `actual / estimate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub overall: f64,
    pub by_area: HashMap<String, f64>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            overall: 1.0,
            by_area: HashMap::new(),
        }
    }
}

fn median_ratio(ratios: &mut [f64]) -> Option<f64> {
    if ratios.len() < MIN_SAMPLES {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    let mid = ratios.len() / 2;
    let m = if ratios.len() % 2 == 0 {
        f64::midpoint(ratios[mid - 1], ratios[mid])
    } else {
        ratios[mid]
    };
    // A median outside this range is more likely bad data than a real habit.
    Some(m.clamp(0.5, 3.0))
}

impl Calibration {
    /// Median `actual/estimate` over done items that have both. Median, not
    /// mean: one task that ran 5x over shouldn't inflate every estimate.
    pub fn from_history(done: &[Item]) -> Self {
        let mut all = Vec::new();
        let mut per_area: HashMap<String, Vec<f64>> = HashMap::new();
        for i in done.iter().filter(|i| i.status == ItemStatus::Done) {
            if let (Some(est), Some(act)) = (i.estimate_min, i.actual_min) {
                let r = f64::from(act) / f64::from(est);
                all.push(r);
                per_area.entry(i.area.clone()).or_default().push(r);
            }
        }
        Self {
            overall: median_ratio(&mut all).unwrap_or(1.0),
            by_area: per_area
                .into_iter()
                .filter_map(|(area, mut rs)| median_ratio(&mut rs).map(|m| (area, m)))
                .collect(),
        }
    }

    pub fn factor(&self, area: &str) -> f64 {
        self.by_area.get(area).copied().unwrap_or(self.overall)
    }

    /// Calibrated minutes for an item, at least 1.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded: <= 480 * 3
    pub fn minutes(&self, item: &Item, default_estimate: u32) -> u32 {
        let raw = f64::from(item.estimate_min.unwrap_or(default_estimate));
        ((raw * self.factor(&item.area)).round() as u32).max(1)
    }
}

/// Why an item is in the plan. Ordered: earlier variants win ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    FixedTime,
    Deadline,
    Blocking,
    LongDeferred,
    Overdue,
    Scheduled,
    SpareTime,
}

impl Reason {
    fn must(self) -> bool {
        matches!(self, Self::FixedTime | Self::Deadline)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FixedTime => "固定时间",
            Self::Deadline => "到截止日了",
            Self::Blocking => "卡着别的事",
            Self::LongDeferred => "已经推迟好几次",
            Self::Overdue => "逾期没做",
            Self::Scheduled => "原定这天",
            Self::SpareTime => "有空，顺手做",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planned {
    pub item: ItemId,
    pub title: String,
    pub minutes: u32,
    pub reason: Reason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pushed {
    pub item: ItemId,
    pub title: String,
    pub to: NaiveDate,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nudge {
    pub item: ItemId,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub day: NaiveDate,
    pub capacity_min: u32,
    pub used_min: u32,
    pub planned: Vec<Planned>,
    pub pushed: Vec<Pushed>,
    pub nudges: Vec<Nudge>,
    /// Fixed-time and deadline items alone exceed the budget.
    pub over_capacity: bool,
}

struct Candidate<'a> {
    item: &'a Item,
    reason: Reason,
    minutes: u32,
}

fn reason_for(item: &Item, day: NaiveDate) -> Option<Reason> {
    if item.status != ItemStatus::Open || item.kind != ItemKind::Todo {
        return None;
    }
    match item.day {
        Some(d) if d > day => None,
        Some(d) if d == day && item.at.is_some() => Some(Reason::FixedTime),
        _ if item.deadline.is_some_and(|dl| dl <= day) => Some(Reason::Deadline),
        _ if item.blocking => Some(Reason::Blocking),
        Some(_) if item.deferrals >= BOOST_AFTER_DEFERRALS => Some(Reason::LongDeferred),
        Some(d) if d < day => Some(Reason::Overdue),
        Some(_) => Some(Reason::Scheduled),
        None => Some(Reason::SpareTime),
    }
}

/// Proposes `day`'s plan from open todos. Ideas are never auto-planned: the
/// pool is reviewed by a human.
pub fn plan_day(
    items: &[Item],
    day: NaiveDate,
    settings: PlannerSettings,
    calibration: &Calibration,
) -> Plan {
    let mut candidates: Vec<Candidate<'_>> = items
        .iter()
        .filter_map(|item| {
            reason_for(item, day).map(|reason| Candidate {
                item,
                reason,
                minutes: calibration.minutes(item, settings.default_estimate_min),
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.reason
            .cmp(&b.reason)
            .then(a.item.at.cmp(&b.item.at))
            .then(a.item.deadline.cmp(&b.item.deadline))
            .then(b.item.deferrals.cmp(&a.item.deferrals))
            .then(a.minutes.cmp(&b.minutes))
            .then(a.item.created_at.cmp(&b.item.created_at))
    });

    let capacity = settings.capacity_min;
    let mut used = 0_u32;
    let mut planned = Vec::new();
    let mut pushed = Vec::new();
    let next = day + TimeDelta::days(1);
    for c in candidates {
        let fits = used + c.minutes <= capacity;
        if c.reason.must() || fits {
            used += c.minutes;
            planned.push(Planned {
                item: c.item.id,
                title: c.item.title.clone(),
                minutes: c.minutes,
                reason: c.reason,
            });
        } else if c.reason != Reason::SpareTime {
            // Unscheduled todos that don't fit simply stay unscheduled.
            pushed.push(Pushed {
                item: c.item.id,
                title: c.item.title.clone(),
                to: next,
                why: format!(
                    "放不下：这天只剩 {} 分钟，它要 {} 分钟，也不比已排的急",
                    capacity.saturating_sub(used),
                    c.minutes
                ),
            });
        }
    }

    let nudges = items
        .iter()
        .filter(|i| i.status == ItemStatus::Open && i.deferrals >= NUDGE_AFTER_DEFERRALS)
        .map(|i| Nudge {
            item: i.id,
            title: i.title.clone(),
            text: format!(
                "已经推迟 {} 次了：拆成更小的一步，或者干脆放弃？",
                i.deferrals
            ),
        })
        .collect();

    let must_minutes: u32 = planned
        .iter()
        .filter(|p| p.reason.must())
        .map(|p| p.minutes)
        .sum();
    Plan {
        day,
        capacity_min: capacity,
        used_min: used,
        planned,
        pushed,
        nudges,
        over_capacity: must_minutes > capacity,
    }
}

impl Plan {
    /// The item changes that turn this proposal into reality.
    pub fn changes(&self, items: &[Item]) -> Vec<(ItemId, ItemChange)> {
        let by_id: HashMap<ItemId, &Item> = items.iter().map(|i| (i.id, i)).collect();
        let mut out = Vec::new();
        for p in &self.planned {
            if let Some(i) = by_id.get(&p.item) {
                if i.day != Some(self.day) {
                    out.push((
                        p.item,
                        ItemChange::Schedule {
                            day: self.day,
                            at: i.at,
                        },
                    ));
                }
            }
        }
        for p in &self.pushed {
            out.push((p.item, ItemChange::Defer { to: p.to }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemDraft;
    use chrono::{DateTime, NaiveTime, TimeZone, Utc};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }
    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }
    const S: PlannerSettings = PlannerSettings {
        capacity_min: 120,
        default_estimate_min: 30,
    };

    fn todo(title: &str, day: Option<NaiveDate>, est: u32) -> Item {
        ItemDraft {
            title: title.into(),
            day,
            estimate_min: Some(est),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap()
    }

    fn titles(ps: &[Planned]) -> Vec<&str> {
        ps.iter().map(|p| p.title.as_str()).collect()
    }

    #[test]
    fn fills_by_priority_and_explains_what_was_pushed() {
        let mut fixed = todo("20:00 发帖", Some(d(29)), 10);
        fixed.at = NaiveTime::from_hms_opt(20, 0, 0);
        let mut blocker = todo("定价拍板", Some(d(29)), 45);
        blocker.blocking = true;
        let overdue = todo("昨天没做", Some(d(28)), 30);
        let big = todo("大块活", Some(d(29)), 60);
        let small = todo("小事", Some(d(29)), 15);
        let someday = todo("随时", None, 10);
        let items = vec![big.clone(), someday, small, overdue, blocker, fixed];

        let plan = plan_day(&items, d(29), S, &Calibration::default());
        assert_eq!(
            titles(&plan.planned),
            ["20:00 发帖", "定价拍板", "昨天没做", "小事", "随时"],
            "10+45+30+15+10 = 110; the 60-minute item doesn't fit, smaller ones still do"
        );
        assert_eq!(plan.used_min, 110);
        assert_eq!(plan.pushed.len(), 1);
        assert_eq!(plan.pushed[0].item, big.id);
        assert_eq!(plan.pushed[0].to, d(30));
        assert!(
            plan.pushed[0].why.contains("只剩 20 分钟，它要 60 分钟"),
            "{}",
            plan.pushed[0].why
        );
        assert!(!plan.over_capacity);
    }

    #[test]
    fn must_items_stay_even_over_budget() {
        let mut a = todo("会议", Some(d(29)), 100);
        a.at = NaiveTime::from_hms_opt(10, 0, 0);
        let mut b = todo("截止", None, 60);
        b.deadline = Some(d(29));
        let plan = plan_day(&[a, b], d(29), S, &Calibration::default());
        assert_eq!(plan.planned.len(), 2);
        assert!(plan.over_capacity);
        assert_eq!(plan.used_min, 160);
    }

    #[test]
    fn long_deferred_items_jump_ahead_and_get_nudged() {
        let mut stale = todo("一直拖", Some(d(29)), 60);
        stale.deferrals = 3;
        let fresh = todo("新的", Some(d(29)), 90);
        let plan = plan_day(&[fresh, stale.clone()], d(29), S, &Calibration::default());
        assert_eq!(plan.planned[0].title, "一直拖");
        assert_eq!(plan.planned[0].reason, Reason::LongDeferred);
        assert_eq!(plan.nudges.len(), 1);
        assert_eq!(plan.nudges[0].item, stale.id);
    }

    #[test]
    fn ideas_future_and_closed_items_are_left_alone() {
        let idea = ItemDraft {
            kind: ItemKind::Idea,
            title: "点子".into(),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap();
        let future = todo("下周", Some(d(30)), 10);
        let done = ItemChange::Done { actual_min: None }
            .apply(todo("做完了", Some(d(29)), 10), now())
            .unwrap();
        let plan = plan_day(&[idea, future, done], d(29), S, &Calibration::default());
        assert!(plan.planned.is_empty() && plan.pushed.is_empty());
    }

    #[test]
    fn calibration_uses_median_per_area_with_minimum_samples() {
        let done = |area: &str, est: u32, act: u32| {
            let mut i = todo("x", Some(d(20)), est);
            i.area = area.into();
            ItemChange::Done {
                actual_min: Some(act),
            }
            .apply(i, now())
            .unwrap()
        };
        let history = vec![
            done("网站", 30, 45),
            done("网站", 20, 30),
            done("网站", 60, 90),
            done("网站", 10, 100), // outlier: median ignores it
            done("家里", 30, 30),
        ];
        let c = Calibration::from_history(&history);
        assert!((c.factor("网站") - 1.5).abs() < 1e-9, "{c:?}");
        assert!(!c.by_area.contains_key("家里"), "one sample is not enough");
        assert!(
            (c.overall - 1.5).abs() < 1e-9,
            "5 samples overall: median 1.5"
        );

        let mut item = todo("改首页", Some(d(29)), 40);
        item.area = "网站".into();
        assert_eq!(c.minutes(&item, 30), 60);
    }

    #[test]
    fn changes_schedule_pulled_items_and_defer_pushed_ones() {
        let overdue = todo("逾期", Some(d(28)), 30);
        let someday = todo("随时", None, 10);
        let today = todo("原定", Some(d(29)), 10);
        let huge = todo("放不下", Some(d(29)), 200);
        let items = vec![
            overdue.clone(),
            someday.clone(),
            today.clone(),
            huge.clone(),
        ];
        let plan = plan_day(&items, d(29), S, &Calibration::default());
        let changes = plan.changes(&items);
        assert!(changes.contains(&(
            overdue.id,
            ItemChange::Schedule {
                day: d(29),
                at: None
            }
        )));
        assert!(changes.contains(&(
            someday.id,
            ItemChange::Schedule {
                day: d(29),
                at: None
            }
        )));
        assert!(changes.contains(&(huge.id, ItemChange::Defer { to: d(30) })));
        assert!(
            !changes.iter().any(|(id, _)| *id == today.id),
            "already on the day"
        );
        for (id, change) in changes {
            let item = items.iter().find(|i| i.id == id).unwrap().clone();
            assert!(
                change.apply(item, now()).is_ok(),
                "every proposed change must be valid"
            );
        }
    }
}
