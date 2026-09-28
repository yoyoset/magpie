# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/). Versions follow SemVer;
while < 1.0, minor bumps may break config or API.

## [Unreleased] — v0.2 personal assistant (installed locally 2026-09-28 as build 0.1.0)

### Added
- Todos and ideas (`Item`): why / action / source / area, local day and time, deadline,
  estimate and actual minutes, blocking flag, deferral count. SQLite migrations v2 (items), v3 (followups).
- Capacity-based day planner with a reason for every inclusion and every push; time-ledger
  calibration (median actual/estimate per area); nudges for items deferred 3+ times.
- 今天 page (default tab) and 点子池; quick add; tick-off asks how long it took.
- Follow-ups: 09:00 briefing, urgent reminder 5 min before timed items, "做完了吗" afterwards.
- 21:00 closing review (tally, time ledger, carry-over, tomorrow's draft) and a Sunday weekly review.
- `magpie mcp`: MCP server (today, add_todo, add_idea, list, update, plan, notify).
- CLI: `today`, `todo add|list|done|defer|drop`, `idea add|list|plan|drop`, `plan [--apply]`.
- `MAGPIE_HOME` to point at another data directory.
- `docs/prompts/nightly-review.md`: prompt for the optional 21:10 Claude review task.

### Changed
- Pet redrawn as an abstract round magpie (owner feedback: less literal, no look-alike risk).
- Stop hook defaults to `info` (Claude Code fires it after every turn).

### Fixed
- A draft for tomorrow no longer pulls today's still-open items in as "overdue".

### Known issues
- NSIS uninstall leaves `HKCU\Software\vii\Magpie`, so a reinstall reuses the old directory.
- The pet window's transparent area blocks clicks underneath.

## [0.1.0] — unreleased

First usable version.

### Added
- Event inbox with four severities (info / normal / urgent / critical) and an
  escalation policy: re-notify, move to the phone, full-screen takeover, give up visibly.
- Reminders: one-shot, interval, cron (local time); missed runs coalesce into one.
- Local HTTP API (loopback only, bearer token in Windows Credential Manager).
- `magpie` CLI: notify / remind / list / ack / unremind / `ask` (wait for a human's
  answer on stdout) / `hook claude-code` (never blocks Claude: always exits 0).
- Channels: Windows toast, desktop pet, full-screen takeover with chime,
  Telegram (buttons + replies back), ntfy, webhook.
- Desktop app (Tauri 2): tray, inbox / reminders / settings UI, SVG magpie pet.
- Strict `config.toml`: unknown keys are errors; cross-field validation.

### Known limitations
See ROADMAP.md 「已知问题」.
