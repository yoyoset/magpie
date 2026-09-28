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
| POST | `/v1/items` | 新建待办/点子（`ItemDraft`），201 |
| GET | `/v1/items?status=&kind=&day=\|before=\|from=&to=\|unscheduled=true&limit=` | 日期筛选四选一，混用 422 |
| GET | `/v1/items/{id}` | 单条 |
| PATCH | `/v1/items/{id}` | 变更（`ItemChange`），返回新状态 |
| GET | `/v1/plan?day=YYYY-MM-DD` | 排期草案，不改任何东西 |
| POST | `/v1/plan/apply` | `{"day":"…"}`：服务端重算并应用（不接受客户端传来的计划） |

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

## 待办与点子

```json
{"kind": "todo", "title": "定价系数拍板", "why": "服务页唯一硬闸门", "action": "看 P1-P4 选一组",
 "source": "viiyd 网站", "area": "工作", "day": "2026-09-29", "at": "20:00:00",
 "deadline": "2026-10-01", "estimate_min": 45, "blocking": true}
```

`kind`：`todo`（默认）/ `idea`（点子不能带 day/at）。`day`/`deadline` 是本地日期，`at` 是本地时刻。
未知字段一律 422（AI 写入的输入要严格）。

变更（PATCH 体）：

```json
{"op": "done", "actual_min": 40}
{"op": "schedule", "day": "2026-09-30", "at": "10:00:00"}
{"op": "defer", "to": "2026-09-30"}
{"op": "unschedule"} · {"op": "estimate", "minutes": 30} · {"op": "drop"} · {"op": "reopen"}
```

已完成/已放弃的必须先 `reopen` 才能再改；`defer` 必须挪到更晚的日子（推迟次数 +1）。

## 排期草案

```json
{"day": "2026-09-29", "capacity_min": 180, "used_min": 80, "over_capacity": false,
 "planned": [{"item": "…", "title": "定价系数拍板", "minutes": 45, "reason": "blocking"}],
 "pushed":  [{"item": "…", "title": "整理 R2 旧图", "to": "2026-09-30",
              "why": "放不下：这天只剩 105 分钟，它要 120 分钟，也不比已排的急"}],
 "nudges":  [{"item": "…", "title": "…", "text": "已经推迟 3 次了：拆成更小的一步，或者干脆放弃？"}]}
```

`reason`：`fixed_time` · `deadline` · `blocking` · `long_deferred` · `overdue` · `scheduled` · `spare_time`。
