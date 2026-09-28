use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use magpie_channels::{Channel, Message};
use magpie_core::{
    follow_ups, select_channels, ChannelSpec, Delivery, DeliveryOutcome, EscalationPolicy, Event,
    EventId, FollowUpSettings, ItemKind, ItemStatus, NextAction, Route, Tier,
};
use magpie_store::{DayFilter, ItemQuery, Store, StoreError};
use tokio::task::JoinSet;

/// Recorded as the channel when routing selected nothing, so a muted or
/// unconfigured event shows up as a visible failed delivery instead of being
/// retried every tick or vanishing silently.
pub const NO_CHANNEL: &str = "(none)";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickReport {
    pub reminders_fired: u32,
    pub followups_sent: u32,
    pub deliveries_sent: u32,
    pub deliveries_failed: u32,
    /// Latest moment the next tick is needed; `None` = nothing scheduled.
    pub next_wake: Option<DateTime<Utc>>,
}

impl TickReport {
    fn wake_at(&mut self, t: DateTime<Utc>) {
        self.next_wake = Some(self.next_wake.map_or(t, |w| w.min(t)));
    }
}

pub struct Dispatcher {
    store: Arc<Mutex<Store>>,
    channels: Vec<Arc<dyn Channel>>,
    specs: Vec<ChannelSpec>,
    routes: Vec<Route>,
    policy: EscalationPolicy,
    followups: Option<FollowUpSettings>,
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher")
            .field("channels", &self.specs)
            .field("routes", &self.routes.len())
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl Dispatcher {
    pub fn new(
        store: Arc<Mutex<Store>>,
        channels: Vec<Arc<dyn Channel>>,
        routes: Vec<Route>,
        policy: EscalationPolicy,
    ) -> Self {
        let specs = channels
            .iter()
            .map(|c| ChannelSpec {
                id: c.id().to_owned(),
                reach: c.reach(),
            })
            .collect();
        Self {
            store,
            channels,
            specs,
            routes,
            policy,
            followups: None,
        }
    }

    /// Enables the planner's own nudges: morning briefing, due reminders and
    /// "done?" checks for today's items.
    #[must_use]
    pub fn with_followups(mut self, settings: FollowUpSettings) -> Self {
        self.followups = Some(settings);
        self
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        // A poisoned lock means a panic mid-write elsewhere; SQLite rolled the
        // transaction back, so the data is still consistent. Keep going.
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// One pass at `now`. Store errors abort the pass; channel errors don't
    /// (they are recorded as failed deliveries).
    pub async fn tick<Tz: TimeZone>(
        &self,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Result<TickReport, StoreError> {
        let mut report = TickReport::default();
        self.fire_reminders(now, tz, &mut report)?;
        self.follow_up(now, tz, &mut report)?;

        let open = self.store().events_needing_attention()?;
        for event in open {
            self.advance(&event, now, &mut report).await?;
        }
        Ok(report)
    }

    fn fire_reminders<Tz: TimeZone>(
        &self,
        now: DateTime<Utc>,
        tz: &Tz,
        report: &mut TickReport,
    ) -> Result<(), StoreError> {
        let store = self.store();
        for r in store.reminders()? {
            if let Some(at) = r.due(now, tz) {
                match r.template.clone().validate(EventId::new(), now) {
                    Ok(event) => {
                        store.insert_event(&event)?;
                        report.reminders_fired += 1;
                    }
                    // Validated at creation, so only reachable if limits change
                    // between versions. Skip this occurrence, keep the reminder.
                    Err(e) => {
                        tracing::error!(reminder = %r.id, error = %e, "invalid reminder template");
                    }
                }
                store.mark_reminder_fired(r.id, at)?;
            }
            if r.enabled {
                if let Some(next) = r.schedule.next_fire(now, tz) {
                    report.wake_at(next);
                }
            }
        }
        Ok(())
    }

    fn follow_up<Tz: TimeZone>(
        &self,
        now: DateTime<Utc>,
        tz: &Tz,
        report: &mut TickReport,
    ) -> Result<(), StoreError> {
        let Some(settings) = self.followups else {
            return Ok(());
        };
        let local = now.with_timezone(tz).naive_local();
        let today = local.date();
        let store = self.store();
        let todays = store.list_items(&ItemQuery {
            kind: Some(ItemKind::Todo),
            day: DayFilter::On(today),
            ..ItemQuery::default()
        })?;
        let overdue = store
            .list_items(&ItemQuery {
                status: Some(ItemStatus::Open),
                kind: Some(ItemKind::Todo),
                day: DayFilter::Before(today),
                ..ItemQuery::default()
            })?
            .len();
        // Keys embed their day; two days of history covers any clock skew.
        let sent = store.followup_keys_since(now - TimeDelta::days(2))?;
        let plan = follow_ups(&todays, overdue, local, &sent, settings);
        for f in plan.send {
            match f.draft.validate(EventId::new(), now) {
                Ok(event) => {
                    store.insert_event(&event)?;
                    report.followups_sent += 1;
                }
                Err(e) => tracing::error!(key = %f.key, error = %e, "invalid follow-up"),
            }
            // Recorded even if invalid, so a bad one can't loop every tick.
            store.mark_followup(&f.key, now)?;
        }
        if let Some(next) = plan
            .next
            .and_then(|n| tz.from_local_datetime(&n).earliest())
        {
            report.wake_at(next.with_timezone(&Utc));
        }
        Ok(())
    }

    async fn advance(
        &self,
        event: &Event,
        now: DateTime<Utc>,
        report: &mut TickReport,
    ) -> Result<(), StoreError> {
        let (attempts, ack) = {
            let store = self.store();
            (store.attempt_times(event.id)?, store.latest_ack(event.id)?)
        };
        match self
            .policy
            .next_action(event.severity, &attempts, ack.as_ref(), now)
        {
            NextAction::Deliver(tier) => {
                let attempt = u32::try_from(attempts.len()).unwrap_or(u32::MAX);
                let deliveries = self.deliver(event, tier, attempt, now).await;
                let store = self.store();
                for d in &deliveries {
                    match d.outcome {
                        DeliveryOutcome::Sent => report.deliveries_sent += 1,
                        DeliveryOutcome::Failed { .. } => report.deliveries_failed += 1,
                    }
                    store.record_delivery(d)?;
                }
                if event.severity.escalates() {
                    report.wake_at(
                        now + TimeDelta::seconds(i64::from(self.policy.renotify_after_secs)),
                    );
                }
            }
            NextAction::WaitUntil(t) => report.wake_at(t),
            NextAction::Done | NextAction::Exhausted => {}
        }
        Ok(())
    }

    /// Delivers one round to all selected channels concurrently: a slow phone
    /// channel must not hold up the desktop toast.
    async fn deliver(
        &self,
        event: &Event,
        tier: Tier,
        attempt: u32,
        now: DateTime<Utc>,
    ) -> Vec<Delivery> {
        let selected = select_channels(&self.routes, &self.specs, event, tier);
        if selected.is_empty() {
            return vec![Delivery {
                event_id: event.id,
                channel: NO_CHANNEL.into(),
                tier,
                at: now,
                outcome: DeliveryOutcome::Failed {
                    error: "no channel selected: the matching route is muted or no channel is configured"
                        .into(),
                },
            }];
        }

        let msg = Arc::new(Message::new(event, tier, attempt));
        let mut set = JoinSet::new();
        for id in &selected {
            let Some(channel) = self.channels.iter().find(|c| c.id() == *id).cloned() else {
                continue;
            };
            let msg = Arc::clone(&msg);
            set.spawn(async move {
                let outcome = match channel.deliver(&msg).await {
                    Ok(()) => DeliveryOutcome::Sent,
                    Err(e) => DeliveryOutcome::Failed {
                        error: e.to_string(),
                    },
                };
                (channel.id().to_owned(), outcome)
            });
        }

        let mut out = Vec::with_capacity(selected.len());
        while let Some(joined) = set.join_next().await {
            let (channel, outcome) = joined.unwrap_or_else(|e| {
                (
                    "(panicked)".to_owned(),
                    DeliveryOutcome::Failed {
                        error: format!("channel task failed: {e}"),
                    },
                )
            });
            if let DeliveryOutcome::Failed { error } = &outcome {
                tracing::warn!(event = %event.id, %channel, %error, "delivery failed");
            }
            out.push(Delivery {
                event_id: event.id,
                channel,
                tier,
                at: now,
                outcome,
            });
        }
        // Deterministic order for storage and tests.
        out.sort_by(|a, b| a.channel.cmp(&b.channel));
        out
    }
}
