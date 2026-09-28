use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use magpie_channels::{BoxFuture, Channel, ChannelError, Message};
use magpie_core::{
    Ack, AckKind, DeliveryOutcome, EscalationPolicy, EventDraft, EventId, Matcher, Reach, Reminder,
    ReminderId, Route, Schedule, Severity, Tier,
};
use magpie_runtime::{Dispatcher, TickReport};
use magpie_store::Store;

/// Records what it was asked to deliver; optionally fails.
struct FakeChannel {
    id: &'static str,
    reach: Reach,
    fail: bool,
    seen: Mutex<Vec<Message>>,
}

impl FakeChannel {
    fn new(id: &'static str, reach: Reach) -> Arc<Self> {
        Arc::new(Self {
            id,
            reach,
            fail: false,
            seen: Mutex::default(),
        })
    }
    fn failing(id: &'static str, reach: Reach) -> Arc<Self> {
        Arc::new(Self {
            id,
            reach,
            fail: true,
            seen: Mutex::default(),
        })
    }
    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl Channel for FakeChannel {
    fn id(&self) -> &str {
        self.id
    }
    fn reach(&self) -> Reach {
        self.reach
    }
    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        self.seen.lock().unwrap().push(msg.clone());
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                Err(ChannelError::Permanent("HTTP 401: Unauthorized".into()))
            } else {
                Ok(())
            }
        })
    }
}

fn t(min: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap() + TimeDelta::minutes(min)
}

const POLICY: EscalationPolicy = EscalationPolicy {
    renotify_after_secs: 600,
    phone_from_attempt: 1,
    max_attempts: 3,
};

struct World {
    store: Arc<Mutex<Store>>,
    toast: Arc<FakeChannel>,
    phone: Arc<FakeChannel>,
    dispatcher: Dispatcher,
}

fn world_with(routes: Vec<Route>, phone: Arc<FakeChannel>) -> World {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let toast = FakeChannel::new("toast", Reach::Local);
    let channels: Vec<Arc<dyn Channel>> = vec![toast.clone(), phone.clone()];
    let dispatcher = Dispatcher::new(Arc::clone(&store), channels, routes, POLICY);
    World {
        store,
        toast,
        phone,
        dispatcher,
    }
}

fn world() -> World {
    world_with(vec![], FakeChannel::new("phone", Reach::Remote))
}

fn post(w: &World, title: &str, severity: Severity, at: DateTime<Utc>) -> EventId {
    let e = EventDraft {
        source: "test".into(),
        title: title.into(),
        severity,
        ..EventDraft::default()
    }
    .validate(EventId::new(), at)
    .unwrap();
    w.store.lock().unwrap().insert_event(&e).unwrap();
    e.id
}

async fn tick(w: &World, at: DateTime<Utc>) -> TickReport {
    w.dispatcher.tick(at, &Utc).await.unwrap()
}

#[tokio::test]
async fn urgent_escalates_to_phone_then_stops_on_ack() {
    let w = world();
    let id = post(&w, "Claude 在等你", Severity::Urgent, t(0));

    let r = tick(&w, t(0)).await;
    assert_eq!(
        (w.toast.count(), w.phone.count()),
        (1, 0),
        "first round: desktop only"
    );
    assert_eq!(r.next_wake, Some(t(10)));

    tick(&w, t(5)).await;
    assert_eq!(
        (w.toast.count(), w.phone.count()),
        (1, 0),
        "nothing before renotify"
    );

    tick(&w, t(10)).await;
    assert_eq!(
        (w.toast.count(), w.phone.count()),
        (2, 1),
        "second round reaches the phone"
    );
    assert_eq!(w.phone.seen.lock().unwrap()[0].attempt, 1);
    assert_eq!(w.phone.seen.lock().unwrap()[0].tier, Tier::Phone);

    w.store
        .lock()
        .unwrap()
        .record_ack(&Ack {
            event_id: id,
            kind: AckKind::Acknowledged,
            via: "pet".into(),
            at: t(11),
        })
        .unwrap();
    let r = tick(&w, t(30)).await;
    assert_eq!((w.toast.count(), w.phone.count()), (2, 1), "acked: silence");
    assert_eq!(r.next_wake, None);
}

#[tokio::test]
async fn urgent_gives_up_after_max_attempts() {
    let w = world();
    post(&w, "x", Severity::Urgent, t(0));
    for m in [0, 10, 20, 30, 40] {
        tick(&w, t(m)).await;
    }
    assert_eq!(w.toast.count(), 3, "max_attempts = 3 rounds");
}

#[tokio::test]
async fn normal_delivers_once_info_never() {
    let w = world();
    post(&w, "normal", Severity::Normal, t(0));
    post(&w, "info", Severity::Info, t(0));
    let r = tick(&w, t(0)).await;
    assert_eq!(r.deliveries_sent, 1);
    tick(&w, t(60)).await;
    assert_eq!(w.toast.count(), 1);
    assert_eq!(w.phone.count(), 0);
}

#[tokio::test]
async fn failing_channel_is_recorded_not_swallowed() {
    let w = world_with(vec![], FakeChannel::failing("phone", Reach::Remote));
    let id = post(&w, "x", Severity::Critical, t(0));
    let r = tick(&w, t(0)).await;
    assert_eq!((r.deliveries_sent, r.deliveries_failed), (1, 1));

    let ds = w.store.lock().unwrap().deliveries(id).unwrap();
    let phone = ds.iter().find(|d| d.channel == "phone").unwrap();
    assert_eq!(
        phone.outcome,
        DeliveryOutcome::Failed {
            error: "HTTP 401: Unauthorized".into()
        }
    );
    assert!(
        ds.iter().all(|d| d.at == t(0)),
        "one round shares one timestamp"
    );
}

