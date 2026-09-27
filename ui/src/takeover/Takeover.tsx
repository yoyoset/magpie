import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "../api";
import { Magpie } from "../pet/Magpie";

interface Pending {
  event_id: string;
  title: string;
  body: string;
  source: string;
  attempt: number;
}

const CHIME_EVERY_MS = 4000;

/** Two-note chime from WebAudio, so no sound asset ships with the app. */
function chime(ctx: AudioContext) {
  const now = ctx.currentTime;
  [880, 660].forEach((freq, i) => {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = freq;
    osc.type = "sine";
    gain.gain.setValueAtTime(0.0001, now + i * 0.22);
    gain.gain.exponentialRampToValueAtTime(0.25, now + i * 0.22 + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + i * 0.22 + 0.2);
    osc.connect(gain).connect(ctx.destination);
    osc.start(now + i * 0.22);
    osc.stop(now + i * 0.22 + 0.22);
  });
}

export function Takeover() {
  const [queue, setQueue] = useState<Pending[]>([]);
  const [muted, setMuted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const audio = useRef<AudioContext | null>(null);

  const refresh = useCallback(async () => {
    setQueue(await invoke<Pending[]>("takeover_pending"));
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("takeover://changed", () => void refresh());
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  // Sound while anything is pending and not muted.
  useEffect(() => {
    if (queue.length === 0 || muted) return;
    audio.current ??= new AudioContext();
    const ctx = audio.current;
    const ring = () => {
      if (ctx.state === "suspended") void ctx.resume();
      chime(ctx);
    };
    ring();
    const id = window.setInterval(ring, CHIME_EVERY_MS);
    return () => window.clearInterval(id);
  }, [queue.length, muted]);

  const current = queue[0];

  async function respond(kind: "ack" | "snooze") {
    if (!current) return;
    try {
      await api.ack(
        current.event_id,
        kind === "ack" ? { kind: "acknowledged" } : { kind: "snoozed", until: new Date(Date.now() + 30 * 60_000).toISOString() },
        "takeover",
      );
      setError(null);
      await invoke("takeover_done", { id: current.event_id });
      await refresh();
    } catch (e) {
      setError(errorText(e));
    }
  }

  if (!current) return <div className="takeover empty" />;

  return (
    <div className="takeover">
      <div className="takeover-card">
        <div className="takeover-bird">
          <Magpie mood="alert" />
        </div>
        <div className="takeover-label">
          严重 · {current.source}
          {current.attempt > 0 && ` · 第 ${current.attempt + 1} 次`}
          {queue.length > 1 && ` · 还有 ${queue.length - 1} 件`}
        </div>
        <h1>{current.title}</h1>
        {current.body && <pre className="takeover-body">{current.body}</pre>}
        <div className="takeover-actions">
          <button className="primary big" onClick={() => void respond("ack")} autoFocus>
            我知道了
          </button>
          <button className="big" onClick={() => void respond("snooze")}>
            30 分钟后再说
          </button>
          <button className="ghost" onClick={() => void invoke("show_inbox")}>
            打开收件箱
          </button>
          <button className="ghost" onClick={() => setMuted((m) => !m)}>
            {muted ? "取消静音" : "静音"}
          </button>
        </div>
        {error && <p className="banner error">{error}</p>}
        <p className="hint">Alt+F4 或托盘「关闭全屏提醒」可临时关掉；没处理的话下一轮还会出现。</p>
      </div>
    </div>
  );
}
