use chrono::{DateTime, Local, NaiveDate, Utc};

/// Source of "now" for the server; injectable so tests are deterministic.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;

    /// The machine's local calendar day. "Today" is a local notion; this is
    /// the one place the server reads the local time zone.
    fn today(&self) -> NaiveDate {
        self.now().with_timezone(&Local).date_naive()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
