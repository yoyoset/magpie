# Magpie 喜鹊

> 喜鹊报信 —— the desktop messenger between your AI agents and you.

**中文** · AI 在后台干完活、需要你拍板时，Magpie 把结果收进一个收件箱，按轻重缓急提醒你：
桌面通知、桌宠气泡、全屏提醒、手机推送。没确认就升级，直到你处理；你在桌面、桌宠或手机上的回应
（确认、稍后、回复）会记录下来，`magpie ask` 还能把你的回答直接交还给发问的 AI。

**English** · Magpie is a desktop inbox and escalating notifier for AI agent results. Any agent,
scheduled task or script can post an event over a local HTTP API or the `magpie` CLI; Magpie routes
it to Windows toasts, a desktop pet, a full-screen alert or your phone (Telegram / ntfy), keeps
nagging until someone responds, and relays the response back.

## What it does

| Severity | Behavior |
|---|---|
| `info` | Inbox only |
| `normal` | One notification |
| `urgent` | Re-notifies every 10 min, moves to the phone on the 2nd round, gives up visibly after 6 |
| `critical` | Full-screen alert with chime + every channel, until answered |

Answer from anywhere — inbox window, the pet's speech bubble, the full-screen alert, or Telegram
buttons / replies.

## Install (Windows)

Download `Magpie_x.y.z_x64-setup.exe` from Releases and run it (per-user install, no admin).
The installer ships the `magpie` CLI next to the app. On first start Magpie writes a commented
`%APPDATA%\magpie\config.toml` and keeps secrets in Windows Credential Manager.

## Quick start

```bash
magpie notify "备份完成" -b "12 GB, 3 min"                 # tell me
magpie notify "NAS 不可达" -s critical                     # really tell me
magpie remind "发小红书" -s urgent --cron "50 19 * * 1,3,5" # remind me
answer=$(magpie ask "推送 5 条到手机？" --timeout 10m)       # ask me and wait
```

Claude Code hooks, scheduled-task prompts and the HTTP API: [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md),
[docs/API.md](docs/API.md).

## Design

Rust workspace, one crate per responsibility, dependencies only point down:

```
app (Tauri 2: tray, windows, pet, takeover, toasts)        ui/ (React + TS)
 ├─ magpie-runtime   reminders + escalation loop
 ├─ magpie-server    loopback HTTP API + shared ops layer
 ├─ magpie-channels  Telegram (two-way) / ntfy / webhook
 ├─ magpie-store     SQLite
 ├─ magpie-config    paths, config.toml, credential store
 └─ magpie-core      pure domain logic: no IO, no clock
magpie-cli           `magpie` command
```

Details and the reasoning behind them: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Progress,
decisions and known limitations: [ROADMAP.md](ROADMAP.md).

## Development

```bash
(cd ui && npm ci && npm run build)          # the app embeds ui/dist at compile time
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p magpie-app --features tauri/custom-protocol   # app with the built UI
npm ci && npm run release                   # NSIS installer -> target/release/bundle/nsis/
```

Plain `cargo run -p magpie-app` loads the Vite dev server (`cd ui && npm run dev`) instead.

## License

MIT