#[tokio::test]
async fn muted_route_records_a_visible_failure_once_per_round() {
    let mute = Route {
        name: "mute".into(),
        matcher: Matcher {
            source_prefix: Some("test".into()),
            ..Matcher::default()
        },
        channels: vec![],
    };
    let w = world_with(vec![mute], FakeChannel::new("phone", Reach::Remote));
    let id = post(&w, "x", Severity::Normal, t(0));
    tick(&w, t(0)).await;
    tick(&w, t(1)).await;

    let ds = w.store.lock().unwrap().deliveries(id).unwrap();
    assert_eq!(ds.len(), 1, "not retried every tick");
    assert_eq!(ds[0].channel, magpie_runtime::NO_CHANNEL);
    assert_eq!(w.toast.count(), 0);
}

#[tokio::test]
async fn reminder_fires_once_then_schedules_next() {
    let w = world();
    let reminder = Reminder {
        id: ReminderId::new(),
        schedule: Schedule::cron("0 12 * * *").unwrap(), // noon UTC daily
        template: EventDraft {
            source: "reminder".into(),
            title: "发小红书".into(),
            severity: Severity::Normal,
            ..EventDraft::default()
        },
        enabled: true,
        created_at: t(-60),
        last_fired: None,
    };
    w.store.lock().unwrap().save_reminder(&reminder).unwrap();

    let r = tick(&w, t(0)).await;
    assert_eq!(r.reminders_fired, 1);
    assert_eq!(
        w.toast.count(),
        1,
        "the fired event is delivered in the same tick"
    );
    assert_eq!(r.next_wake, Some(t(24 * 60)), "tomorrow noon");

    let r = tick(&w, t(1)).await;
    assert_eq!(r.reminders_fired, 0, "same occurrence never fires twice");

    // Three days offline: one catch-up event, not three.
    let r = tick(&w, t(3 * 24 * 60 + 30)).await;
    assert_eq!(r.reminders_fired, 1);
}

/// The real loop, real clock: a change notification must trigger delivery
/// immediately, not after the idle interval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_loop_reacts_to_change_notification() {
    let w = world();
    let toast = Arc::clone(&w.toast);
    let store = Arc::clone(&w.store);
    let changed = Arc::new(tokio::sync::Notify::new());
    let dispatcher = Arc::new(w.dispatcher);
    let handle = tokio::spawn(magpie_runtime::run(dispatcher, Arc::clone(&changed), Utc));

    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // first idle tick
    let e = EventDraft {
        source: "api".into(),
        title: "hi".into(),
        ..EventDraft::default()
    }
    .validate(EventId::new(), Utc::now())
    .unwrap();
    store.lock().unwrap().insert_event(&e).unwrap();
    changed.notify_one();

    let started = std::time::Instant::now();
    while toast.count() == 0 {
        assert!(
            started.elapsed().as_secs() < 2,
            "not delivered within 2s of notify"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    handle.abort();
}

#[tokio::test]
async fn followups_are_sent_once_and_delivered_in_the_same_tick() {
    use chrono::{FixedOffset, NaiveDate, NaiveTime};
    use magpie_core::{FollowUpSettings, ItemDraft, ItemId};

    let cst = FixedOffset::east_opt(8 * 3600).unwrap();
    // 20:56 Beijing time on 2026-09-28.
    let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 56, 0).unwrap();
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let toast = FakeChannel::new("toast", Reach::Local);
    let dispatcher = Dispatcher::new(
        Arc::clone(&store),
        vec![toast.clone() as Arc<dyn Channel>],
        vec![],
        POLICY,
    )
    .with_followups(FollowUpSettings::default());

    let item = ItemDraft {
        title: "发小红书第一条".into(),
        day: NaiveDate::from_ymd_opt(2026, 9, 28),
        at: NaiveTime::from_hms_opt(21, 0, 0),
        estimate_min: Some(10),
        ..ItemDraft::default()
    }
    .validate(ItemId::new(), now)
    .unwrap();
    store.lock().unwrap().save_item(&item).unwrap();

    let r = dispatcher.tick(now, &cst).await.unwrap();
    assert_eq!(
        r.followups_sent, 2,
        "briefing (after 09:00) + due (5 min before 21:00)"
    );
    let titles: Vec<String> = toast
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.title.clone())
        .collect();
    assert!(
        titles.contains(&"21:00 发小红书第一条".to_owned()),
        "{titles:?}"
    );
    assert!(
        titles.iter().any(|t| t.starts_with("今天 1 件事")),
        "{titles:?}"
    );
    // Next look is the earliest of: the urgent due reminder's re-notify
    // (12:56 + 10 min) and the "done?" check (21:30 local = 13:30 UTC).
    assert_eq!(
        r.next_wake,
        Some(Utc.with_ymd_and_hms(2026, 9, 28, 13, 6, 0).unwrap())
    );

    let again = dispatcher
        .tick(now + TimeDelta::minutes(1), &cst)
        .await
        .unwrap();
    assert_eq!(again.followups_sent, 0, "dedup keys persisted");

    let later = dispatcher
        .tick(Utc.with_ymd_and_hms(2026, 9, 28, 13, 31, 0).unwrap(), &cst)
        .await
        .unwrap();
    assert_eq!(later.followups_sent, 1, "the done? check");
}
