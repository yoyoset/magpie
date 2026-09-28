# Magpie 路线图

每轮 loop 只做**一个**里程碑（太大就拆成子项做一部分），做完的打 `[x]` 并在「进度日志」记一行。

## 每轮协议（loop 每次醒来照做）

1. 读本文件 + `CLAUDE.md`，找第一个未完成项。
2. 实现。核心逻辑写成纯函数并配单元测试。
3. `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` 全绿；
   动了前端再加 `npm run build`（在 `ui/`）。不绿不提交。
   **提交必须写成条件式**：`if cargo fmt --all -- --check && cargo clippy … && cargo test …; then git commit …; fi`。
   只「跑一下再 commit」栽过两次（852b50f 漏 clippy、8f72068 测试红着提交）。
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
- [x] **M5 渠道**：`Channel` trait；Telegram（带 确认/稍后 按钮）、ntfy、webhook；HTTP 层可注入以便测试
- [x] **M6a 运行时**：`magpie-runtime` 的 Dispatcher（触发提醒、推进升级、并发投递、失败入库）+ run 循环
- [x] **M6b 配置文件**：`config.toml` 模式（端口、升级策略、路由、渠道），读写 + 校验，放 `magpie-config`
- [x] **M6c Tauri 外壳**：托盘、单实例、自启动、Windows toast 渠道、接线（server + runtime）、endpoint.json 带 PID
- [x] **M7 界面**：收件箱、事件详情（ack/snooze/回复）、提醒编辑、渠道设置
- [x] **M8 桌宠**：透明置顶窗口、喜鹊形象、气泡、idle/alert/sleep 状态、拖动、穿透点击
- [x] **M9 手机双向**：Telegram getUpdates 长轮询，按钮回调 → Ack，引用回复 → Replied；用 `tools/mock_telegram.py` 实测全流程
- [x] **M9b 回传来源**：改为「拉」模型 `magpie ask`：发 urgent 事件并阻塞等回答，答案打到 stdout（退出码 0/4/5）。理由见「这次不做」
- [x] **M10 critical 升级**：全屏遮罩 + WebAudio 提示音 + 队列，直到确认；托盘/Alt+F4 可临时关
- [x] **M11 交付**：NSIS 安装包（含 CLI）实测装/卸干净；GitHub Actions 已写好待仓库建立后首跑；LICENSE、CHANGELOG
- [x] **M12 调研**：结论不做（会话 id 对不上文件、无定时任务标记字段、内部格式）；改用任务 prompt 里显式 `magpie notify`。
      附带：Stop hook 默认降为 info，避免每轮交互都弹。见 `docs/RESEARCH-scheduled-tasks.md`

## v0.2 助理（见 docs/VISION-assistant.md）

目标：用户每天打开就知道要干什么；对话里的点子和「需要你做的事」不再丢；每晚复盘排好第二天。
做完拉起来给用户真用。

- [x] **A1 待办与点子**：core `Item`（点子/待办、为什么、你要做什么、来源、领域、日期、时间、估时/实际、状态）；
      store 迁移 v2；ops + HTTP `/v1/items`；CLI `magpie todo|idea add/list/done/defer`；config `[planning] daily_minutes`
- [x] **A2 排期引擎（core 纯函数）**：给定待办、死线、固定安排、容量 → 明日计划 + 被挤掉的清单（每条带原因、改排日期）；
      估时按历史「实际/估计」比例校正
- [x] **A3 今天页**：UI 首页改「今天」：今日计划（时间块）、逾期、需要处理的事件、明天预览；点子池页；勾选/推迟/改估时
- [x] **A4 AI 接口（MCP）**：`magpie mcp`（stdio），工具：记待办/点子、查今天/明天、完成、推迟、生成明日草案、写回计划
- [x] **A5 跟进**：计划里的事到点提醒；过时未勾 → 「做了吗？挪到明天？」；早 9:00 今日简报
- [x] **A6 收尾会**：每晚 21:00 对账（完成数、时间账）→ 收点子 → 排明天 → `magpie ask` 确认；周日周复盘统计
- [x] **A7 拉起来用**：导入现有待办清单、登记现有定时任务（干嘛用/哪步要你）、装安装包并开机自启、
      接 Claude（MCP + 全局捕获规则，改用户配置前给他看改动）

