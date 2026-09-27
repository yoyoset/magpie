//! `magpie` — command-line client for the Magpie desktop app.
//!
//! Exit codes: 0 ok · 1 error · 2 usage error · 3 Magpie not reachable.
//! `magpie hook …` always exits 0: in a Claude Code `Stop` hook, exit code 2
//! would stop Claude from finishing, and a notifier must never do that.

mod ask;
mod cli;
mod client;
mod commands;
mod hook;
mod when;

use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser as _;
use magpie_config::connection::{self, Connection};
use magpie_config::paths::Paths;
use magpie_config::secrets::OsKeyring;

use crate::cli::{Cli, Command, HookAgent};
use crate::client::{Client, ClientError};
use crate::commands::Output;

const TIMEOUT: Duration = Duration::from_secs(10);
/// Hooks run inline with the agent; don't hold it up when Magpie is down.
const HOOK_TIMEOUT: Duration = Duration::from_secs(3);

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // clap exits 2 on usage errors. Under a Claude Code Stop hook that would
        // block Claude from finishing, so a malformed `magpie hook …` exits 0.
        Err(e) if std::env::args().nth(1).as_deref() == Some("hook") => {
            eprintln!("magpie hook: {e}");
            return ExitCode::SUCCESS;
        }
        Err(e) => e.exit(),
    };
    let out = Output { json: cli.json };

    if let Command::Hook {
        agent,
        stop_severity,
    } = cli.command
    {
        run_hook(agent, stop_severity.into());
        return ExitCode::SUCCESS;
    }

    let result = resolve_connection().and_then(|conn| {
        let client = Client::new(conn, TIMEOUT);
        match cli.command {
            Command::Ask(a) => commands::ask(&client, out, a).map(Some),
            other => run(&client, out, other).map(|()| None),
        }
    });

    match result {
        Ok(Some(code)) => ExitCode::from(code),
        Ok(None) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("magpie: {e:#}");
            if matches!(e.downcast_ref(), Some(ClientError::Unreachable { .. })) {
                eprintln!("hint: is the Magpie app running?");
                ExitCode::from(3)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Every subcommand except `hook` and `ask`, which have their own exit-code rules.
fn run(client: &Client, out: Output, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Notify(a) => commands::notify(client, out, a.event),
        Command::Remind(a) => commands::remind(client, out, a),
        Command::List(a) => commands::list(client, out, &a),
        Command::Ack(a) => commands::ack(client, out, &a),
        Command::Unremind { id } => commands::unremind(client, out, &id),
        Command::Hook { .. } | Command::Ask(_) => unreachable!("handled in main"),
    }
}

fn resolve_connection() -> anyhow::Result<Connection> {
    let paths = Paths::default_location()?;
    Ok(connection::resolve(
        &|k| std::env::var(k).ok(),
        &paths,
        &OsKeyring,
    )?)
}

fn run_hook(agent: HookAgent, stop_severity: magpie_core::Severity) {
    let mut stdin = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut stdin) {
        eprintln!("magpie hook: cannot read stdin: {e}");
        return;
    }
    let client = resolve_connection()
        .ok()
        .map(|conn| Client::new(conn, HOOK_TIMEOUT));
    let result = match agent {
        HookAgent::ClaudeCode => commands::hook_claude_code(client.as_ref(), &stdin, stop_severity),
    };
    match result {
        Ok(msg) => eprintln!("magpie hook: {msg}"),
        Err(e) => eprintln!("magpie hook: {e:#}"),
    }
}
