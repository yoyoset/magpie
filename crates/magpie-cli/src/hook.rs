//! Agent hook payloads → events. Pure mapping; IO is in `commands`.

use magpie_core::{EventDraft, Severity};
use serde_json::Value;

pub const SOURCE: &str = "claude-code";
/// Enough to recognize the answer on a phone screen, not a transcript dump.
const MAX_BODY_CHARS: usize = 1500;

/// Maps a Claude Code hook payload (stdin JSON) to an event, or `None` to stay quiet.
///
/// Field names follow <https://code.claude.com/docs/en/hooks>. The Notification
/// payload's text fields are not fully documented, so `message`/`title` are
/// read if present and the event still works without them.
///
/// `stop_severity` applies to `Stop`: it fires after *every* interactive turn,
/// so the default (`Info`) records it in the inbox without a toast. Scheduled or
/// unattended setups can raise it with `magpie hook claude-code --stop-severity normal`.
pub fn claude_code(payload: &Value, stop_severity: Severity) -> Option<EventDraft> {
    let project = project_name(payload);
    let mut tags = vec!["claude-code".to_owned()];
    if let Some(p) = &project {
        tags.push(p.clone());
    }
    let where_ = project.map(|p| format!(" · {p}")).unwrap_or_default();

    match payload["hook_event_name"].as_str()? {
        "Stop" => {
            // Set when Claude is continuing because a stop hook asked it to: the
            // turn is not really over, so don't announce it twice.
            if payload["stop_hook_active"].as_bool() == Some(true) {
                return None;
            }
            let body = payload["last_assistant_message"]
                .as_str()
                .map(|m| truncate(m.trim(), MAX_BODY_CHARS))
                .unwrap_or_default();
            Some(EventDraft {
                source: SOURCE.into(),
                title: format!("Claude 完成了{where_}"),
                body,
                severity: stop_severity,
                tags,
            })
        }
        "Notification" => {
            let kind = payload["notification_type"].as_str().unwrap_or("");
            let (severity, headline) = match kind {
                "permission_prompt"
                | "elicitation_dialog"
                | "elicitation_url_dialog"
                | "agent_needs_input" => (Severity::Urgent, "Claude 在等你"),
                "idle_prompt" => (Severity::Normal, "Claude 空闲了"),
                "" => (Severity::Normal, "Claude 通知"),
                _ => (Severity::Info, "Claude 通知"),
            };
            let message = payload["message"].as_str().unwrap_or("").trim();
            let title = payload["title"].as_str().unwrap_or("").trim();
            let body = [title, message]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            tags.push(format!(
                "notification:{}",
                if kind.is_empty() { "unknown" } else { kind }
            ));
            Some(EventDraft {
                source: SOURCE.into(),
                title: format!("{headline}{where_}"),
                body: truncate(&body, MAX_BODY_CHARS),
                severity,
                tags,
            })
        }
        _ => None,
    }
}

fn project_name(payload: &Value) -> Option<String> {
    let cwd = payload["cwd"].as_str()?;
    cwd.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", &s[..cut]),
        None => s.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude_code_default(payload: &Value) -> Option<EventDraft> {
        claude_code(payload, Severity::Info)
    }

    #[test]
    fn stop_is_quiet_by_default_and_raisable() {
        let p = json!({"hook_event_name": "Stop", "last_assistant_message": "done"});
        assert_eq!(
            claude_code(&p, Severity::Info).unwrap().severity,
            Severity::Info
        );
        assert_eq!(
            claude_code(&p, Severity::Normal).unwrap().severity,
            Severity::Normal
        );
    }

    #[test]
    fn stop_uses_last_message_and_project() {
        let e = claude_code_default(&json!({
            "hook_event_name": "Stop",
            "cwd": "F:\\my_ai\\viiyd3.0",
            "last_assistant_message": "  下周 5 条已备好。 "
        }))
        .unwrap();
        assert_eq!(e.title, "Claude 完成了 · viiyd3.0");
        assert_eq!(e.body, "下周 5 条已备好。");
        assert_eq!(e.severity, Severity::Info);
        assert_eq!(e.tags, vec!["claude-code", "viiyd3.0"]);
    }

    #[test]
    fn stop_continuation_is_silent() {
        assert!(
            claude_code_default(&json!({"hook_event_name":"Stop","stop_hook_active":true}))
                .is_none()
        );
    }

    #[test]
    fn permission_prompt_is_urgent() {
        let e = claude_code_default(&json!({
            "hook_event_name": "Notification",
            "notification_type": "permission_prompt",
            "message": "Claude needs your permission to use Bash",
            "cwd": "/home/u/proj/"
        }))
        .unwrap();
        assert_eq!(e.severity, Severity::Urgent);
        assert_eq!(e.title, "Claude 在等你 · proj");
        assert_eq!(e.body, "Claude needs your permission to use Bash");
        assert!(e
            .tags
            .contains(&"notification:permission_prompt".to_owned()));
    }

    #[test]
    fn housekeeping_notifications_are_info() {
        let e = claude_code_default(
            &json!({"hook_event_name":"Notification","notification_type":"auth_success"}),
        )
        .unwrap();
        assert_eq!(e.severity, Severity::Info);
    }

    #[test]
    fn other_hooks_ignored() {
        assert!(claude_code_default(&json!({"hook_event_name":"PreToolUse"})).is_none());
        assert!(claude_code_default(&json!({})).is_none());
    }

    #[test]
    fn long_messages_are_truncated_by_chars() {
        let long = "鹊".repeat(MAX_BODY_CHARS + 10);
        let e =
            claude_code_default(&json!({"hook_event_name":"Stop","last_assistant_message":long}))
                .unwrap();
        assert_eq!(e.body.chars().count(), MAX_BODY_CHARS + 1); // + ellipsis
        assert!(e.body.ends_with('…'));
    }
}
