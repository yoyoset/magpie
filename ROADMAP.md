# Magpie 路线图

每轮 loop 只做**一个**里程碑（太大就拆成子项做一部分），做完的打 `[x]` 并在「进度日志」记一行。

## 每轮协议（loop 每次醒来照做）

1. 读本文件 + `CLAUDE.md`，找第一个未完成项。
2. 实现。核心逻辑写成纯函数并配单元测试。
3. `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` 全绿；
   动了前端再加 `npm run build`（在 `ui/`）。不绿不提交。
4. 一个逻辑改动一个 commit，message 写清楚做了什么、为什么、测了什么。
5. 更新本文件：打勾、进度日志加一行、新发现的问题写进「已知问题」。
6. 遇到只有用户能决定的事（账号、token、要花钱的服务），写进「等用户」，跳过做下一项，不要卡住。

## 里程碑

- [x] **M0 骨架**：git init、Cargo workspace、rustfmt/clippy(pedantic) 配置、`.gitignore`、README（中英）、
      core 的 Event/EventDraft 校验/Severity/Ack + 9 个测试。其它 crate 到用它的里程碑再建，不建空壳
- [x] **M1 调度与升级（core）**：`Schedule`（once/cron/interval）→ `next_fire(now)`；`EscalationPolicy` →
      `next_action(event, deliveries, now)`；`Route` 匹配。纯函数 + 边界测试（DST 无关、跨月、错过的触发）。
      实测：DST 不存在的时刻 croner 顺延到当天第一个有效时刻（不是跳过）
- [x] **M2 存储**：SQLite 迁移、Event/Reminder/Delivery/Ack 仓储，内存库测试 + 真实文件 WAL 重开测试
- [x] **M3 本机 API**：axum，`POST /v1/events`、`GET /v1/events`、`POST /v1/events/{id}/ack`、
      `POST /v1/reminders`、`GET /v1/health`；token 鉴权；集成测试。契约见 `docs/API.md`
- [x] **M4 CLI + Claude Code 集成**：`magpie notify|remind|list|ack`；`docs/INTEGRATIONS.md` 给出
      Claude Code `Stop`/`Notification` hook 配方、定时任务 prompt 片段
- [ ] **M5 渠道**：`Channel` trait；Telegram（带 确认/稍后 按钮）、ntfy、webhook；HTTP 层可注入以便测试
- [ ] **M6 Tauri 外壳**：托盘、单实例、自启动、运行时（调度 tick + API server + 投递 worker）、Windows toast
- [ ] **M7 界面**：收件箱、事件详情（ack/snooze/回复）、提醒编辑、渠道设置
- [ ] **M8 桌宠**：透明置顶窗口、喜鹊形象、气泡、idle/alert/sleep 状态、拖动、穿透点击
- [ ] **M9 手机双向**：Telegram getUpdates 长轮询，按钮回调 → Ack，文字回复 → 回传来源 callback
- [ ] **M10 critical 升级**：紧急 toast + 全屏遮罩 + 声音，直到确认
- [ ] **M11 交付**：NSIS 安装包、GitHub Actions（fmt/clippy/test/build）、版本号与 CHANGELOG
- [ ] **M12 调研**：能否直接读取 Claude 桌面定时任务的运行结果（本地会话文件）做自动接入

## 这次不做（及理由）

- 管理员提权：不需要，见 ARCHITECTURE「权限高」。
- 自建推送服务器：Telegram / ntfy 已够用，自建要运维。
- 跨平台打包：先只保证 Windows；代码不写死 Windows，但不在 mac/linux 上验证。
- 自动代发社媒：Magpie 只提醒和回传，发布动作永远由人确认。

## 等用户

（暂无）

## 已知问题

- **Magpie 没开时 hook 要等约 2.2 秒**（2026-09-28 实测）：Windows 连本机未监听端口不是立即拒绝，
  会重试 SYN。每轮 Claude 结束会被拖 2 秒。候选方案：hook 先看 `endpoint.json` 里记录的 app PID 是否存活，
  不在就直接跳过（M6 写 endpoint 时顺带记 PID）。
- **Claude Code hook 配方还没在真实会话里跑过**：映射逻辑和退出码都按官方文档 + 模拟 stdin 验证了，
  但 `settings.json` 那段配置本身要等 app 能常驻后再装上实测（不能在没有接收端时装，否则每轮都等 2 秒）。

## 进度日志

- 2026-09-28 立项：名字、架构、路线图。
- 2026-09-28 M0：workspace + core 领域类型，fmt/clippy/test 全绿（9 测试）。
- 2026-09-28 M1：Schedule/EscalationPolicy/select_channels，34 测试。DST 行为最初假设错了，以实测为准改了测试。
- 2026-09-28 M2：magpie-store（rusqlite bundled、user_version 迁移、WAL），store 12 + core 38 测试。
- 2026-09-28 M3：magpie-server（axum 0.8），11 个 API 测试含真实回环 socket；413/401/404/422 均实测。
- 2026-09-28 M4：magpie-config（凭据管理器实测可用）+ magpie-cli + INTEGRATIONS.md。真跑二进制发现并修了 2 个单测漏掉的 bug（clap 组 id、hook 退出码 2）。
