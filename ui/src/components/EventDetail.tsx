import { useState } from "react";
import { api, errorText } from "../api";
import { severityLabel, shortTime } from "../format";
import type { AckKind } from "../types";
import { usePolling } from "../usePolling";

const SNOOZES = [
  { label: "30 分钟后", minutes: 30 },
  { label: "2 小时后", minutes: 120 },
  { label: "明早", minutes: null },
] as const;

function tomorrowMorning(): Date {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(9, 0, 0, 0);
  return d;
}

function ackLabel(kind: AckKind): string {
  switch (kind.kind) {
    case "acknowledged":
      return "已确认";
    case "dismissed":
      return "已忽略";
    case "replied":
      return `已回复：${kind.text}`;
    case "snoozed":
      return `稍后提醒（${shortTime(kind.until)}）`;
  }
}

export function EventDetailPane({ id, onChanged }: { id: string; onChanged: () => void }) {
  const { data: e, error, reload } = usePolling(() => api.eventDetail(id), 3000, [id]);
  const [reply, setReply] = useState("");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  async function respond(kind: AckKind) {
    setBusy(true);
    setActionError(null);
    try {
      await api.ack(id, kind);
      setReply("");
      await reload();
      onChanged();
    } catch (err) {
      setActionError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  if (error) return <p className="banner error">读取失败：{error}</p>;
  if (!e) return <p className="empty">加载中…</p>;

  const failed = e.deliveries.filter((d) => d.outcome.status === "failed");
  const open = !e.closed_at;

  return (
    <article className="detail">
      <div className={`sev-badge sev-${e.severity}`}>{severityLabel[e.severity]}</div>
      <h2>{e.title}</h2>
      <p className="event-meta">
        {e.source} · {shortTime(e.created_at)}
        {e.tags.length > 0 && ` · ${e.tags.join(" / ")}`}
      </p>
      {e.body && <pre className="body">{e.body}</pre>}

      {e.latest_ack && (
        <p className="ack-state">
          {ackLabel(e.latest_ack.kind)} · 来自 {e.latest_ack.via} · {shortTime(e.latest_ack.at)}
        </p>
      )}

      {open && (
        <div className="actions">
          <button className="primary" disabled={busy} onClick={() => respond({ kind: "acknowledged" })}>
            确认
          </button>
          {SNOOZES.map((s) => (
            <button
              key={s.label}
              disabled={busy}
              onClick={() =>
                respond({
                  kind: "snoozed",
                  until: (s.minutes === null ? tomorrowMorning() : new Date(Date.now() + s.minutes * 60_000)).toISOString(),
                })
              }
            >
              {s.label}
            </button>
          ))}
          <button className="ghost" disabled={busy} onClick={() => respond({ kind: "dismissed" })}>
            忽略
          </button>
          <form
            className="reply"
            onSubmit={(ev) => {
              ev.preventDefault();
              if (reply.trim()) void respond({ kind: "replied", text: reply.trim() });
            }}
          >
            <input value={reply} onChange={(ev) => setReply(ev.target.value)} placeholder="回复一句（例如：发）" />
            <button type="submit" disabled={busy || !reply.trim()}>
              回复
            </button>
          </form>
        </div>
      )}
      {actionError && <p className="banner error">{actionError}</p>}

      <h3>投递记录</h3>
      {failed.length > 0 && <p className="banner error">{failed.length} 次投递失败，下面标红的是原因。</p>}
      {e.deliveries.length === 0 ? (
        <p className="empty small">还没有投递。</p>
      ) : (
        <table className="deliveries">
          <tbody>
            {e.deliveries.map((d, i) => (
              <tr key={i} className={d.outcome.status === "failed" ? "failed" : ""}>
                <td>{shortTime(d.at)}</td>
                <td>{d.channel}</td>
                <td>{d.tier}</td>
                <td>{d.outcome.status === "sent" ? "已送达" : d.outcome.error}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </article>
  );
}
