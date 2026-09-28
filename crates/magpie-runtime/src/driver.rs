use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use tokio::sync::Notify;

use crate::Dispatcher;

/// Upper bound between ticks even when nothing is scheduled: covers wall-clock
/// jumps (sleep/resume, manual clock changes) that a single long sleep would miss.
pub const MAX_IDLE: Duration = Duration::from_secs(30);

/// Ticks forever. Wakes at the report's `next_wake`, after [`MAX_IDLE`], or as
/// soon as `changed` is notified (new event, ack, reminder).
pub async fn run<Tz>(dispatcher: Arc<Dispatcher>, changed: Arc<Notify>, tz: Tz)
where
    Tz: TimeZone + Send + Sync,
{
    loop {
        let now = Utc::now();
        let sleep_for = match dispatcher.tick(now, &tz).await {
            Ok(report) => {
                if report.reminders_fired
                    + report.followups_sent
                    + report.deliveries_sent
                    + report.deliveries_failed
                    > 0
                {
                    tracing::info!(?report, "tick");
                }
                report
                    .next_wake
                    .and_then(|t| (t - Utc::now()).to_std().ok())
                    .map_or(MAX_IDLE, |d| d.min(MAX_IDLE))
            }
            Err(e) => {
                tracing::error!(error = %e, "tick failed; retrying");
                Duration::from_secs(5)
            }
        };
        tokio::select! {
            () = tokio::time::sleep(sleep_for) => {}
            () = changed.notified() => {}
        }
    }
}