## 这次不做（及理由）

- 管理员提权：不需要，见 ARCHITECTURE「权限高」。
- 自建推送服务器：Telegram / ntfy 已够用，自建要运维。
- 跨平台打包：先只保证 Windows；代码不写死 Windows，但不在 mac/linux 上验证。
- 自动代发社媒：Magpie 只提醒和回传，发布动作永远由人确认。
- 事件 `callback_url`（推模型）：主要调用方是终端里的 AI，`magpie ask` 读 stdout 更简单，调用方不用开端口，
  Magpie 也不必向事件内容里带的地址发请求（少一个 SSRF 面）。真有服务端需要推送时再加，届时必须白名单。

## 等用户

- **Telegram 真机验证**：需要 ① 找 @BotFather 建一个 bot 拿 token（或复用现有表单链路的 bot）② 给 bot 发一句话后拿到你的 chat id。
  代码和测试已完成，缺的只是真实投递那一步。token 会存进凭据管理器，不落文件。
- **建 GitHub 仓库**：建好后 `git remote add origin … && git push -u origin main`，CI 会首次运行；我没法替你建号/建库。
  打 `v0.1.0` tag 会自动出安装包草稿 release。
- **要不要给现有两个定时任务接上 Magpie**（`viiyd-weekly-social-prep`、`viiyd-monthly-marketing-review`）：
  在它们的 prompt 末尾加一句 `magpie notify …`。这是改你的定时任务配置，等你点头再动。
- **看一眼屏幕**：运行 `F:/my_ai/magpie/target/debug/magpie-app.exe`，再在终端跑 `magpie notify 测试`，
  确认右下角有「鹊」托盘图标、弹出了 Windows 通知。日志显示通知 API 返回成功，但未打包的调试版在 Windows 上
  通知可能显示成别的应用名或被系统吞掉，只有肉眼能确认。
- **按一次 Alt+F4**：出现全屏提醒时按 Alt+F4，确认它能关掉（代码走同一个隐藏逻辑，但自动化里按不了键）。
- **ntfy（可选）**：手机装 ntfy App 并订阅一个只有你知道的 topic 名。不装也行，Telegram 够用。

## 已知问题

- **卸载后注册表残留安装路径**（2026-09-28 实测）：NSIS 卸载不删 `HKCU\Softwareii\Magpie`（默认值=安装目录），
  再装会静默装回旧目录（当时装进了测试用的 `%TEMP%` 目录）。已手动删键重装到 `%LOCALAPPDATA%\Magpie`。
  要修：安装脚本加卸载钩子删这个键，或安装时忽略 temp 路径。

- toast 没做 Windows「urgent」场景（插件不支持），专注助手开着时 toast 可能被收起；critical 靠全屏窗口兜底，那是普通窗口不受专注助手影响。

- 桌宠窗口 300×320 的透明区域会挡住下面窗口的点击（没做逐像素穿透）。窗口已尽量小；要做穿透需按鼠标位置动态切 `set_ignore_cursor_events`。

- 界面改渠道/路由还得手改 `config.toml` 再重启；界面里设了密钥也要重启才生效（M7 只做了密钥写入）。
- 界面数据是 2 秒轮询，不是推送（有意为之，本机 SQLite 读很便宜）。

- ~~Magpie 没开时 hook 要等约 2.2 秒~~ 已修：连接阶段限 300ms，实测降到 0.45 秒（冷启动首次 1.2 秒）。
- **Claude Code hook 配方还没在真实会话里跑过**：映射逻辑和退出码都按官方文档 + 模拟 stdin 验证了，
  但 `settings.json` 那段配置本身要等 app 能常驻后再装上实测（不能在没有接收端时装，否则每轮都等 2 秒）。

## 进度日志

