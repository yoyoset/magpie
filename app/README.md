# app/ — Tauri shell

- `tauri.conf.json`: what every build needs (windows, CSP, icons).
- `tauri.release.conf.json`: only for `npm run release` at the repo root. It adds the
  `magpie` CLI as a sidecar and runs `scripts/prebuild.mjs` first.
  **Why separate:** with `externalBin` in the main config, `tauri-build` refuses to
  compile unless `binaries/magpie-<triple>.exe` exists, which breaks plain
  `cargo build` / `clippy` in dev and CI (measured 2026-09-28).
