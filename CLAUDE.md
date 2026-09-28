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

## 踩过的坑

- **门禁要写成条件提交**（`if 全绿; then git commit; fi`），否则测试红着也会提交：852b50f、8f72068 都是这样漏的。
- rustfmt 会把长断言折行，之后用 `sed`/单行 `replace` 改断言会静默不匹配；改前先 `grep -n` 看实际形态。

- `cargo build -p A -p B --example X` **只编 example**，不编 A 的二进制。测 CLI 前单独 `cargo build -p magpie-cli`，
  否则测的是旧二进制（栽过两次：M4、M9b）。
- debug 构建的 app 默认加载 Vite dev server（空白页）；要测真界面用 `--features tauri/custom-protocol`。
- 屏幕访问不可用时，界面验证走 WebView2 调试协议：
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`，再用 CDP `Runtime.evaluate` / `Page.captureScreenshot`。
- 本机 Python 控制台是 GBK，打印 emoji 会崩：脚本前 `export PYTHONIOENCODING=utf-8`。
  `python -c` 命令行里写中文字面量也会被嚼坏（匹配不到）：测试脚本按位置或 id 取，别按中文标题过滤。
- WAL 模式下只复制 `magpie.db` 会丢数据：要连 `-wal`、`-shm` 一起复制。
