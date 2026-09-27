//! Argument definitions. Parsing only; behavior lives in `commands`.

use clap::{Args, Parser, Subcommand, ValueEnum};
use magpie_core::Severity;

#[derive(Debug, Parser)]
#[command(
    name = "magpie",
    version,
    about = "Magpie 喜鹊 — tell a human, and make sure they saw it."
)]
pub struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send an event to the inbox.
    Notify(NotifyArgs),
    /// Create a reminder that fires on a schedule.
    Remind(RemindArgs),
    /// List events or reminders.
    List(ListArgs),
    /// Ask a human and wait for the answer; prints it on stdout.
    /// Exit: 0 answered/acknowledged · 4 dismissed · 5 timed out (event stays open).
    Ask(AskArgs),
    /// Respond to an event: acknowledge (default), snooze, reply or dismiss.
    Ack(AckArgs),
    /// Delete a reminder.
    Unremind {
        /// Reminder id (full UUID).
        id: String,
    },
    /// Read an agent hook payload from stdin and forward it. Never blocks the agent:
    /// always exits 0, errors go to stderr.
    Hook {
        #[arg(value_enum)]
        agent: HookAgent,
        /// Severity for `Stop` (fires after every turn). Default: inbox only.
        #[arg(long, value_enum, default_value_t = SeverityArg::Info)]
        stop_severity: SeverityArg,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HookAgent {
    /// Claude Code `Stop` / `Notification` hooks.
    ClaudeCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SeverityArg {
    Info,
    Normal,
    Urgent,
    Critical,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::Info => Self::Info,
            SeverityArg::Normal => Self::Normal,
            SeverityArg::Urgent => Self::Urgent,
            SeverityArg::Critical => Self::Critical,
        }
    }
}

#[derive(Debug, Args)]
pub struct EventArgs {
    pub title: String,
    /// Longer text. Use `-` to read it from stdin.
    #[arg(short, long, default_value = "")]
    pub body: String,
    #[arg(short, long, value_enum, default_value_t = SeverityArg::Normal)]
    pub severity: SeverityArg,
    /// Who is sending this.
    #[arg(long, env = "MAGPIE_SOURCE", default_value = "cli")]
    pub source: String,
    /// Repeatable: `-t social -t xhs`.
    #[arg(short, long = "tag")]
    pub tags: Vec<String>,
}

#[derive(Debug, Args)]
pub struct NotifyArgs {
    #[command(flatten)]
    pub event: EventArgs,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("when").required(true).args(["at", "in", "every", "cron"])))]
pub struct RemindArgs {
    #[command(flatten)]
    pub event: EventArgs,
    /// Once, at local time: `2026-09-28 19:50` or `19:50` (today, or tomorrow if past).
    #[arg(long)]
    pub at: Option<String>,
    /// Once, after a delay: `30m`, `2h`, `1day`.
    #[arg(long = "in")]
    pub r#in: Option<String>,
    /// Repeatedly, starting now: `2h`, `1day`.
    #[arg(long)]
    pub every: Option<String>,
    /// 5-field cron in local time: `50 19 * * 1,3,5`.
    #[arg(long)]
    pub cron: Option<String>,
}

#[derive(Debug, Args)]
pub struct AskArgs {
    /// The question, e.g. "推送 5 条到手机？".
    pub title: String,
    /// Context the human needs to answer. Use `-` to read it from stdin.
    #[arg(short, long, default_value = "")]
    pub body: String,
    /// Defaults to urgent: a question someone is blocked on should escalate.
    #[arg(short, long, value_enum, default_value_t = SeverityArg::Urgent)]
    pub severity: SeverityArg,
    #[arg(long, env = "MAGPIE_SOURCE", default_value = "cli")]
    pub source: String,
    #[arg(short, long = "tag")]
    pub tags: Vec<String>,
    /// Give up waiting after this long: `10m`, `2h`.
    #[arg(long, default_value = "10m")]
    pub timeout: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// List reminders instead of events.
    #[arg(long)]
    pub reminders: bool,
    /// Include closed events.
    #[arg(long)]
    pub all: bool,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Debug, Args)]
pub struct AckArgs {
    /// Event id, or a unique prefix of an open event's id (as shown by `list`).
    pub id: String,
    /// Remind again after this long: `30m`.
    #[arg(long, conflicts_with_all = ["reply", "dismiss"])]
    pub snooze: Option<String>,
    /// Answer with text; relayed to the source when it registered a callback.
    #[arg(long, conflicts_with = "dismiss")]
    pub reply: Option<String>,
    /// Close without implying it was handled.
    #[arg(long)]
    pub dismiss: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    /// Catches definition mistakes (bad group ids, conflicts) that otherwise only
    /// panic when a user runs the command.
    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
