import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, errorText } from "../api";
import type { Severity } from "../types";
import { Magpie, type Mood } from "./Magpie";

interface Bubble {
  event_id: string;
  title: string;
  body: string;
  source: string;
  severity: Severity;
  attempt: number;
}

const ESCALATING: Severity[] = ["urgent", "critical"];
/** Normal/info bubbles fade on their own; escalating ones wait for an answer. */
const AUTO_HIDE_MS = 15_000;
const DRAG_THRESHOLD_PX = 4;

function isNight(d = new Date()): boolean {
  const h = d.getHours();
  return h >= 23 || h < 7;
}

export function Pet() {
  const [bubble, setBubble] = useState<Bubble | null>(null);
  const [pending, setPending] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);
  const hideTimer = useRef<number | undefined>(undefined);
  const root = useRef<HTMLDivElement>(null);

  // Tell the shell where we draw; the rest of the transparent window is click-through.
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const report = () => {
      const rects = Array.from(el.querySelectorAll<HTMLElement>(".bird, .bubble, .pet-menu")).map((n) => {
        const r = n.getBoundingClientRect();
        return [r.x, r.y, r.width, r.height];
      });
      void invoke("pet_hit_rects", { rects });
    };
    report();
    const ro = new ResizeObserver(report);
    ro.observe(el);
    el.querySelectorAll(".bird, .bubble, .pet-menu").forEach((n) => ro.observe(n));
    return () => ro.disconnect();
  }, [bubble, menu, error]);

  // Bubbles pushed by the `pet` channel.
  useEffect(() => {
    const unlisten = listen<Bubble>("pet://message", (e) => {
      setBubble(e.payload);
      setError(null);
      window.clearTimeout(hideTimer.current);
      if (!ESCALATING.includes(e.payload.severity)) {
        hideTimer.current = window.setTimeout(() => setBubble(null), AUTO_HIDE_MS);
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  // Mood comes from the inbox itself, so it is right even if a bubble was missed.
  useEffect(() => {
    const load = async () => {
      try {
        const open = await api.listEvents(true, 100);
        setPending(open.filter((e) => ESCALATING.includes(e.severity)).length);
      } catch {
        /* services still starting; try again next tick */
      }
    };
    void load();
    const id = window.setInterval(() => void load(), 3000);
    return () => window.clearInterval(id);
  }, []);

  const mood: Mood = pending > 0 || (bubble && ESCALATING.includes(bubble.severity)) ? "alert" : isNight() ? "sleep" : "idle";

  async function respond(kind: "ack" | "snooze") {
    if (!bubble) return;
    try {
      await api.ack(
        bubble.event_id,
        kind === "ack" ? { kind: "acknowledged" } : { kind: "snoozed", until: new Date(Date.now() + 30 * 60_000).toISOString() },
        "pet",
      );
      setBubble(null);
      setPending((n) => Math.max(0, n - 1));
    } catch (e) {
      setError(errorText(e));
    }
  }

  // Click vs drag: only start an OS drag after the pointer actually moves.
  const down = useRef<{ x: number; y: number } | null>(null);
  function onPointerDown(e: React.PointerEvent) {
    if (e.button !== 0) return;
    down.current = { x: e.screenX, y: e.screenY };
  }
  function onPointerMove(e: React.PointerEvent) {
    const d = down.current;
    if (d && Math.hypot(e.screenX - d.x, e.screenY - d.y) > DRAG_THRESHOLD_PX) {
      down.current = null;
      void getCurrentWindow().startDragging();
    }
  }
  function onPointerUp() {
    if (down.current) {
      down.current = null;
      setMenu(false);
      // Click: open the inbox when something is waiting, otherwise just say hi.
      if (pending > 0 || bubble) void invoke("show_inbox");
    }
  }

  return (
    <div className="pet" ref={root}>
      {bubble && (
        <div className={`bubble sev-${bubble.severity}`}>
          <div className="bubble-title">{bubble.title}</div>
          {bubble.body && <div className="bubble-body">{bubble.body}</div>}
          <div className="bubble-meta">
            {bubble.source}
            {bubble.attempt > 0 && ` · 第 ${bubble.attempt + 1} 次`}
          </div>
          <div className="bubble-actions">
            <button className="primary" onClick={() => void respond("ack")}>
              确认
            </button>
            <button onClick={() => void respond("snooze")}>30 分钟后</button>
            <button className="ghost" onClick={() => void invoke("show_inbox")}>
              打开
            </button>
            {!ESCALATING.includes(bubble.severity) && (
              <button className="ghost" onClick={() => setBubble(null)} aria-label="关闭">
                ×
              </button>
            )}
          </div>
          {error && <div className="bubble-error">{error}</div>}
        </div>
      )}
      {menu && (
        <div className="pet-menu">
          <button onClick={() => void invoke("show_inbox")}>打开收件箱</button>
          <button onClick={() => void invoke("hide_pet")}>隐藏桌宠</button>
        </div>
      )}
      <div
        className="bird"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu((m) => !m);
        }}
        title={pending > 0 ? `${pending} 件事在等你` : "Magpie 喜鹊"}
      >
        <Magpie mood={mood} />
        {pending > 0 && <span className="badge">{pending}</span>}
      </div>
    </div>
  );
}
