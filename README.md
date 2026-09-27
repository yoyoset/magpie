# Magpie 喜鹊

> 喜鹊报信 —— the desktop messenger between your AI agents and you.

**中文** · AI 在后台干完活、需要你拍板时，Magpie 把结果收进一个收件箱，按轻重缓急提醒你：
桌面通知、桌宠气泡、手机推送。没确认就升级，直到你处理；你在桌面或手机上的回应（确认、稍后、回复）
会记录下来并回传给发起方。

**English** · Magpie is a desktop inbox and escalating notifier for AI agent results. Any agent,
scheduled task or script can post an event over a local HTTP API or CLI; Magpie routes it to
desktop toasts, a desktop pet, or your phone, keeps nagging until someone responds, and relays the
response (ack / snooze / reply) back to the source.

## Status

Early development. See [ROADMAP.md](ROADMAP.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Stack

Rust workspace (domain core, SQLite store, axum local API, delivery channels) · Tauri 2 shell ·
React + TypeScript UI.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd ui && npm ci && npm run build)          # UI must be built before the app embeds it
cargo run -p magpie-app --features tauri/custom-protocol   # app with the built UI
```

Plain `cargo run -p magpie-app` loads the Vite dev server (`cd ui && npm run dev`) instead.

## License

MIT
