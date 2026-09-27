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
