//! Magpie's engine.
//!
//! [`Dispatcher::tick`] is one deterministic pass at a given `now`: fire due
//! reminders, then move every open event one step along its escalation policy.
//! [`run`] drives ticks forever, waking early when the API signals a change.
//! Tauri, tray and windows are not in here; the app only wires this up.

mod dispatcher;
mod driver;

pub use dispatcher::{Dispatcher, TickReport, NO_CHANNEL};
pub use driver::run;
