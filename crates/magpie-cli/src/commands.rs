//! One function per subcommand: build the request, call the API, print.

use std::io::Read;

use anyhow::{bail, Context};
use chrono::{DateTime, Local, Utc};
use magpie_core::{Event, EventDraft, Reminder, Schedule, Severity};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::{AckArgs, EventArgs, ListArgs, RemindArgs};
use crate::client::Client;
use crate::{hook, when};

#[derive(Debug, Clone, Copy)]
pub struct Output {
    pub json: bool,
}

impl Output {
    fn value(self, v: &impl serde::Serialize, human: impl FnOnce() -> String) {
        if self.json {
            println!("{}", serde_json::to_string_pretty(v).expect("serializable"));
        } else {
            println!("{}", human());
        }
    }
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct EventRow {
    #[serde(flatten)]
    event: Event,
    closed_at: Option<DateTime<Utc>>,
}

fn draft(args: EventArgs) -> anyhow::Result<EventDraft> {
    let body = if args.body == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .context("reading body from stdin")?;
        s
    } else {
        args.body
    };
    Ok(EventDraft {
        source: args.source,
        title: args.title,
        body,
        severity: args.severity.into(),
        tags: args.tags,
    })
}

pub fn notify(client: &Client, out: Output, args: EventArgs) -> anyhow::Result<()> {
    let event: Event = client.post("/v1/events", &draft(args)?)?;
    out.value(&event, || format!("sent {}", short(&event.id.to_string())));
    Ok(())
}

pub fn remind(client: &Client, out: Output, args: RemindArgs) -> anyhow::Result<()> {
    let now = Utc::now();
    let schedule = if let Some(s) = &args.at {
        Schedule::Once {
            at: when::at(s, now, &Local)?,
        }
    } else if let Some(s) = &args.r#in {
        Schedule::Once {
            at: now + when::duration(s)?,
        }
    } else if let Some(s) = &args.every {
        let secs = u64::try_from(when::duration(s)?.num_seconds()).context("negative interval")?;
        Schedule::interval(now, secs)?
    } else if let Some(s) = &args.cron {
        Schedule::cron(s)?
    } else {
        unreachable!("clap requires one of --at/--in/--every/--cron");
    };
    let body = json!({ "schedule": schedule, "template": draft(args.event)? });
    let r: Reminder = client.post("/v1/reminders", &body)?;
    let next = r.schedule.next_fire(now, &Local);
    out.value(&r, || match next {
        Some(t) => format!("reminder {} set, next at {}", r.id, when::local_short(t)),
        None => format!("reminder {} set", r.id),
    });
    Ok(())
}

pub fn list(client: &Client, out: Output, args: &ListArgs) -> anyhow::Result<()> {
    if args.reminders {
        let rs: Vec<Reminder> = client.get("/v1/reminders", &[])?;
        let now = Utc::now();
        out.value(&rs, || {
            if rs.is_empty() {
                return "no reminders".into();
            }
            rs.iter()
                .map(|r| {
                    let next = r
                        .schedule
                        .next_fire(now, &Local)
                        .filter(|_| r.enabled)
                        .map_or_else(|| "—".to_owned(), when::local_short);
                    format!("{}  next {:<11}  {}", r.id, next, r.template.title)
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        return Ok(());
    }

    let mut query = vec![("limit", args.limit.to_string())];
    if !args.all {
        query.push(("open", "true".into()));
    }
    let rows: Vec<EventRow> = client.get("/v1/events", &query)?;
    out.value(&rows, || {
        if rows.is_empty() {
            return if args.all {
                "inbox is empty"
            } else {
                "nothing open"
            }
            .into();
        }
        rows.iter().map(format_row).collect::<Vec<_>>().join("\n")
    });
    Ok(())
}

fn format_row(r: &EventRow) -> String {
    let e = &r.event;
    let state = if r.closed_at.is_some() {
        "closed"
    } else {
        "open"
    };
    format!(
        "{}  {:<8} {:<6} {}  {:<14} {}",
        short(&e.id.to_string()),
        severity_label(e.severity),
        state,
        when::local_short(e.created_at),
        e.source,
        e.title
    )
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Normal => "normal",
        Severity::Urgent => "URGENT",
        Severity::Critical => "CRITICAL",
    }
}

fn short(id: &str) -> &str {
    &id[..8.min(id.len())]
}

pub fn ack(client: &Client, out: Output, args: &AckArgs) -> anyhow::Result<()> {
    let id = resolve_event_id(client, &args.id)?;
    let body = if let Some(s) = &args.snooze {
        json!({ "kind": "snoozed", "until": Utc::now() + when::duration(s)?, "via": "cli" })
    } else if let Some(text) = &args.reply {
        json!({ "kind": "replied", "text": text, "via": "cli" })
    } else if args.dismiss {
        json!({ "kind": "dismissed", "via": "cli" })
    } else {
        json!({ "kind": "acknowledged", "via": "cli" })
    };
    let _: Value = client.post(&format!("/v1/events/{id}/ack"), &body)?;
    out.value(&json!({ "id": id, "ack": body }), || {
        format!("ok {}", short(&id))
    });
    Ok(())
}

/// Full UUIDs pass through; otherwise match a unique prefix among open events.
fn resolve_event_id(client: &Client, input: &str) -> anyhow::Result<String> {
    let input = input.trim().to_lowercase();
    if uuid_like(&input) {
        return Ok(input);
    }
    let open: Vec<EventRow> = client.get(
        "/v1/events",
        &[("open", "true".into()), ("limit", "500".into())],
    )?;
    let matches: Vec<String> = open
        .iter()
        .map(|r| r.event.id.to_string())
        .filter(|id| id.starts_with(&input))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no open event id starts with `{input}`"),
        many => bail!(
            "`{input}` matches {} open events; use more characters",
            many.len()
        ),
    }
}

fn uuid_like(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn unremind(client: &Client, out: Output, id: &str) -> anyhow::Result<()> {
    client.delete(&format!("/v1/reminders/{}", id.trim()))?;
    out.value(&json!({ "deleted": id }), || format!("deleted {id}"));
    Ok(())
}

/// Reads the hook payload from stdin and forwards it. Returns what happened so
/// `main` can report it on stderr; the exit code is always 0 regardless.
pub fn hook_claude_code(client: Option<&Client>, stdin: &str) -> anyhow::Result<String> {
    let payload: Value = serde_json::from_str(stdin).context("hook stdin is not JSON")?;
    let Some(draft) = hook::claude_code(&payload) else {
        return Ok("ignored".into());
    };
    let client = client.context("Magpie connection not configured")?;
    let event: Event = client.post("/v1/events", &draft)?;
    Ok(format!("sent {}", short(&event.id.to_string())))
}
