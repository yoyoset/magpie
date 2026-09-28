# Magpie 喜鹊

> 喜鹊报信 —— the desktop messenger between your AI agents and you.

**中文** · 一个管你每天的「监工型助理」加 AI 结果信使。对话里冒出来的点子和「这步需要你做」
由 Claude 经 MCP 当场记进来；按每天的时间预算排好明天，排不下的说清为什么挪走；到点提醒、事后追问「做完了吗」、
早上简报、每晚 21:00 收尾对账并排好第二天。AI 在后台干完活或需要你拍板时，提醒会一路升级到桌宠、全屏和手机，
你的回答（确认、稍后、一句「发」）直接交还给 AI。

**English** · A supervising personal assistant and escalating notifier for AI agents. Todos and
ideas are captured by Claude over MCP the moment they come up; each day is planned within a time
budget with reasons for every trade-off; timed items get reminders and "done?" follow-ups; a
morning briefing and a 21:00 closing review keep the loop going. Agents and scripts post events over
a local API or the `magpie` CLI, and `magpie ask` hands the human's answer back.

## What it does

| Severity | Behavior |
|---|---|
| `info` | Inbox only |
| `normal` | One notification |
| `urgent` | Re-notifies every 10 min, moves to the phone on the 2nd round, gives up visibly after 6 |
| `critical` | Full-screen alert with chime + every channel, until answered |

Answer from anywhere — inbox window, the pet's speech bubble, the full-screen alert, or Telegram
buttons / replies.

**Planning** — the 今天 page shows overdue items, today's plan (tick off → "how long did it take?"),
urgent reminders waiting, and tomorrow's draft with reasons; 点子池 holds ideas until you decide.
Every evening at 21:00 Magpie tallies the day, carries leftovers over and drafts tomorrow; an
optional Claude task at 21:10 suggests trade-offs and asks you to confirm.

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
magpie todo add "定价系数拍板" --why "服务页闸门" --day 明天 --est 45m --blocking
magpie today                                                # overdue + today
magpie plan 明天                                            # draft; add --apply to commit it
magpie mcp                                                  # MCP server for Claude / OpenCode
```

Claude Code hooks, scheduled-task prompts and the HTTP API: [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md),
[docs/API.md](docs/API.md).

## Design

Rust workspace, one crate per responsibility, dependencies only point down:

```
app (Tauri 2: tray, windows, pet, takeover, toasts)        ui/ (React + TS)
 ├─ magpie-runtime   reminders, follow-ups, 21:00 review, escalation loop
 ├─ magpie-server    shared ops layer + loopback HTTP API
 ├─ magpie-channels  Telegram (two-way) / ntfy / webhook
 ├─ magpie-store     SQLite
 ├─ magpie-config    paths, config.toml, credential store
 └─ magpie-core      pure logic: events, escalation, items, planner, reviews
magpie-cli           `magpie` command and `magpie mcp`
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
