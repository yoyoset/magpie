import { useState } from "react";
import { api, errorText } from "../api";
import { dayLabel, localDay, minutesLabel } from "../format";
import type { Item } from "../types";

/** One todo: check off (asking how long it took), push to tomorrow, or open details. */
export function ItemRow({ item, onChanged, showDay = false }: { item: Item; onChanged: () => void; showDay?: boolean }) {
  const [finishing, setFinishing] = useState(false);
  const [took, setTook] = useState(String(item.estimate_min ?? ""));
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const overdue = item.status === "open" && item.day !== null && item.day < localDay(0);
  const done = item.status === "done";

  async function run(p: Promise<unknown>) {
    try {
      await p;
      setError(null);
      setFinishing(false);
      onChanged();
    } catch (e) {
      setError(errorText(e));
    }
  }

  const minutes = Number.parseInt(took, 10);
  return (
    <li className={`item-row ${done ? "done" : ""} ${overdue ? "overdue" : ""}`}>
      <div className="item-main">
        <button
          className={`check ${done ? "on" : ""}`}
          aria-label={done ? "重新打开" : "完成"}
          onClick={() => (done ? void run(api.changeItem(item.id, { op: "reopen" })) : setFinishing((f) => !f))}
        >
          {done ? "✓" : ""}
        </button>
        <div className="item-text" onClick={() => setOpen((o) => !o)}>
          <div className="item-title">
            {item.at && <span className="item-time">{item.at.slice(0, 5)}</span>}
            {item.title}
          </div>
          <div className="event-meta">
            {showDay && item.day && `${dayLabel(item.day)} · `}
            {minutesLabel(item.estimate_min)}
            {done && item.actual_min != null && ` → 实际 ${minutesLabel(item.actual_min)}`}
            {item.area && ` · ${item.area}`}
            {item.blocking && " · ⚑ 卡着别的事"}
            {overdue && " · 逾期"}
            {item.deferrals > 0 && ` · 推迟过 ${item.deferrals} 次`}
          </div>
        </div>
        {!done && (
          <button className="ghost small" title="挪到明天" onClick={() => void run(api.changeItem(item.id, { op: "defer", to: localDay(1) > (item.day ?? "") ? localDay(1) : localDay(2) }))}>
            明天
          </button>
        )}
      </div>

      {finishing && (
        <div className="finish">
          <span>用了多久？</span>
          <input type="number" min={1} value={took} onChange={(e) => setTook(e.target.value)} placeholder="分钟" autoFocus />
          <span>分钟</span>
          <button className="primary small" onClick={() => void run(api.changeItem(item.id, { op: "done", actual_min: minutes > 0 ? minutes : null }))}>
            完成
          </button>
          <button className="ghost small" onClick={() => void run(api.changeItem(item.id, { op: "done", actual_min: null }))}>
            不记时间
          </button>
        </div>
      )}

      {open && (item.why || item.action || item.source) && (
        <dl className="item-detail">
          {item.why && (
            <>
              <dt>为什么</dt>
              <dd>{item.why}</dd>
            </>
          )}
          {item.action && (
            <>
              <dt>你要做的</dt>
              <dd>{item.action}</dd>
            </>
          )}
          {item.source && (
            <>
              <dt>来自</dt>
              <dd>{item.source}</dd>
            </>
          )}
        </dl>
      )}
      {error && <p className="banner error">{error}</p>}
    </li>
  );
}
