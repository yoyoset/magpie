//! `magpie today | todo … | idea …`: the human-facing (and AI-facing) planner.

use anyhow::{bail, Context};
use chrono::NaiveDate;
use magpie_core::{Item, ItemChange, ItemDraft, ItemKind, ItemStatus, Plan};

use crate::cli::{IdeaCmd, ItemText, TodoCmd};
use crate::client::Client;
use crate::commands::Output;
use crate::when;

fn minutes(s: &str) -> anyhow::Result<u32> {
    let m = when::duration(s)?.num_minutes();
    u32::try_from(m)
        .ok()
        .filter(|m| *m > 0)
        .with_context(|| format!("`{s}` is not a positive duration"))
}

fn draft(text: ItemText, kind: ItemKind) -> ItemDraft {
    ItemDraft {
        kind,
        title: text.title,
        why: text.why,
        action: text.action,
        source: text.source,
        area: text.area,
        ..ItemDraft::default()
    }
}

/// One line per item: `6bbe86f0  09-29 20:00  45m  [工作] 定价系数拍板  ⚑ 逾期 ↻2`.
pub fn format_item(i: &Item, today: NaiveDate) -> String {
    let when = match (i.day, i.at) {
        (Some(d), Some(t)) => format!("{} {}", d.format("%m-%d"), t.format("%H:%M")),
        (Some(d), None) => format!("{}      ", d.format("%m-%d")),
        _ => "someday    ".into(),
    };
    let est = i
        .estimate_min
        .map_or_else(|| "  -".to_owned(), |m| format!("{m:>3}m"));
    let mut tail = Vec::new();
    if i.blocking {
        tail.push("⚑".to_owned());
    }
    if i.is_overdue(today) {
        tail.push("逾期".to_owned());
    }
    if i.deferrals > 0 {
        tail.push(format!("↻{}", i.deferrals));
    }
    match i.status {
        ItemStatus::Done => tail.push("✓".to_owned()),
        ItemStatus::Dropped => tail.push("✗".to_owned()),
        ItemStatus::Open => {}
    }
    let area = if i.area.is_empty() {
        String::new()
    } else {
        format!("[{}] ", i.area)
    };
    format!(
        "{}  {when}  {est}  {area}{}{}",
        &i.id.to_string()[..8],
        i.title,
        if tail.is_empty() {
            String::new()
        } else {
            format!("  {}", tail.join(" "))
        }
    )
}

fn print_items(out: Output, items: &[Item], empty: &str) {
    let today = when::local_today();
    out.value(&items, || {
        if items.is_empty() {
            empty.to_owned()
        } else {
            items
                .iter()
                .map(|i| format_item(i, today))
                .collect::<Vec<_>>()
                .join("\n")
        }
    });
}

fn list(client: &Client, query: &[(&str, String)]) -> anyhow::Result<Vec<Item>> {
    Ok(client.get("/v1/items", query)?)
}

/// Full UUIDs pass through; otherwise a unique prefix among open items.
fn resolve(client: &Client, input: &str) -> anyhow::Result<String> {
    let input = input.trim().to_lowercase();
    if input.len() == 36 {
        return Ok(input);
    }
    let open = list(client, &[("status", "open".into())])?;
    let hits: Vec<String> = open
        .iter()
        .map(|i| i.id.to_string())
        .filter(|id| id.starts_with(&input))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no open item id starts with `{input}`"),
        many => bail!(
            "`{input}` matches {} open items; use more characters",
            many.len()
        ),
    }
}

fn change(client: &Client, out: Output, id: &str, change: &ItemChange) -> anyhow::Result<()> {
    let id = resolve(client, id)?;
    let item: Item = client.patch(&format!("/v1/items/{id}"), change)?;
    out.value(&item, || format_item(&item, when::local_today()));
    Ok(())
}

/// Human-readable plan: what's in and why, what's pushed and why, nudges.
pub fn format_plan(p: &Plan) -> String {
    let mut lines = vec![format!(
        "{} 计划：{} / {} 分钟{}",
        p.day.format("%m-%d"),
        p.used_min,
        p.capacity_min,
        if p.over_capacity {
            "  ⚠ 固定安排已经超出预算"
        } else {
            ""
        }
    )];
    for x in &p.planned {
        lines.push(format!(
            "  ✔ {:>4}m  {}  · {}",
            x.minutes,
            x.title,
            x.reason.label()
        ));
    }
    if !p.pushed.is_empty() {
        lines.push("  挪走：".into());
        for x in &p.pushed {
            lines.push(format!(
                "  → {}  {}  · {}",
                x.to.format("%m-%d"),
                x.title,
                x.why
            ));
        }
    }
    for n in &p.nudges {
        lines.push(format!("  ! {}  {}", n.title, n.text));
    }
    if p.planned.is_empty() && p.pushed.is_empty() {
        lines.push("  （这天没有要排的）".into());
    }
    lines.join(
        "
",
    )
}

