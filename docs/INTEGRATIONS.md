# 接入 Magpie

任何能跑命令或发 HTTP 的东西都能接入。命令行 `magpie` 自动从 `%APPDATA%\magpie\endpoint.json` 找端口、
从 Windows 凭据管理器取 token；也可以用环境变量 `MAGPIE_URL` / `MAGPIE_TOKEN` 覆盖。HTTP 契约见 [API.md](API.md)。

## 命令行速查

```bash
magpie notify "备货完成" -b "5 条已推送" -s urgent -t social
some-command | magpie notify "日志" -b -          # 正文从 stdin 读
magpie remind "发小红书" -s urgent --cron "50 19 * * 1,3,5"
magpie remind "喝水" --in 30m
magpie remind "睡觉" --at 23:30                    # 今天已过就是明天
magpie list                                         # 未处理的
magpie list --all --limit 50
magpie list --reminders
magpie ack 6bbe86                                   # id 前缀即可
magpie ack 6bbe86 --snooze 30m | --reply 发 | --dismiss
magpie ask "要推送吗？" --timeout 10m               # 等你回答，答案打到 stdout
magpie unremind <reminder-id>
magpie --json list                                  # 给脚本用
```

退出码：0 成功 · 1 出错 · 2 参数错误 · 3 连不上 Magpie（没开）；`ask` 另有 4 被忽略 · 5 超时。

## 让 AI 问你、等你回答（`magpie ask`）

AI 或脚本需要你拍板时，用 `ask` 代替 `notify`：它发一条 urgent 事件（会升级、会上手机），然后**阻塞等你回答**，
把答案打印到 stdout。你在桌面、桌宠、Telegram 任何一处回应都算。

```bash
answer=$(magpie ask "推送 5 条小红书到手机？" -b "下周素材已备好，见 outbox" --timeout 30m)
case $? in
  0) echo "同意：$answer" ;;      # 确认 → "acknowledged"；回复 → 你写的原话，比如「发」
  4) echo "被忽略了" ;;
  5) echo "30 分钟没人回答（事件还开着，之后回答也会记下）" ;;
esac
```

给 Claude 的写法（放进 prompt 或 CLAUDE.md）：

```text
需要我决定的事，不要自己猜：运行 `magpie ask "<一句话问题>" -b "<背景>" --timeout 10m`，
按 stdout 的回答继续；退出码 4 表示我拒绝，5 表示我没空回答，这两种都停下来别继续。
```

注意 Claude Code 的 Bash 工具单次最长 10 分钟，`--timeout` 不要超过它。

为什么是「拉」不是「推」：调用方跑一条命令读 stdout 就行，不用开端口；Magpie 也永远不会往事件里带的地址发请求。

## Claude Code

在 `~/.claude/settings.json`（全局）或项目的 `.claude/settings.json` 里加：

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "magpie", "args": ["hook", "claude-code"], "timeout": 10 }] }
    ],
    "Notification": [
      { "hooks": [{ "type": "command", "command": "magpie", "args": ["hook", "claude-code"], "timeout": 10 }] }
    ]
  }
}
```

- 用 `args`（exec 形式）而不是把整条命令写进 `command`：不经过 shell，Windows 上不会被 PowerShell/bash 的引号规则嚼坏。
- `magpie` 要在 PATH 里，或者写绝对路径。
- 映射规则：

  | Claude Code 事件 | Magpie 事件 |
  |---|---|
  | `Stop` | normal「Claude 完成了 · <项目>」，正文是最后一条回复（截到 1500 字） |
  | `Stop` 且 `stop_hook_active` | 不发（Claude 在被 hook 要求继续，这轮没结束） |
  | `Notification`：`permission_prompt` / `elicitation_*` / `agent_needs_input` | **urgent**「Claude 在等你」—— 没回应会一直追 |
  | `Notification`：`idle_prompt` | normal「Claude 空闲了」 |
  | 其它 `Notification` | info（只进收件箱） |

- **`magpie hook` 永远退出 0**。原因：Stop hook 退出码 2 会阻止 Claude 结束这一轮。
  所以连不上 Magpie、stdin 不是 JSON、参数写错，都只在 stderr 说一声，不影响 Claude。

## Claude 桌面定时任务

在定时任务 prompt 的末尾加一段：

```text
最后一步：用 magpie 通知我结果（不要跳过）。
- 需要我处理的事：magpie notify "<一句话结论>" -b "<要我做什么>" -s urgent --source scheduled:<任务 id>
- 一切正常：magpie notify "<一句话结论>" -s normal --source scheduled:<任务 id>
```

`--source scheduled:<任务 id>` 让路由规则可以按 `source_prefix = "scheduled:"` 把定时任务单独分流（比如直接上手机）。

## 脚本 / 其它语言

```bash
curl -s -X POST http://127.0.0.1:<port>/v1/events \
  -H "Authorization: Bearer $MAGPIE_TOKEN" -H "Content-Type: application/json" \
  -d '{"source":"backup","title":"备份失败","severity":"critical"}'
```
