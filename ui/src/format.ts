import type { Schedule, Severity } from "./types";

export const severityLabel: Record<Severity, string> = {
  info: "记录",
  normal: "普通",
  urgent: "紧急",
  critical: "严重",
};

const pad = (n: number) => String(n).padStart(2, "0");

/** Local time: "09-28 20:05" (or "20:05" for today). */
export function shortTime(iso: string): string {
  const d = new Date(iso);
  const now = new Date();
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return d.toDateString() === now.toDateString() ? hm : `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${hm}`;
}

export function describeSchedule(s: Schedule): string {
  switch (s.kind) {
    case "once":
      return `一次 · ${shortTime(s.at)}`;
    case "interval": {
      const m = s.every_secs / 60;
      return m % 60 === 0 ? `每 ${m / 60} 小时` : `每 ${m} 分钟`;
    }
    case "cron":
      return `cron · ${s.expr}`;
  }
}

/** Local "YYYY-MM-DDTHH:MM" (datetime-local input) -> UTC ISO string. */
export function localInputToIso(value: string): string {
  return new Date(value).toISOString();
}

/** Local calendar day as YYYY-MM-DD, `offset` days from today. */
export function localDay(offset = 0): string {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

const WEEKDAYS = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

/** "今天" / "明天" / "10-02 周五". */
export function dayLabel(day: string): string {
  if (day === localDay(0)) return "今天";
  if (day === localDay(1)) return "明天";
  if (day === localDay(-1)) return "昨天";
  const d = new Date(`${day}T00:00:00`);
  return `${day.slice(5)} ${WEEKDAYS[d.getDay()]}`;
}

export function minutesLabel(m: number | null | undefined): string {
  if (m == null) return "—";
  return m >= 60 ? `${Math.floor(m / 60)}h${m % 60 ? `${m % 60}m` : ""}` : `${m}m`;
}

export const reasonLabel: Record<string, string> = {
  fixed_time: "固定时间",
  deadline: "到截止日",
  blocking: "卡着别的事",
  long_deferred: "拖了好几次",
  overdue: "逾期",
  scheduled: "原定",
  spare_time: "顺手做",
};
