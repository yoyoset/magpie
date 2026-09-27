import { useState } from "react";
import { api } from "../api";
import { severityLabel, shortTime } from "../format";
import { usePolling } from "../usePolling";
import { EventDetailPane } from "./EventDetail";

export function Inbox() {
  const [openOnly, setOpenOnly] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const { data: events, error, reload } = usePolling(() => api.listEvents(openOnly), 2000, [openOnly]);

  return (
    <div className="split">
      <section className="list-pane">
        <header className="pane-header">
          <h1>收件箱</h1>
          <div className="segmented">
            <button className={openOnly ? "on" : ""} onClick={() => setOpenOnly(true)}>
              未处理
            </button>
            <button className={!openOnly ? "on" : ""} onClick={() => setOpenOnly(false)}>
              全部
            </button>
          </div>
        </header>
        {error && <p className="banner error">读取失败：{error}</p>}
        {events && events.length === 0 && <p className="empty">{openOnly ? "没有待处理的事。" : "收件箱是空的。"}</p>}
        <ul className="event-list">
          {events?.map((e) => (
            <li
              key={e.id}
              className={`event-row sev-${e.severity} ${e.closed_at ? "closed" : ""} ${selected === e.id ? "selected" : ""}`}
              onClick={() => setSelected(e.id)}
            >
              <span className="sev-dot" title={severityLabel[e.severity]} />
              <div className="event-main">
                <div className="event-title">{e.title}</div>
                <div className="event-meta">
                  {e.source} · {shortTime(e.created_at)}
                  {e.closed_at && " · 已处理"}
                </div>
              </div>
            </li>
          ))}
        </ul>
      </section>
      <section className="detail-pane">
        {selected ? (
          <EventDetailPane id={selected} onChanged={reload} />
        ) : (
          <p className="empty">选一条看详情。</p>
        )}
      </section>
    </div>
  );
}