pub fn plan(client: &Client, out: Output, day: &str, apply: bool) -> anyhow::Result<()> {
    let day = when::day(day, when::local_today())?;
    let plan: Plan = if apply {
        client.post("/v1/plan/apply", &serde_json::json!({ "day": day }))?
    } else {
        client.get("/v1/plan", &[("day", day.to_string())])?
    };
    out.value(&plan, || {
        let mut s = format_plan(&plan);
        if !apply {
            s.push_str(
                "
（这只是草案；确认后 `magpie plan ",
            );
            s.push_str(&day.to_string());
            s.push_str(" --apply`）");
        }
        s
    });
    Ok(())
}

pub fn today(client: &Client, out: Output) -> anyhow::Result<()> {
    let today = when::local_today();
    let mut items = list(
        client,
        &[("status", "open".into()), ("before", today.to_string())],
    )?;
    items.extend(list(client, &[("day", today.to_string())])?);
    print_items(out, &items, "今天没有待办。");
    Ok(())
}

pub fn todo(client: &Client, out: Output, cmd: TodoCmd) -> anyhow::Result<()> {
    let today = when::local_today();
    match cmd {
        TodoCmd::Add {
            text,
            day,
            at,
            due,
            est,
            blocking,
        } => {
            let mut d = draft(text, ItemKind::Todo);
            d.day = day.as_deref().map(|s| when::day(s, today)).transpose()?;
            d.at = at.as_deref().map(when::clock).transpose()?;
            d.deadline = due.as_deref().map(|s| when::day(s, today)).transpose()?;
            d.estimate_min = est.as_deref().map(minutes).transpose()?;
            d.blocking = blocking;
            let item: Item = client.post("/v1/items", &d)?;
            out.value(&item, || format!("added  {}", format_item(&item, today)));
            Ok(())
        }
        TodoCmd::List { day, someday } => {
            let items = if someday {
                list(
                    client,
                    &[
                        ("kind", "todo".into()),
                        ("status", "open".into()),
                        ("unscheduled", "true".into()),
                    ],
                )?
            } else if let Some(d) = day {
                list(
                    client,
                    &[
                        ("kind", "todo".into()),
                        ("day", when::day(&d, today)?.to_string()),
                    ],
                )?
            } else {
                return self::today(client, out);
            };
            print_items(out, &items, "没有待办。");
            Ok(())
        }
        TodoCmd::Done { id, took } => {
            let actual_min = took.as_deref().map(minutes).transpose()?;
            change(client, out, &id, &ItemChange::Done { actual_min })
        }
        TodoCmd::Defer { id, to } => change(
            client,
            out,
            &id,
            &ItemChange::Defer {
                to: when::day(&to, today)?,
            },
        ),
        TodoCmd::Drop { id } => change(client, out, &id, &ItemChange::Drop),
    }
}

pub fn idea(client: &Client, out: Output, cmd: IdeaCmd) -> anyhow::Result<()> {
    let today = when::local_today();
    match cmd {
        IdeaCmd::Add { text } => {
            let item: Item = client.post("/v1/items", &draft(text, ItemKind::Idea))?;
            out.value(&item, || format!("parked {}", format_item(&item, today)));
            Ok(())
        }
        IdeaCmd::List => {
            let items = list(
                client,
                &[("kind", "idea".into()), ("status", "open".into())],
            )?;
            print_items(out, &items, "点子池是空的。");
            Ok(())
        }
        IdeaCmd::Plan { id, day, at } => {
            let change_ = ItemChange::Schedule {
                day: when::day(&day, today)?,
                at: at.as_deref().map(when::clock).transpose()?,
            };
            change(client, out, &id, &change_)
        }
        IdeaCmd::Drop { id } => change(client, out, &id, &ItemChange::Drop),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveTime, TimeZone, Utc};
    use magpie_core::ItemId;

    #[test]
    fn line_shows_time_estimate_area_and_flags() {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let mut i = ItemDraft {
            title: "定价系数拍板".into(),
            area: "工作".into(),
            day: NaiveDate::from_ymd_opt(2026, 9, 27),
            at: NaiveTime::from_hms_opt(20, 0, 0),
            estimate_min: Some(45),
            blocking: true,
            ..ItemDraft::default()
        }
        .validate(ItemId::new(), now)
        .unwrap();
        i.deferrals = 2;
        let line = format_item(&i, NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
        assert!(line.contains("09-27 20:00"), "{line}");
        assert!(line.contains(" 45m"), "{line}");
        assert!(line.contains("[工作] 定价系数拍板"), "{line}");
        assert!(line.ends_with("⚑ 逾期 ↻2"), "{line}");
    }

    #[test]
    fn durations_to_minutes() {
        assert_eq!(minutes("1h30m").unwrap(), 90);
        assert!(minutes("10s").is_err(), "rounds to 0 minutes");
    }
}
