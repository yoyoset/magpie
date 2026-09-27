# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/). Versions follow SemVer;
while < 1.0, minor bumps may break config or API.

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