- 2026-09-28 立项：名字、架构、路线图。
- 2026-09-28 M0：workspace + core 领域类型，fmt/clippy/test 全绿（9 测试）。
- 2026-09-28 M1：Schedule/EscalationPolicy/select_channels，34 测试。DST 行为最初假设错了，以实测为准改了测试。
- 2026-09-28 M2：magpie-store（rusqlite bundled、user_version 迁移、WAL），store 12 + core 38 测试。
- 2026-09-28 M3：magpie-server（axum 0.8），11 个 API 测试含真实回环 socket；413/401/404/422 均实测。
- 2026-09-28 M4：magpie-config（凭据管理器实测可用）+ magpie-cli + INTEGRATIONS.md。真跑二进制发现并修了 2 个单测漏掉的 bug（clap 组 id、hook 退出码 2）。
- 2026-09-28 M5：magpie-channels（Telegram/ntfy/webhook），实测 reqwest 错误会带出含 token 的 URL，已用 without_url 去掉并有变异测试守着。真机投递等 bot token。
- 2026-09-28 M6a：magpie-runtime，7 个场景测试含真实 run 循环（通知后 2 秒内投递，实测整套 0.16 秒）。M6 拆成 a/b/c。
- 2026-09-28 M6b：config.toml 全表 deny_unknown_fields + 交叉校验（吸取 viiyd 配置静默失效三次的教训），108 测试。
- 2026-09-28 M6c：Tauri 外壳跑起来了。单实例、重启保数据、配置缺 token 进收件箱、坏配置拒绝启动均实测；屏幕可见性待用户确认。另修 hook 连接超时（2.2s→0.45s）。
- 2026-09-28 M7：抽出 ops 层（HTTP 与 Tauri 命令共用），React 界面。屏幕权限被拒，改用 WebView2 调试协议在真实 app 里点完全流程验证。
- 2026-09-28 M8：桌宠（SVG 喜鹊 + 三种情绪 + 气泡 + 计数角标 + pet 渠道），WebView2 截图与点击全流程验证。
- 2026-09-28 M9：Telegram 双向（按钮/引用回复/陌生人忽略/offset），对本地假 Bot API 跑通；真机仍等 token。
- 2026-09-28 M9b：`magpie ask`（回复/忽略/snooze 后确认/超时/JSON 五种情况两进程实测）。推模型不做，理由已记。
- 2026-09-28 M10：全屏接管。实测 WebView2 下 AudioContext 无需手势即 running，所以不用 MessageBeep/unsafe。Alt+F4 待人工确认。另：852b50f 只跑了测试漏跑 clippy，已补提交修正。
- 2026-09-28 M11：安装包 4.9MB，静默装/卸实测干净，CLI 随包；externalBin 放主配置会让普通 cargo build 失败，已拆到 release 配置。
- 2026-09-28 M12：调研结论写入 docs；Stop hook 默认 info。路线图 M0–M12 全部完成，剩下的都在「等用户」。
- 2026-09-28 转向 v0.2「监工型个人助理」，新增 A1–A7。
- 2026-09-28 A1：Item（core）+ items 表 v2（真实库副本 16/16 事件无损升级）+ /v1/items + `magpie today|todo|idea`。
- 2026-09-28 A2：plan_day（分层+装箱+理由+催办）+ 中位数校准 + /v1/plan + `magpie plan`；端到端草案/应用一致。
- 2026-09-28 A3：今天页 + 点子池。真机点测时发现排期把「今天还没做的」算逾期拉进明天，已修（planner 需要 today）。
- 2026-09-28 A4：`magpie mcp`（手写 stdio MCP，7 个工具），子进程真协议端到端跑通。尚未注册进用户的 Claude（A7 先问）。
- 2026-09-28 A5：早报/到点提醒(urgent)/做完了吗，去重键落库，运行时同 tick 投递；167 测试。
- 2026-09-28 A6：21:00 收尾（对账/时间账/顺延/明日草案）+ 周日周复盘，运行时实跑测试；Claude 夜间任务 prompt 写好待 A7 注册。
- 2026-09-28 A7：装到 %LOCALAPPDATA%\Magpie 并开机自启；数据先备份到 %APPDATA%\magpie-backup-20260928 再升级到 v3；导入 16 条（今晚 20:00 小红书等）；MCP 用户级注册 ✔ Connected；全局 CLAUDE.md 加捕获规则；建 21:10 收尾定时任务。v0.2 A1–A7 全部完成。
