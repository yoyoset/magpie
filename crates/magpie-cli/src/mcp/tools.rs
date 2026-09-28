//! The MCP tools: thin wrappers over the local API, answering in plain Chinese
//! text the model can read back to the user.

use magpie_core::{Event, EventDraft, Item, ItemChange, ItemDraft, ItemKind, Plan, Severity};
use serde_json::{json, Value};

use super::ToolExecutor;
use crate::client::Client;
use crate::items::{format_item, format_plan};
use crate::when;

/// `None` when Magpie isn't reachable at startup; every call then explains why.
pub struct Tools {
    client: Option<Client>,
    offline_reason: String,
}

impl Tools {
    pub fn new(client: Result<Client, String>) -> Self {
        match client {
            Ok(c) => Self {
                client: Some(c),
                offline_reason: String::new(),
            },
            Err(reason) => Self {
                client: None,
                offline_reason: reason,
            },
        }
    }
}

fn s(args: &Value, key: &str) -> String {
    args[key].as_str().unwrap_or_default().trim().to_owned()
}

fn opt(args: &Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn day_arg(args: &Value, key: &str) -> Result<Option<chrono::NaiveDate>, String> {
    opt(args, key)
        .map(|v| when::day(&v, when::local_today()).map_err(|e| e.to_string()))
        .transpose()
}

fn minutes_arg(args: &Value, key: &str) -> Result<Option<u32>, String> {
    match &args[key] {
        Value::Null => Ok(None),
        v => v
            .as_u64()
            .and_then(|m| u32::try_from(m).ok())
            .map(Some)
            .ok_or_else(|| format!("`{key}` must be a whole number of minutes")),
    }
}

const DAY_HELP: &str = "日期：今天 / 明天 / 后天 / 周三 / 10-02 / 2026-10-02";

fn schema_list() -> Value {
    json!([
        {
            "name": "today",
            "description": "用户今天要做的事：逾期未做的 + 今天的待办（含完成情况），以及等用户回应的紧急提醒数。用户问「今天干什么」时用。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "add_todo",
            "description": "记一件用户要做的事。对话里一出现「这步需要你来做」「你记得去…」就调用，别等对话结束。\
                            标题动词开头（「定价系数拍板」而不是「定价」）；why 写为什么重要（最容易忘的部分）；\
                            action 写用户具体要做什么；source 写来自哪（项目/对话主题）。不确定哪天做就不填 day。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "why": { "type": "string" },
                    "action": { "type": "string", "description": "用户具体要做的动作" },
                    "source": { "type": "string", "description": "来自哪个项目或对话" },
                    "area": { "type": "string", "description": "工作 / 家里 / 孩子" },
                    "day": { "type": "string", "description": DAY_HELP },
                    "at": { "type": "string", "description": "当天几点，HH:MM" },
                    "deadline": { "type": "string", "description": format!("硬截止。{DAY_HELP}") },
                    "estimate_min": { "type": "integer", "description": "估计要多少分钟" },
                    "blocking": { "type": "boolean", "description": "别的事在等它" }
                },
                "required": ["title", "why"]
            }
        },
        {
            "name": "add_idea",
            "description": "把一个点子放进点子池（不排日期，以后晚间复盘时再决定做不做）。对话里冒出「以后可以…」「要不要试试…」时用。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "why": { "type": "string" },
                    "source": { "type": "string" },
                    "area": { "type": "string" }
                },
                "required": ["title"]
            }
        },
        {
            "name": "list",
            "description": "按条件列事项。what: day（某天，需 day）/ ideas（点子池）/ someday（未排期的待办）/ overdue（逾期）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "what": { "type": "string", "enum": ["day", "ideas", "someday", "overdue"] },
                    "day": { "type": "string", "description": DAY_HELP }
                },
                "required": ["what"]
            }
        },
        {
            "name": "update",
            "description": "改一条事项。id 可以用前 6-8 位。op: done（可带 actual_min）/ defer（to）/ schedule（day, 可带 at；点子排期也用它）/ drop / reopen / estimate（minutes）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "op": { "type": "string", "enum": ["done", "defer", "schedule", "drop", "reopen", "estimate"] },
                    "actual_min": { "type": "integer" },
                    "to": { "type": "string", "description": DAY_HELP },
                    "day": { "type": "string", "description": DAY_HELP },
                    "at": { "type": "string" },
                    "minutes": { "type": "integer" }
                },
                "required": ["id", "op"]
            }
        },
        {
            "name": "plan",
            "description": "按每天的时间预算为某天出排期草案：排进哪些、为什么、放不下的挪到哪天及原因。apply=true 才真正生效——先把草案给用户看、用户同意后再 apply。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "day": { "type": "string", "description": format!("默认明天。{DAY_HELP}") },
                    "apply": { "type": "boolean" }
                }
            }
        },
        {
            "name": "notify",
            "description": "给用户发一条提醒（桌面/桌宠/手机）。severity: normal / urgent（没回应会追）/ critical（全屏）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "severity": { "type": "string", "enum": ["info", "normal", "urgent", "critical"] }
                },
                "required": ["title"]
            }
        }
    ])
}

