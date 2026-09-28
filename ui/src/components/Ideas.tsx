import { useState } from "react";
import { api, errorText } from "../api";
import { localDay, shortTime } from "../format";
import { usePolling } from "../usePolling";
import { QuickAdd } from "./QuickAdd";

/** The idea pool: capture freely, decide later. Nothing here is on a schedule. */
export function Ideas() {
  const { data: ideas, error, reload } = usePolling(() => api.listItems({ kind: "idea", status: "open" }), 5000);
  const [actionError, setActionError] = useState<string | null>(null);

  const act = (p: Promise<unknown>) =>
    void p.then(() => {
      setActionError(null);
      void reload();
    }).catch((e) => setActionError(errorText(e)));

  return (
    <div className="page">
      <h1>点子池</h1>
      <p className="hint">想到就丢进来，不用马上决定。每晚收尾时挑值得做的排进某一天，不做的就划掉。</p>
      <QuickAdd onAdded={() => void reload()} defaultKind="idea" />
      {error && <p className="banner error">读取失败：{error}</p>}
      {actionError && <p className="banner error">{actionError}</p>}
      {ideas && ideas.length === 0 && <p className="empty">池子是空的。</p>}
      <ul className="reminder-list">
        {ideas?.map((i) => (
          <li key={i.id} className="card idea">
            <div>
              <div className="event-title">{i.title}</div>
              {i.why && <div className="idea-why">{i.why}</div>}
              <div className="event-meta">
                {i.area && `${i.area} · `}
                {i.source && `来自 ${i.source} · `}
                {shortTime(i.created_at)}
              </div>
            </div>
            <div className="idea-actions">
              <button onClick={() => act(api.changeItem(i.id, { op: "schedule", day: localDay(1) }))}>排到明天</button>
              <button className="ghost" onClick={() => act(api.changeItem(i.id, { op: "drop" }))}>
                不做了
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
