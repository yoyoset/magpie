import { useState } from "react";
import { api, errorText } from "../api";
import { describeSchedule, localInputToIso, severityLabel, shortTime } from "../format";
import type { Schedule, Severity } from "../types";
import { usePolling } from "../usePolling";

type Kind = "once" | "interval" | "cron";

export function Reminders() {
  const { data: reminders, error, reload } = usePolling(api.listReminders, 5000);
  const [title, setTitle] = useState("");
  const [severity, setSeverity] = useState<Severity>("urgent");
  const [kind, setKind] = useState<Kind>("cron");
  const [at, setAt] = useState("");
  const [everyMinutes, setEveryMinutes] = useState(60);
  const [cron, setCron] = useState("50 19 * * 1,3,5");
  const [formError, setFormError] = useState<string | null>(null);

  async function create(ev: React.FormEvent) {
    ev.preventDefault();
    setFormError(null);
    let schedule: Schedule;
    if (kind === "once") {
      if (!at) return setFormError("选一个时间");
      schedule = { kind: "once", at: localInputToIso(at) };
    } else if (kind === "interval") {
      schedule = { kind: "interval", anchor: new Date().toISOString(), every_secs: everyMinutes * 60 };
    } else {
      schedule = { kind: "cron", expr: cron };
    }
    try {
      await api.createReminder(schedule, { source: "reminder", title, body: "", severity, tags: [] });
      setTitle("");
      await reload();
    } catch (e) {
      setFormError(errorText(e));
    }
  }

  async function remove(id: string) {
    try {
      await api.deleteReminder(id);
      await reload();
    } catch (e) {
      setFormError(errorText(e));
    }
  }

  return (
    <div className="page">
      <h1>提醒</h1>
      <form className="card form" onSubmit={create}>
        <input required value={title} onChange={(e) => setTitle(e.target.value)} placeholder="提醒什么（例如：发小红书）" />
        <div className="row">
          <select value={severity} onChange={(e) => setSeverity(e.target.value as Severity)}>
            {(["normal", "urgent", "critical"] as const).map((s) => (
              <option key={s} value={s}>
                {severityLabel[s]}
              </option>
            ))}
          </select>
          <select value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
            <option value="cron">按周期（cron）</option>
            <option value="once">一次</option>
            <option value="interval">每隔</option>
          </select>
          {kind === "once" && <input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} />}
          {kind === "interval" && (
            <label className="inline">
              <input
                type="number"
                min={1}
                value={everyMinutes}
                onChange={(e) => setEveryMinutes(Number(e.target.value))}
              />
              分钟
            </label>
          )}
          {kind === "cron" && <input value={cron} onChange={(e) => setCron(e.target.value)} title="分 时 日 月 周，本机时间" />}
          <button className="primary" type="submit">
            添加
          </button>
        </div>
        {kind === "cron" && <p className="hint">五段：分 时 日 月 周（本机时间）。例：<code>50 19 * * 1,3,5</code> = 周一三五 19:50。</p>}
        {formError && <p className="banner error">{formError}</p>}
      </form>

      {error && <p className="banner error">读取失败：{error}</p>}
      {reminders && reminders.length === 0 && <p className="empty">还没有提醒。</p>}
      <ul className="reminder-list">
        {reminders?.map((r) => (
          <li key={r.id} className="card reminder">
            <div>
              <div className="event-title">{r.template.title}</div>
              <div className="event-meta">
                {describeSchedule(r.schedule)} · {severityLabel[r.template.severity]}
                {r.next_fire ? ` · 下次 ${shortTime(r.next_fire)}` : " · 不再触发"}
              </div>
            </div>
            <button className="ghost" onClick={() => void remove(r.id)}>
              删除
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
