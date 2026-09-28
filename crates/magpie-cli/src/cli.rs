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
    /// What's on today: overdue + today's todos.
    Today,
    /// Todos: things you've committed to do.
    #[command(subcommand)]
    Todo(TodoCmd),
    /// The idea pool: things you might do, not yet scheduled.
    #[command(subcommand)]
    Idea(IdeaCmd),
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

#[derive(Debug, Args)]
pub struct ItemText {
    /// Start with a verb: "定价系数拍板", not "定价".
    pub title: String,
    /// Why it matters. The part that gets forgotten.
    #[arg(long, default_value = "")]
    pub why: String,
    /// What you concretely have to do.
    #[arg(long = "do", default_value = "")]
    pub action: String,
    /// Where it came from: a project, a conversation, a scheduled task.
    #[arg(long, env = "MAGPIE_SOURCE", default_value = "cli")]
    pub source: String,
    /// 工作 / 家里 / 孩子 …
    #[arg(long, default_value = "")]
    pub area: String,
}

#[derive(Debug, Subcommand)]
pub enum TodoCmd {
    /// Add a todo.
    Add {
        #[command(flatten)]
        text: ItemText,
        /// 今天 / 明天 / 周三 / 10-02 / 2026-10-02. Omit for "someday".
        #[arg(long)]
        day: Option<String>,
        /// Local time on that day: 19:50.
        #[arg(long)]
        at: Option<String>,
        /// Hard deadline (same formats as --day).
        #[arg(long)]
        due: Option<String>,
        /// Estimate: 30m, 1h30m.
        #[arg(long)]
        est: Option<String>,
        /// Other work waits on this; plan it first.
        #[arg(long)]
        blocking: bool,
    },
    /// List todos for a day (default today, plus anything overdue).
    List {
        #[arg(long)]
        day: Option<String>,
        /// Unscheduled todos only.
        #[arg(long, conflicts_with = "day")]
        someday: bool,
    },
    /// Mark done; --took feeds the time ledger.
    Done {
        id: String,
        #[arg(long)]
        took: Option<String>,
    },
    /// Push to a later day (default tomorrow).
    Defer {
        id: String,
        #[arg(long, default_value = "明天")]
        to: String,
    },
    /// Decide not to do it.
    Drop { id: String },
}

#[derive(Debug, Subcommand)]
pub enum IdeaCmd {
    /// Park an idea in the pool.
    Add {
        #[command(flatten)]
        text: ItemText,
    },
    /// Show the pool.
    List,
    /// Turn an idea into a todo on a day.
    Plan {
        id: String,
        #[arg(long)]
        day: String,
        #[arg(long)]
        at: Option<String>,
    },
    /// Discard an idea.
    Drop { id: String },
}