fn resolve(client: &Client, prefix: &str) -> Result<String, String> {
    let p = prefix.trim().to_lowercase();
    if p.len() == 36 {
        return Ok(p);
    }
    let items = get_items(client, &[("status", "open".into())])?;
    let hits: Vec<String> = items
        .iter()
        .map(|i| i.id.to_string())
        .filter(|id| id.starts_with(&p))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!("没有以 `{p}` 开头的未完成事项")),
        many => Err(format!("`{p}` 对应 {} 条，给更长的 id", many.len())),
    }
}

fn get_items(client: &Client, q: &[(&str, String)]) -> Result<Vec<Item>, String> {
    client.get("/v1/items", q).map_err(|e| e.to_string())
}

fn lines(items: &[Item], empty: &str) -> String {
    if items.is_empty() {
        return empty.to_owned();
    }
    let today = when::local_today();
    items
        .iter()
        .map(|i| format_item(i, today))
        .collect::<Vec<_>>()
        .join("\n")
}

fn clock_arg(args: &Value, key: &str) -> Result<Option<chrono::NaiveTime>, String> {
    opt(args, key)
        .map(|t| when::clock(&t).map_err(|e| e.to_string()))
        .transpose()
}

fn today(client: &Client) -> Result<String, String> {
    let today = when::local_today();
    let overdue = get_items(
        client,
        &[
            ("status", "open".into()),
            ("kind", "todo".into()),
            ("before", today.to_string()),
        ],
    )?;
    let todays = get_items(
        client,
        &[("kind", "todo".into()), ("day", today.to_string())],
    )?;
    let events: Vec<Value> = client
        .get("/v1/events", &[("open", "true".into())])
        .map_err(|e| e.to_string())?;
    let urgent = events
        .iter()
        .filter(|e| matches!(e["severity"].as_str(), Some("urgent" | "critical")))
        .count();
    Ok(format!(
        "{today}\n逾期 {}：\n{}\n今天 {}：\n{}\n等用户回应的紧急提醒：{urgent}",
        overdue.len(),
        lines(&overdue, "  （无）"),
        todays.len(),
        lines(&todays, "  （无）"),
    ))
}

fn add(client: &Client, kind: ItemKind, args: &Value) -> Result<String, String> {
    let draft = ItemDraft {
        kind,
        title: s(args, "title"),
        why: s(args, "why"),
        action: s(args, "action"),
        source: opt(args, "source").unwrap_or_else(|| "claude".into()),
        area: s(args, "area"),
        day: if kind == ItemKind::Todo {
            day_arg(args, "day")?
        } else {
            None
        },
        at: clock_arg(args, "at")?,
        deadline: day_arg(args, "deadline")?,
        estimate_min: minutes_arg(args, "estimate_min")?,
        blocking: args["blocking"].as_bool().unwrap_or(false),
    };
    let item: Item = client
        .post("/v1/items", &draft)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "已记下：{}",
        format_item(&item, when::local_today())
    ))
}

fn list(client: &Client, args: &Value) -> Result<String, String> {
    let today = when::local_today().to_string();
    let open = || ("status", "open".to_owned());
    let items = match s(args, "what").as_str() {
        "day" => {
            let d = day_arg(args, "day")?.ok_or("what=day 需要 day")?;
            get_items(client, &[("kind", "todo".into()), ("day", d.to_string())])?
        }
        "ideas" => get_items(client, &[("kind", "idea".into()), open()])?,
        "someday" => get_items(
            client,
            &[
                ("kind", "todo".into()),
                open(),
                ("unscheduled", "true".into()),
            ],
        )?,
        "overdue" => get_items(
            client,
            &[("kind", "todo".into()), open(), ("before", today)],
        )?,
        other => return Err(format!("未知的 what: `{other}`")),
    };
    Ok(lines(&items, "（空）"))
}

