# Magpie 架构

> Magpie（喜鹊）—— 喜鹊报信。一开始是「AI 结果信使」，2026-09-28 起扩成「监工型个人助理」：
> 收下 AI/脚本的结果和人要做的事，按时间预算排好每天，到点提醒、事后追问、每晚收尾。

## 它负责的四件事

1. **收**：任何来源用一条 HTTP 请求、一条 CLI 命令或 MCP 工具，把「发生了什么」（事件）或
   「要做什么」（待办/点子）投进来。对话里冒出来的事由 Claude 经 MCP 当场记下。
2. **排**：按每天的时间预算为某天出排期草案，排不下的给出取舍理由；用时间账校准估时。
3. **提醒到位**：事件按严重度升级（重复 → 手机 → 全屏）；待办到点提醒、事后问「做完了吗」、早上简报。
4. **收尾与回传**：每晚对账、顺延、出明天草案；人的回应（确认/稍后/回复）记录并可经 `magpie ask` 交还给 AI。

Magpie **不内置大模型**。理解自然语言、取舍判断的「大脑」是用户已有的 Claude 订阅，经 MCP 读写 Magpie。

## 分层（依赖只能向下）

```
 app/  Tauri 2 外壳：托盘、收件箱/今天/点子池窗口、桌宠、全屏接管、toast、Telegram 回传轮询
 ui/   React + TS：同一份 bundle 三个窗口（默认 / ?view=pet / ?view=takeover）
   │
   ├── magpie-runtime   每 tick：触发提醒 → 收尾（21:00）→ 跟进（简报/到点/做完了吗）→ 事件升级投递
   ├── magpie-server    ops 层（唯一的业务入口）+ 本机 HTTP API（axum，127.0.0.1 + bearer token）
   ├── magpie-channels  Telegram（双向）/ ntfy / webhook，HTTP 可注入以便测试
   ├── magpie-store     SQLite：events / deliveries / acks / reminders / items / followups，user_version 迁移
   ├── magpie-config    路径、config.toml（全表 deny_unknown_fields）、endpoint.json、凭据管理器
   └── magpie-core      纯领域逻辑：无 IO、不读系统时钟（now/today/时区都由参数传入）
 magpie-cli            `magpie` 命令：notify/ask/remind/today/todo/idea/plan/hook/mcp
```

- **ops 层是唯一业务入口**：HTTP 路由、Tauri 命令、（经 HTTP 的）CLI 和 MCP 都是它的薄适配器，校验只写一次。
- **core 全是纯函数**：`Schedule`、`EscalationPolicy`、`select_channels`、`ItemChange::apply`、
  `plan_day`、`Calibration`、`follow_ups`、`review_day/review_week`——全部可穷举测试。
- **一个 crate 一个职责，一个模块一个概念**；新渠道 = 新文件实现 `Channel`。

## 核心概念

| 概念 | 含义 |
|---|---|
| `Event` | 「发生了什么，看一眼」：来源、标题、正文、严重度 info/normal/urgent/critical |
| `Ack` | 人对事件的回应：确认 / 稍后(until) / 回复(text) / 忽略；首个最终回应关闭事件 |
| `EscalationPolicy` | 未回应时：隔多久重提、第几次上手机、最多几轮；critical 直接全屏 |
| `Route` / `Reach` | 路由规则选渠道；Local 总可用、Remote 从 Phone 档起、Takeover 只在 critical |
| `Item` | 「你要做的事」：点子/待办、why、action、来源、领域、日期、时间、估时/实际、blocking、推迟次数 |
| `ItemChange` | 待办的全部状态变迁：done(实际用时)/schedule/defer/unschedule/estimate/drop/reopen |
| `Plan` | 某天的排期草案：排进的（带理由）、挪走的（带原因和日期）、催办、是否超预算 |
| `Calibration` | 时间账：按领域取 实际/估计 的中位数（≥3 样本，0.5–3 倍）校正估时 |
| `FollowUp` | 助理主动说的话：早报、到点提醒(urgent)、做完了吗；带去重键落库 |
| `Reminder` | 按 cron/间隔/一次性触发的事件模板 |

## 一天是怎么转的

| 时间 | 谁 | 做什么 |
|---|---|---|
| 随时 | 用户 ↔ Claude（MCP） | 对话里出现要做的事/点子 → `add_todo` / `add_idea` 当场记 |
| 09:00 | runtime | 早报：今天几件、约多少分钟、逾期几件 |
| 到点前 5 分钟 | runtime | urgent 提醒（没回应会升级到手机） |
| 到点 + max(估时, 30 分) | runtime | 「做完了吗」 |
| 21:00 | runtime | 收尾：对账、时间账、今天没做完的顺延到明天、出明天草案、发收尾提醒 |
| 21:10 | Claude 定时任务 | 取舍/拆分拖延项，`magpie ask` 问用户，同意后 `plan apply` |
| 周日 21:00 | runtime | 周复盘：每天完成数、估时差最多的、一直在拖的、新点子数 |

## 排期规则（`plan_day`）

1. 固定时间、到截止日的：必排（超预算也排，并标 over_capacity）
2. 卡着别的事的、推迟 ≥2 次的（防饿死）
3. 逾期的（只有**已经过去的日子**才算逾期；今天还没做完的不会被明天的草案拉走）
4. 原定那天的
5. 未排期的：只在有剩余时间时顺手拉进来

同层内：截止早 → 推迟多 → 估时短。大的放不下就跳过、让后面小的填空。
推迟 ≥3 次加一句「拆小还是放弃」。点子池永远不自动排。

## 配置与密钥

- 配置：`%APPDATA%/magpie/config.toml`（`MAGPIE_HOME` 可覆盖整个数据目录）。拼错的键启动报错（故意）。
- `[planning]`：`daily_minutes`（180）、`default_estimate_min`（30）、`briefing_at`（09:00）、`review_at`（21:00）。
- 密钥（API token、Telegram/ntfy/webhook）：Windows 凭据管理器，不落明文；界面只允许设置白名单里的键。
- 数据：`%APPDATA%/magpie/magpie.db`（WAL）。客户端经 `endpoint.json` 找端口。

## 「权限高」的具体含义

做：开机自启（仅 release 构建）、单实例、桌宠置顶、critical 全屏接管 + 提示音、手机推送与回传。
**不做**：管理员提权、键鼠钩子、读其它进程内存、读 Claude 的会话文件（见 RESEARCH-scheduled-tasks.md）。

## 安全边界

- HTTP API 只绑 `127.0.0.1`，必须带 token（常量时间比较，空 token 永不匹配）。
- Magpie 从不向事件内容里带的地址发请求（回传用 `magpie ask` 拉模型）。
- Telegram 只接受配置里的 chat id；按钮/回复经 ops 层写入，来源记为 `telegram`。
- Telegram token 在 URL 里：reqwest 错误用 `without_url()` 去掉，有变异测试守着。
