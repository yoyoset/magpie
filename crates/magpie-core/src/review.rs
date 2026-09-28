//! The evening closing review and the weekly review, modelled on the kids'
//! paper card: count what got done (no judgement), a time ledger, carry the
//! rest forward, look at tomorrow. Pure: items in, summaries and texts out.

use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, TimeZone, Utc, Weekday};

use crate::event::{EventDraft, Severity};
use crate::item::{Item, ItemId, ItemKind, ItemStatus};
use crate::planning::Plan;

pub const SOURCE: &str = "magpie:review";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerLine {
    pub title: String,
    pub estimate_min: u32,
    pub actual_min: u32,
}

impl LedgerLine {
    fn overrun(&self) -> i64 {
        i64::from(self.actual_min) - i64::from(self.estimate_min)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayReview {
    pub day: NaiveDate,
    pub planned: usize,
    pub done: usize,
    pub ledger: Vec<LedgerLine>,
    /// Open todos on the day: to carry forward.
    pub leftovers: Vec<ItemId>,
    pub leftover_titles: Vec<String>,
}

/// `items` = the day's todos (any status).
pub fn review_day(items: &[Item], day: NaiveDate) -> DayReview {
    let todays: Vec<&Item> = items
        .iter()
        .filter(|i| {
            i.kind == ItemKind::Todo && i.day == Some(day) && i.status != ItemStatus::Dropped
        })
        .collect();
    let ledger = todays
        .iter()
        .filter(|i| i.status == ItemStatus::Done)
        .filter_map(|i| {
            Some(LedgerLine {
                title: i.title.clone(),
                estimate_min: i.estimate_min?,
                actual_min: i.actual_min?,
            })
        })
        .collect();
    let open: Vec<&&Item> = todays
        .iter()
        .filter(|i| i.status == ItemStatus::Open)
        .collect();
    DayReview {
        day,
        planned: todays.len(),
        done: todays
            .iter()
            .filter(|i| i.status == ItemStatus::Done)
            .count(),
        ledger,
        leftovers: open.iter().map(|i| i.id).collect(),
        leftover_titles: open.iter().map(|i| i.title.clone()).collect(),
    }
}

/// The evening event: count, ledger, what moved, tomorrow's draft, what to do.
pub fn closing_event(r: &DayReview, tomorrow: &Plan) -> EventDraft {
    let mut body = Vec::new();
    if let Some(worst) = r
        .ledger
        .iter()
        .filter(|l| l.overrun() > 0)
        .max_by_key(|l| l.overrun())
    {
        body.push(format!(
            "时间账：「{}」估 {} 分钟，实际 {} 分钟，多了 {}。",
            worst.title,
            worst.estimate_min,
            worst.actual_min,
            worst.overrun()
        ));
    } else if !r.ledger.is_empty() {
        body.push("时间账：做完的都没超出估时。".into());
    }
    if !r.leftover_titles.is_empty() {
        body.push(format!(
            "没做完、已挪到明天：{}",
            r.leftover_titles.join("、")
        ));
    }
    body.push(String::new());
    body.push(format!(
        "明天草案 {} / {} 分钟：",
        tomorrow.used_min, tomorrow.capacity_min
    ));
    body.extend(
        tomorrow
            .planned
            .iter()
            .map(|p| format!("• {}（{} 分钟 · {}）", p.title, p.minutes, p.reason.label())),
    );
    body.extend(
        tomorrow
            .pushed
            .iter()
            .map(|p| format!("→ {} 挪到 {}：{}", p.title, p.to.format("%m-%d"), p.why)),
    );
    body.extend(
        tomorrow
            .nudges
            .iter()
            .map(|n| format!("! {}：{}", n.title, n.text)),
    );
    body.push(String::new());
    body.push(
        "在「今天」页看一眼明天草案，行就点「就这样排明天」；或者直接跟 Claude 说怎么调。".into(),
    );
    EventDraft {
        source: SOURCE.into(),
        title: if r.planned == 0 {
            "收尾：今天没排事，看看明天".into()
        } else {
            format!("收尾：今天完成 {}/{}", r.done, r.planned)
        },
        body: body.join("\n"),
        severity: Severity::Normal,
        tags: vec!["review".into()],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeekReview {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Done count per day, Monday first.
    pub done_by_day: Vec<(NaiveDate, usize)>,
    pub biggest_miss: Option<LedgerLine>,
    /// Open items pushed the most, most-deferred first.
    pub most_deferred: Vec<(String, u32)>,
    pub ideas_added: usize,
}

/// The week ending on `to` (inclusive, 7 days). `done_at` is converted to local
/// days with `tz`.
pub fn review_week<Tz: TimeZone>(items: &[Item], to: NaiveDate, tz: &Tz) -> WeekReview {
    let from = to - TimeDelta::days(6);
    let local_day = |t: DateTime<Utc>| t.with_timezone(tz).date_naive();
    let in_week = |d: NaiveDate| d >= from && d <= to;

    let done: Vec<&Item> = items
        .iter()
        .filter(|i| {
            i.status == ItemStatus::Done && i.done_at.is_some_and(|t| in_week(local_day(t)))
        })
        .collect();
    let done_by_day = (0..7)
        .map(|n| {
            let d = from + TimeDelta::days(n);
            (
                d,
                done.iter()
                    .filter(|i| i.done_at.map(local_day) == Some(d))
                    .count(),
            )
        })
        .collect();
    let biggest_miss = done
        .iter()
        .filter_map(|i| {
            Some(LedgerLine {
                title: i.title.clone(),
                estimate_min: i.estimate_min?,
                actual_min: i.actual_min?,
            })
        })
        .max_by_key(|l| l.overrun().abs())
        .filter(|l| l.overrun() != 0);
    let mut deferred: Vec<(String, u32)> = items
        .iter()
        .filter(|i| i.status == ItemStatus::Open && i.deferrals > 0)
        .map(|i| (i.title.clone(), i.deferrals))
        .collect();
    deferred.sort_by(|a, b| b.1.cmp(&a.1));
    deferred.truncate(3);
    WeekReview {
        from,
        to,
        done_by_day,
        biggest_miss,
        most_deferred: deferred,
        ideas_added: items
            .iter()
            .filter(|i| i.kind == ItemKind::Idea && in_week(local_day(i.created_at)))
            .count(),
    }
}

pub fn weekly_event(w: &WeekReview) -> EventDraft {
    let total: usize = w.done_by_day.iter().map(|(_, n)| n).sum();
    let busy_days = w.done_by_day.iter().filter(|(_, n)| *n > 0).count();
    let weekday = |d: NaiveDate| match d.weekday() {
        Weekday::Mon => "一",
        Weekday::Tue => "二",
        Weekday::Wed => "三",
        Weekday::Thu => "四",
        Weekday::Fri => "五",
        Weekday::Sat => "六",
        Weekday::Sun => "日",
    };
    let mut body = vec![format!(
        "每天完成：{}",
        w.done_by_day
            .iter()
            .map(|(d, n)| format!("周{} {n}", weekday(*d)))
            .collect::<Vec<_>>()
            .join(" · ")
    )];
    if let Some(m) = &w.biggest_miss {
        body.push(format!(
            "估时差最多：「{}」估 {} 实际 {} 分钟（写下来就行，不追问为什么）",
            m.title, m.estimate_min, m.actual_min
        ));
    }
    if !w.most_deferred.is_empty() {
        body.push(format!(
            "一直在拖：{}（要不要拆小，或者放弃？）",
            w.most_deferred
                .iter()
                .map(|(t, n)| format!("{t}（{n} 次）"))
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
    body.push(format!(
        "这周新点子：{} 个，记得去点子池挑一挑。",
        w.ideas_added
    ));
    body.push("下周想调整什么？直接跟 Claude 说。".into());
    EventDraft {
        source: SOURCE.into(),
        title: format!(
            "周复盘 {}–{}：完成 {total} 件，{busy_days}/7 天有进展",
            w.from.format("%m-%d"),
            w.to.format("%m-%d")
        ),
        body: body.join("\n"),
        severity: Severity::Normal,
        tags: vec!["review".into(), "weekly".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemChange, ItemDraft};
    use crate::planning::{plan_day, Calibration, PlannerSettings};
    use chrono::FixedOffset;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }
    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }
    fn todo(title: &str, day: u32, est: Option<u32>) -> Item {
        ItemDraft {
            title: title.into(),
            day: Some(d(day)),
            estimate_min: est,
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap()
    }
    fn done(i: Item, actual: Option<u32>) -> Item {
        ItemChange::Done { actual_min: actual }
            .apply(i, now())
            .unwrap()
    }

    #[test]
    fn day_review_counts_ledger_and_leftovers() {
        let items = vec![
            done(todo("回客户消息", 28, Some(20)), Some(45)),
            done(todo("买纸", 28, Some(5)), Some(5)),
            done(todo("没估时的", 28, None), Some(10)),
            todo("定价拍板", 28, Some(45)),
            ItemChange::Drop
                .apply(todo("算了", 28, None), now())
                .unwrap(),
            todo("明天的", 29, None),
        ];
        let r = review_day(&items, d(28));
        assert_eq!((r.done, r.planned), (3, 4), "dropped items don't count");
        assert_eq!(
            r.ledger.len(),
            2,
            "only items with both estimate and actual"
        );
        assert_eq!(r.leftover_titles, ["定价拍板"]);

        let plan = plan_day(
            &items,
            d(29),
            d(28),
            PlannerSettings {
                capacity_min: 180,
                default_estimate_min: 30,
            },
            &Calibration::default(),
        );
        let e = closing_event(&r, &plan);
        assert_eq!(e.title, "收尾：今天完成 3/4");
        assert!(
            e.body
                .contains("「回客户消息」估 20 分钟，实际 45 分钟，多了 25"),
            "{}",
            e.body
        );
        assert!(e.body.contains("已挪到明天：定价拍板"), "{}", e.body);
        assert!(e.body.contains("明天草案"), "{}", e.body);
    }

    #[test]
    fn week_review_uses_local_days_and_finds_patterns() {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut late_night = done(todo("深夜做完", 27, Some(30)), Some(90));
        // 2026-09-27 16:30 UTC = 09-28 00:30 in Beijing: counts for Monday the 28th.
        late_night.done_at = Some(Utc.with_ymd_and_hms(2026, 9, 27, 16, 30, 0).unwrap());
        let mut stale = todo("一直拖的", 28, Some(20));
        stale.deferrals = 4;
        let idea = ItemDraft {
            kind: ItemKind::Idea,
            title: "点子".into(),
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now())
        .unwrap();
        let w = review_week(&[late_night, stale, idea], d(28), &cst);
        assert_eq!(w.from, d(22));
        assert_eq!(w.done_by_day.last(), Some(&(d(28), 1)));
        assert_eq!(w.biggest_miss.as_ref().map(|l| l.actual_min), Some(90));
        assert_eq!(w.most_deferred, [("一直拖的".to_owned(), 4)]);
        assert_eq!(w.ideas_added, 1);
        let e = weekly_event(&w);
        assert!(e.title.contains("完成 1 件，1/7 天有进展"), "{}", e.title);
        assert!(e.body.contains("一直拖的（4 次）"), "{}", e.body);
    }
}
