//! Telegram → Magpie: long-polls the bot for button taps and replies and turns
//! them into acks through the shared ops layer.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use magpie_channels::telegram::inbound::{interpret, update_id, Inbound};
use magpie_channels::telegram::{CallbackAction, TelegramApi};
use magpie_channels::ChannelError;
use magpie_core::{AckKind, EventDraft, EventId, Severity};
use magpie_server::ops::{self, ListQuery};
use magpie_server::AppState;

use crate::services::SELF_SOURCE;

/// Server-side wait of each long poll; the HTTP timeout must exceed it.
pub const POLL_SECS: u32 = 25;
const MIN_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(120);
/// Bad token, webhook set on the bot, …: retrying fast won't fix it.
const PERMANENT_RETRY: Duration = Duration::from_secs(600);
const VIA: &str = "telegram";

pub async fn run(api: TelegramApi, chat_id: i64, state: AppState) {
    let mut offset = 0_i64;
    let mut backoff = MIN_BACKOFF;
    let mut reported = false;
    loop {
        match api.get_updates(offset, POLL_SECS).await {
            Ok(updates) => {
                backoff = MIN_BACKOFF;
                for u in &updates {
                    if let Some(id) = update_id(u) {
                        offset = offset.max(id + 1);
                    }
                    if let Some(intent) = interpret(u, chat_id) {
                        handle(&api, chat_id, &state, intent).await;
                    }
                }
            }
            Err(ChannelError::Permanent(e)) => {
                tracing::error!(error = %e, "telegram polling rejected");
                if !reported {
                    reported = true;
                    report(&state, &e).await;
                }
                tokio::time::sleep(PERMANENT_RETRY).await;
            }
            Err(ChannelError::Transient(e)) => {
                tracing::warn!(error = %e, ?backoff, "telegram polling failed");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// What a button means, and the little toast Telegram shows after the tap.
fn ack_for(action: CallbackAction, now: DateTime<Utc>) -> (AckKind, String) {
    match action {
        CallbackAction::Ack => (AckKind::Acknowledged, "✅ 已确认".into()),
        CallbackAction::Dismiss => (AckKind::Dismissed, "已忽略".into()),
        CallbackAction::Snooze { minutes } => (
            AckKind::Snoozed {
                until: now + TimeDelta::minutes(i64::from(minutes)),
            },
            format!("⏰ {minutes} 分钟后再提醒"),
        ),
    }
}

async fn handle(api: &TelegramApi, chat_id: i64, state: &AppState, intent: Inbound) {
    let result = match intent {
        Inbound::Button {
            callback_id,
            message_id,
            event,
            action,
        } => {
            let (kind, toast) = ack_for(action, Utc::now());
            match ops::ack_event(state, event, kind, VIA).await {
                Ok(()) => {
                    let _ = api.clear_buttons(chat_id, message_id).await;
                    api.answer_callback(&callback_id, &toast).await
                }
                Err(e) => api.answer_callback(&callback_id, &e.to_string()).await,
            }
        }
        Inbound::UnknownButton { callback_id } => {
            api.answer_callback(&callback_id, "这个按钮已失效").await
        }
        Inbound::Reply { event_ref, text } => {
            let answer = match resolve(state, &event_ref).await {
                Ok(id) => {
                    match ops::ack_event(state, id, AckKind::Replied { text: text.clone() }, VIA)
                        .await
                    {
                        Ok(()) => format!("✅ 已记下回复：{text}"),
                        Err(e) => format!("没记上：{e}"),
                    }
                }
                Err(msg) => msg,
            };
            api.send_text(chat_id, &answer).await
        }
        Inbound::Stray => {
            api.send_text(
                chat_id,
                "要回答某条提醒，请长按那条消息选「回复」，或直接点它下面的按钮。",
            )
            .await
        }
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "telegram answer failed");
    }
}

/// Finds the open event whose id starts with `short` (from `#abcd1234`).
async fn resolve(state: &AppState, short: &str) -> Result<EventId, String> {
    let open = ops::list_events(
        state,
        ListQuery {
            open: true,
            limit: Some(ops::MAX_PAGE),
            ..ListQuery::default()
        },
    )
    .await
    .map_err(|e| format!("读收件箱失败：{e}"))?;
    let matches: Vec<EventId> = open
        .iter()
        .map(|v| v.event.id)
        .filter(|id| id.0.simple().to_string().starts_with(short))
        .collect();
    match matches.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("#{short} 已经处理过了，或者不存在。")),
        _ => Err(format!("#{short} 对应多条事件，请在电脑上处理。")),
    }
}

/// Makes a polling failure visible in the inbox once, instead of only in logs.
async fn report(state: &AppState, error: &str) {
    let draft = EventDraft {
        source: SELF_SOURCE.into(),
        title: "Telegram 回传不可用".into(),
        body: format!(
            "手机上的按钮和回复暂时收不到，推送仍然正常。\n原因：{error}\n\
             常见原因：这个 bot 设置了 webhook（和别的服务共用同一个 bot），或 token 失效。"
        ),
        severity: Severity::Normal,
        tags: vec!["config".into(), "telegram".into()],
    };
    if let Err(e) = ops::create_event(state, draft).await {
        tracing::error!(error = %e, "could not report telegram failure");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn button_actions_map_to_acks() {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 20, 0, 0).unwrap();
        assert_eq!(ack_for(CallbackAction::Ack, now).0, AckKind::Acknowledged);
        assert_eq!(ack_for(CallbackAction::Dismiss, now).0, AckKind::Dismissed);
        let (kind, toast) = ack_for(CallbackAction::Snooze { minutes: 30 }, now);
        assert_eq!(
            kind,
            AckKind::Snoozed {
                until: Utc.with_ymd_and_hms(2026, 9, 28, 20, 30, 0).unwrap()
            }
        );
        assert!(toast.contains("30 分钟"));
    }
}