fn update(client: &Client, args: &Value) -> Result<String, String> {
    let id = resolve(client, &s(args, "id"))?;
    let change = match s(args, "op").as_str() {
        "done" => ItemChange::Done {
            actual_min: minutes_arg(args, "actual_min")?,
        },
        "defer" => ItemChange::Defer {
            to: day_arg(args, "to")?.ok_or("defer 需要 to")?,
        },
        "schedule" => ItemChange::Schedule {
            day: day_arg(args, "day")?.ok_or("schedule 需要 day")?,
            at: clock_arg(args, "at")?,
        },
        "drop" => ItemChange::Drop,
        "reopen" => ItemChange::Reopen,
        "estimate" => ItemChange::Estimate {
            minutes: minutes_arg(args, "minutes")?.ok_or("estimate 需要 minutes")?,
        },
        other => return Err(format!("未知的 op: `{other}`")),
    };
    let item: Item = client
        .patch(&format!("/v1/items/{id}"), &change)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "已更新：{}",
        format_item(&item, when::local_today())
    ))
}

fn plan(client: &Client, args: &Value) -> Result<String, String> {
    let day = day_arg(args, "day")?.unwrap_or(when::local_today() + chrono::TimeDelta::days(1));
    let apply = args["apply"].as_bool().unwrap_or(false);
    let plan: Plan = if apply {
        client.post("/v1/plan/apply", &json!({ "day": day }))
    } else {
        client.get("/v1/plan", &[("day", day.to_string())])
    }
    .map_err(|e| e.to_string())?;
    let note = if apply {
        "（已生效）"
    } else {
        "（草案，未生效；用户同意后用 apply=true）"
    };
    Ok(format!("{}\n{note}", format_plan(&plan)))
}

fn notify(client: &Client, args: &Value) -> Result<String, String> {
    let severity = match s(args, "severity").as_str() {
        "" | "normal" => Severity::Normal,
        "info" => Severity::Info,
        "urgent" => Severity::Urgent,
        "critical" => Severity::Critical,
        other => return Err(format!("未知的 severity: `{other}`")),
    };
    let draft = EventDraft {
        source: "claude".into(),
        title: s(args, "title"),
        body: s(args, "body"),
        severity,
        tags: vec![],
    };
    let e: Event = client
        .post("/v1/events", &draft)
        .map_err(|e| e.to_string())?;
    Ok(format!("已提醒（{}）", &e.id.to_string()[..8]))
}

impl ToolExecutor for Tools {
    fn list(&self) -> Value {
        schema_list()
    }

    fn call(&self, name: &str, args: &Value) -> Result<String, String> {
        let Some(c) = &self.client else {
            return Err(format!(
                "Magpie 没连上：{}（桌面程序开着吗？）",
                self.offline_reason
            ));
        };
        match name {
            "today" => today(c),
            "add_todo" => add(c, ItemKind::Todo, args),
            "add_idea" => add(c, ItemKind::Idea, args),
            "list" => list(c, args),
            "update" => update(c, args),
            "plan" => plan(c, args),
            "notify" => notify(c, args),
            other => Err(format!("unknown tool `{other}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_valid_schema_and_description() {
        let tools = schema_list();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["today", "add_todo", "add_idea", "list", "update", "plan", "notify"]
        );
        for t in tools.as_array().unwrap() {
            assert!(
                t["description"]
                    .as_str()
                    .is_some_and(|d| d.chars().count() > 10),
                "{t}"
            );
            assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        }
    }

    #[test]
    fn offline_tools_explain_instead_of_failing_silently() {
        let t = Tools::new(Err("endpoint.json missing".into()));
        let err = t.call("today", &json!({})).unwrap_err();
        assert!(err.contains("endpoint.json missing"), "{err}");
    }

    #[test]
    fn minutes_arg_rejects_non_integers() {
        assert_eq!(minutes_arg(&json!({"m": 30}), "m"), Ok(Some(30)));
        assert_eq!(minutes_arg(&json!({}), "m"), Ok(None));
        assert!(minutes_arg(&json!({"m": "half an hour"}), "m").is_err());
    }
}
