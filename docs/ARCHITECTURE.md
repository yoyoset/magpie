# Magpie 架构

> Magpie（喜鹊）—— 喜鹊报信。桌面常驻的 AI 结果信使：把 AI 助手、定时任务、脚本产生的结果
> 收进一个收件箱，按规则提醒到人（桌面、桌宠气泡、手机），并把人的回应（确认/稍后/回复）送回去。

## 要解决的问题

AI 在后台干完活（定时任务、长任务、需要人拍板的节点），结果散落在各个会话里，人不知道、
也没人追着提醒。Magpie 负责三件事，也只负责这三件：

1. **收**：任何来源都能用一条 HTTP 请求 / 一条 CLI 命令把事件投进来。
2. **提醒到位**：按严重度和规则投递；没确认就升级（重复、换渠道、全屏），直到有人处理。
3. **回传**：人在桌面或手机上的回应（ack / snooze / 文字回复）记录下来，并可回调给来源。

## 分层（依赖只能向下）

```
 app (Tauri shell: tray / 窗口 / 桌宠 / 系统通知 / 自启动)     ui/ (React + TS)
   │
   ├── magpie-server   本机 HTTP API (axum, 127.0.0.1 + bearer token)
   ├── magpie-channels 投递渠道: Telegram / ntfy / webhook（桌面通知在 app 层实现同一 trait）
   ├── magpie-store    SQLite 持久化 (rusqlite, 内嵌迁移)
   └── magpie-core     纯领域逻辑：事件、提醒、调度、升级策略、路由。无 IO、无 async
 magpie-cli            命令行客户端，只依赖 core 的类型 + HTTP，供 Claude Code hooks/脚本调用
```

- **core 不做 IO**：时间由调用方传入（`now: DateTime<Utc>`），所有调度/升级判断都是纯函数，可穷举测试。
- **一个 crate 一个职责**：换存储不碰调度，加渠道不碰存储。
- **渠道是插件**：`trait Channel { fn id(); async fn deliver(&Delivery) -> Result<Receipt> }`，新增渠道只加一个实现 + 一个配置段。

## 核心概念

| 概念 | 含义 |
|---|---|
| `Event` | 一条进收件箱的东西：来源、标题、正文、严重度(info/normal/urgent/critical)、可选动作、可选回调 URL |
| `Reminder` | 按时间触发的事件模板：一次性 / cron / 间隔 |
| `EscalationPolicy` | 未确认时怎么升级：隔多久重提、第几次换到手机、上限多少次 |
| `Route` | 按来源/严重度/标签决定投哪些渠道 |
| `Delivery` | 一次投递尝试及结果，可审计 |
| `Ack` | 人的回应：acknowledged / snoozed(until) / replied(text) / dismissed |

## 配置与密钥

- 配置：`%APPDATA%/magpie/config.toml`，单一来源；UI 改的也是这个文件。
- 密钥（Telegram bot token、ntfy token、API token）：Windows 凭据管理器（`keyring` crate），**不落明文**。
- 数据：`%APPDATA%/magpie/magpie.db`。

## "权限高"的具体含义

做：开机自启、单实例、桌宠置顶、critical 级别用 Windows 紧急 toast（穿透专注助手）+ 全屏提醒遮罩、
手机推送。
**不做**：管理员提权（提醒软件不需要，提权只会扩大被攻击面）、键盘鼠标钩子、读其它进程内存。

## 安全边界

- HTTP API 只绑 `127.0.0.1`，必须带 bearer token（首次运行生成，存凭据管理器，CLI 从同处读）。
- 回调 URL 只允许用户在配置里登记过的前缀，防止事件内容诱导 Magpie 往任意地址发数据。
- 手机端回复只接受配置里登记的 Telegram chat id。
