//! Which channels an event goes to.
//!
//! Two independent filters:
//! 1. **Route** (what the user configured): first matching route wins, like mail
//!    filters. No match falls back to every channel.
//! 2. **Tier** (how far escalation has gone): local channels always, remote ones
//!    only from `Tier::Phone` up.

use serde::{Deserialize, Serialize};

use crate::escalation::Tier;
use crate::event::{Event, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    /// On this machine: toast, pet, takeover.
    Local,
    /// Leaves the machine: Telegram, ntfy, webhook.
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSpec {
    pub id: String,
    pub reach: Reach,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matcher {
    /// Matches when the event source starts with this, e.g. `scheduled:`.
    #[serde(default)]
    pub source_prefix: Option<String>,
    #[serde(default)]
    pub min_severity: Option<Severity>,
    /// Matches when the event carries at least one of these tags. Empty = any.
    #[serde(default)]
    pub any_tag: Vec<String>,
}

impl Matcher {
    pub fn matches(&self, event: &Event) -> bool {
        let source_ok = self
            .source_prefix
            .as_deref()
            .is_none_or(|p| event.source.as_str().starts_with(p));
        let severity_ok = self.min_severity.is_none_or(|m| event.severity >= m);
        let tag_ok = self.any_tag.is_empty()
            || self
                .any_tag
                .iter()
                .any(|t| event.tags.iter().any(|e| e.eq_ignore_ascii_case(t)));
        source_ok && severity_ok && tag_ok
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub name: String,
    #[serde(default, rename = "match")]
    pub matcher: Matcher,
    /// Channel ids. Empty = mute: record in the inbox, deliver nowhere.
    pub channels: Vec<String>,
}

/// Channel ids to deliver `event` to at `tier`, in the order they are configured.
pub fn select_channels<'a>(
    routes: &[Route],
    channels: &'a [ChannelSpec],
    event: &Event,
    tier: Tier,
) -> Vec<&'a str> {
    let allowed: Option<&[String]> = routes
        .iter()
        .find(|r| r.matcher.matches(event))
        .map(|r| r.channels.as_slice());

    channels
        .iter()
        .filter(|c| allowed.is_none_or(|ids| ids.contains(&c.id)))
        .filter(|c| c.reach == Reach::Local || tier >= Tier::Phone)
        .map(|c| c.id.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventDraft, EventId};
    use chrono::{TimeZone, Utc};

    fn event(source: &str, severity: Severity, tags: &[&str]) -> Event {
        EventDraft {
            source: source.into(),
            title: "t".into(),
            body: String::new(),
            severity,
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
        }
        .validate(
            EventId::new(),
            Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap(),
        )
        .unwrap()
    }

    fn chans() -> Vec<ChannelSpec> {
        vec![
            ChannelSpec {
                id: "toast".into(),
                reach: Reach::Local,
            },
            ChannelSpec {
                id: "pet".into(),
                reach: Reach::Local,
            },
            ChannelSpec {
                id: "telegram".into(),
                reach: Reach::Remote,
            },
        ]
    }

    #[test]
    fn no_routes_means_all_channels_filtered_by_tier() {
        let e = event("cli", Severity::Urgent, &[]);
        assert_eq!(
            select_channels(&[], &chans(), &e, Tier::Desktop),
            vec!["toast", "pet"]
        );
        assert_eq!(
            select_channels(&[], &chans(), &e, Tier::Phone),
            vec!["toast", "pet", "telegram"]
        );
    }

    #[test]
    fn first_matching_route_wins() {
        let routes = vec![
            Route {
                name: "social to phone".into(),
                matcher: Matcher {
                    any_tag: vec!["social".into()],
                    ..Matcher::default()
                },
                channels: vec!["pet".into(), "telegram".into()],
            },
            Route {
                name: "everything else".into(),
                matcher: Matcher::default(),
                channels: vec!["toast".into()],
            },
        ];
        let social = event("scheduled:viiyd", Severity::Urgent, &["Social"]);
        assert_eq!(
            select_channels(&routes, &chans(), &social, Tier::Phone),
            vec!["pet", "telegram"]
        );
        let other = event("cli", Severity::Urgent, &[]);
        assert_eq!(
            select_channels(&routes, &chans(), &other, Tier::Phone),
            vec!["toast"]
        );
    }

    #[test]
    fn empty_route_mutes() {
        let routes = vec![Route {
            name: "mute noisy".into(),
            matcher: Matcher {
                source_prefix: Some("noisy".into()),
                ..Matcher::default()
            },
            channels: vec![],
        }];
        let e = event("noisy-bot", Severity::Critical, &[]);
        assert!(select_channels(&routes, &chans(), &e, Tier::Takeover).is_empty());
    }

    #[test]
    fn matcher_severity_floor() {
        let m = Matcher {
            min_severity: Some(Severity::Urgent),
            ..Matcher::default()
        };
        assert!(!m.matches(&event("x", Severity::Normal, &[])));
        assert!(m.matches(&event("x", Severity::Urgent, &[])));
        assert!(m.matches(&event("x", Severity::Critical, &[])));
    }

    #[test]
    fn misspelled_matcher_key_is_an_error() {
        let json = r#"{"name":"r","match":{"source_prefx":"claude"},"channels":[]}"#;
        assert!(serde_json::from_str::<Route>(json).is_err());
    }

    #[test]
    fn route_toml_shape_uses_match_key() {
        let json = r#"{"name":"r","match":{"source_prefix":"claude"},"channels":["toast"]}"#;
        let r: Route = serde_json::from_str(json).unwrap();
        assert_eq!(r.matcher.source_prefix.as_deref(), Some("claude"));
    }
}
