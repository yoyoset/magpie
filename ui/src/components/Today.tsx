import { useState } from "react";
import { api, errorText } from "../api";
import { dayLabel, localDay, minutesLabel, reasonLabel } from "../format";
import type { Item } from "../types";
import { usePolling } from "../usePolling";
import { ItemRow } from "./ItemRow";
import { QuickAdd } from "./QuickAdd";

interface TodayData {
  overdue: Item[];
  today: Item[];
  pending: number;
}

async function loadToday(): Promise<TodayData> {
  const today = localDay(0);
  const [overdue, todays, events] = await Promise.all([
    api.listItems({ status: "open", kind: "todo", before: today }),
    api.listItems({ kind: "todo", day: today }),
    api.listEvents(true, 200),
  ]);
  return {
    overdue,
    today: todays.filter((i) => i.status !== "dropped"),
    pending: events.filter((e) => e.severity === "urgent" || e.severity === "critical").length,
  };
}

export function Today({ goInbox }: { goInbox: () => void }) {
  const { data, error, reload } = usePolling(loadToday, 3000);
  const tomorrow = localDay(1);
  const plan = usePolling(() => api.planDay(tomorrow), 10_000, [tomorrow]);
  const [applyError, setApplyError] = useState<string | null>(null);

  const reloadAll = () => {
    void reload();
    void plan.reload();
  };

  if (error) return <div className="page"><p className="banner error">读取失败：{error}</p></div>;
  if (!data) return <div className="page"><p className="empty">加载中…</p></div>;

  const open = data.today.filter((i) => i.status === "open");
  const doneCount = data.today.length - open.length;
  const leftMin = open.reduce((s, i) => s + (i.estimate_min ?? 0), 0) + data.overdue.reduce((s, i) => s + (i.estimate_min ?? 0), 0);

  return (
    <div className="page today">
      <header className="today-head">
        <h1>
          今天 <span className="muted">{new Date().toLocaleDateString("zh-CN", { month: "long", day: "numeric", weekday: "long" })}</span>
        </h1>
        <p className="today-summary">
          完成 {doneCount}/{data.today.length} · 还剩约 {minutesLabel(leftMin)}
          {data.overdue.length > 0 && <span className="warn"> · 逾期 {data.overdue.length} 件</span>}
          {data.pending > 0 && (
            <button className="link warn" onClick={goInbox}>
              {" "}· {data.pending} 条提醒在等你
            </button>
          )}
        </p>
      </header>

      <QuickAdd onAdded={reloadAll} />

      {data.overdue.length > 0 && (
        <section>
          <h3>逾期</h3>
          <ul className="item-list">
            {data.overdue.map((i) => (
              <ItemRow key={i.id} item={i} onChanged={reloadAll} showDay />
            ))}
          </ul>
        </section>
      )}

      <section>
        <h3>今天</h3>
        {data.today.length === 0 ? (
          <p className="empty small">今天没排事。上面加一条，或者晚上排明天。</p>
        ) : (
          <ul className="item-list">
            {data.today.map((i) => (
              <ItemRow key={i.id} item={i} onChanged={reloadAll} />
            ))}
          </ul>
        )}
      </section>

      <section className="card tomorrow">
        <div className="row spread">
          <h3>明天草案 · {dayLabel(tomorrow)}</h3>
          {plan.data && (
            <span className="muted">
              {plan.data.used_min}/{plan.data.capacity_min} 分钟
              {plan.data.over_capacity && <span className="warn"> · 固定安排已超预算</span>}
            </span>
          )}
        </div>
        {plan.error && <p className="banner error">{plan.error}</p>}
        {plan.data && (
          <>
            {plan.data.planned.length === 0 && <p className="empty small">明天还没有要做的。</p>}
            <ul className="plan-list">
              {plan.data.planned.map((p) => (
                <li key={p.item}>
                  <span className="plan-min">{minutesLabel(p.minutes)}</span> {p.title}
                  <span className="muted"> · {reasonLabel[p.reason]}</span>
                </li>
              ))}
            </ul>
            {plan.data.pushed.length > 0 && (
              <>
                <h4>放不下，建议挪走</h4>
                <ul className="plan-list pushed">
                  {plan.data.pushed.map((p) => (
                    <li key={p.item}>
                      → {dayLabel(p.to)} · {p.title}
                      <div className="muted small">{p.why}</div>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {plan.data.nudges.map((n) => (
              <p key={n.item} className="banner">
                {n.title}：{n.text}
              </p>
            ))}
            {(plan.data.planned.length > 0 || plan.data.pushed.length > 0) && (
              <button
                className="primary"
                onClick={() =>
                  void api
                    .planDay(tomorrow, true)
                    .then(() => {
                      setApplyError(null);
                      reloadAll();
                    })
                    .catch((e) => setApplyError(errorText(e)))
                }
              >
                就这样排明天
              </button>
            )}
            {applyError && <p className="banner error">{applyError}</p>}
          </>
        )}
      </section>
    </div>
  );
}
