import { useState } from "react";
import { api, errorText } from "../api";
import { localDay } from "../format";

const DAYS = [
  { label: "今天", value: () => localDay(0) },
  { label: "明天", value: () => localDay(1) },
  { label: "以后", value: () => null },
] as const;

/** One line to capture a todo or an idea without leaving the page. */
export function QuickAdd({ onAdded, defaultKind = "todo" }: { onAdded: () => void; defaultKind?: "todo" | "idea" }) {
  const [title, setTitle] = useState("");
  const [why, setWhy] = useState("");
  const [dayIndex, setDayIndex] = useState(defaultKind === "idea" ? 2 : 0);
  const [at, setAt] = useState("");
  const [est, setEst] = useState("");
  const [area, setArea] = useState("");
  const [error, setError] = useState<string | null>(null);
  const isIdea = defaultKind === "idea";

  async function submit(ev: React.FormEvent) {
    ev.preventDefault();
    if (!title.trim()) return;
    const day = isIdea ? null : DAYS[dayIndex]?.value() ?? null;
    try {
      await api.createItem({
        kind: isIdea ? "idea" : "todo",
        title,
        why,
        area,
        source: "桌面",
        day,
        at: day && at ? `${at}:00` : null,
        estimate_min: est ? Number.parseInt(est, 10) : null,
      });
      setTitle("");
      setWhy("");
      setAt("");
      setEst("");
      setError(null);
      onAdded();
    } catch (e) {
      setError(errorText(e));
    }
  }

  return (
    <form className="card quick-add" onSubmit={submit}>
      <div className="row">
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={isIdea ? "一个点子（先放进池子，以后再决定做不做）" : "要做的事，动词开头：定价系数拍板"}
        />
        <button className="primary" type="submit" disabled={!title.trim()}>
          {isIdea ? "放进池子" : "加上"}
        </button>
      </div>
      {title.trim() && (
        <div className="row">
          <input value={why} onChange={(e) => setWhy(e.target.value)} placeholder="为什么要做（最容易忘的部分）" />
          {!isIdea && (
            <>
              <div className="segmented">
                {DAYS.map((d, i) => (
                  <button type="button" key={d.label} className={dayIndex === i ? "on" : ""} onClick={() => setDayIndex(i)}>
                    {d.label}
                  </button>
                ))}
              </div>
              {dayIndex < 2 && <input className="narrow" type="time" value={at} onChange={(e) => setAt(e.target.value)} title="几点（可不填）" />}
              <input className="narrow" type="number" min={1} value={est} onChange={(e) => setEst(e.target.value)} placeholder="估时 分钟" />
            </>
          )}
          <input className="narrow" value={area} onChange={(e) => setArea(e.target.value)} placeholder="工作/家里/孩子" />
        </div>
      )}
      {error && <p className="banner error">{error}</p>}
    </form>
  );
}
