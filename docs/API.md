# Magpie 本机 API（v1）

- 地址：`http://127.0.0.1:<port>`，只监听回环，不能改成对外。
- 鉴权：除 `/v1/health` 外都要 `Authorization: Bearer <token>`。
- 请求体上限 128 KB；时间一律 RFC 3339 UTC；事件的 `created_at` 以服务端时钟为准。
- 错误统一为 `{"error":{"code":"unauthorized|not_found|invalid|internal","message":"…"}}`。
  `internal` 的细节只进日志，不返回给调用方。

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/v1/health` | `{"status":"ok","version":"…"}`，无需 token |
| POST | `/v1/events` | 投递事件，返回 201 + 事件 |
| GET | `/v1/events?open=true&limit=100&before=<时间>` | 新的在前；`before` 传上一页最后一条的 `created_at` 翻页；`limit` 1–500 |
| GET | `/v1/events/{id}` | 事件 + `closed_at` + `latest_ack` + `deliveries` |
| POST | `/v1/events/{id}/ack` | 回应，返回 204 |
| POST | `/v1/reminders` | 新建提醒，返回 201；模板在创建时就校验 |
| GET | `/v1/reminders` | 全部提醒 |
| DELETE | `/v1/reminders/{id}` | 删除，返回 204；不存在返回 404 |

## 事件

```json
{"source": "claude-code", "title": "备货完成", "body": "5 条已推送",
 "severity": "urgent", "tags": ["social"]}
```

`severity`：`info`（只进收件箱）· `normal`（提醒一次，默认）· `urgent`（没回应就追，会上手机）·
`critical`（全屏 + 所有渠道）。`title` 1–200 字，`body` ≤ 20000 字，`source` 1–100 字（按字符数，不是字节）。

## 回应

```json
{"kind": "acknowledged", "via": "cli"}
{"kind": "snoozed", "until": "2026-09-28T13:00:00Z", "via": "cli"}
{"kind": "replied", "text": "发", "via": "telegram"}
{"kind": "dismissed", "via": "pet"}
```

除 `snoozed` 外都是最终回应，事件关闭（以第一次最终回应为准）。`snoozed` 的 `until` 必须在未来。

## 提醒

```json
{"schedule": {"kind": "cron", "expr": "50 19 * * 1,3,5"},
 "template": {"source": "reminder", "title": "发小红书", "severity": "urgent"},
 "enabled": true}
```

`schedule.kind`：`once`（`at`）· `interval`（`anchor` + `every_secs` ≥ 60）· `cron`（5 段，按本机时区的墙钟时间）。
错过的触发（关机、休眠）合并成一次，不会补发一串。
