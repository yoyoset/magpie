# Magpie — 工程约定

桌面 AI 结果信使（Tauri 2 + Rust workspace + React/TS）。架构见 `docs/ARCHITECTURE.md`，进度见 `ROADMAP.md`。

## 不可破坏的约定

- **依赖只向下**：`core` ← `store` / `channels` ← `server` ← `app`。`core` 不引 tokio、不做 IO、不读系统时间，
  时间一律由参数 `now` 传入。
- **单一职责**：一个 crate 一个职责，一个模块一个概念。新增渠道 = 新文件实现 `Channel`，不改其它渠道。
- **密钥不落盘明文**：token 走 `keyring`；配置文件里只存非敏感项。日志里不打印 token 和消息全文以外的凭据。
- **API 只绑 127.0.0.1 且必须鉴权**；回调地址必须在配置白名单里。
- **错误分层**：库 crate 用 `thiserror` 定义具体错误；app/cli 边界用 `anyhow` 加上下文；给用户看的是人话摘要，
  技术细节进日志。后台失败必须在 UI 可见（收件箱里的失败投递），不能静默。

## 质量门禁

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
前端：`cd ui && npm run build`。全绿才提交；commit 一个逻辑改动一个。

## 环境事实

- Windows 11，Rust 1.92 msvc，Node 24。git bash 在 `D:\PortableGit\bin\bash.exe`。
- 数据与配置目录：`%APPDATA%/magpie/`。
