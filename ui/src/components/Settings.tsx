import { useState } from "react";
import { api, errorText } from "../api";
import { usePolling } from "../usePolling";

export function Settings() {
  const { data: info, error, reload } = usePolling(api.appInfo, 10_000);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<string | null>(null);

  async function save(key: string) {
    const value = drafts[key]?.trim();
    if (!value) return;
    try {
      await api.setSecret(key, value);
      setDrafts((d) => ({ ...d, [key]: "" }));
      setMessage("已保存到 Windows 凭据管理器。重启 Magpie 后生效。");
      await reload();
    } catch (e) {
      setMessage(errorText(e));
    }
  }

  if (error) return <p className="banner error">读取失败：{error}</p>;
  if (!info) return <p className="empty">加载中…</p>;

  return (
    <div className="page">
      <h1>设置</h1>
      <section className="card">
        <h3>运行状态</h3>
        <dl className="kv">
          <dt>版本</dt>
          <dd>{info.version}</dd>
          <dt>本机 API</dt>
          <dd>127.0.0.1:{info.port}</dd>
          <dt>已启用渠道</dt>
          <dd>{info.channels.length ? info.channels.join("、") : "无"}</dd>
          <dt>配置文件</dt>
          <dd>
            <code>{info.config_file}</code>
          </dd>
        </dl>
        <p className="hint">渠道、路由、升级策略都在配置文件里改，改完重启 Magpie。拼错的键会让它拒绝启动（故意的）。</p>
        <button onClick={() => void api.openDataFolder()}>打开配置与日志目录</button>
      </section>

      <section className="card">
        <h3>密钥</h3>
        <p className="hint">只写入 Windows 凭据管理器，不落任何文件，也不会显示出来。</p>
        {info.secrets.map((s) => (
          <div key={s.key} className="row secret">
            <span className="secret-label">
              {s.label} <span className={s.present ? "ok" : "missing"}>{s.present ? "已设置" : "未设置"}</span>
            </span>
            <input
              type="password"
              autoComplete="off"
              value={drafts[s.key] ?? ""}
              onChange={(e) => setDrafts((d) => ({ ...d, [s.key]: e.target.value }))}
              placeholder={s.present ? "输入新值以替换" : "粘贴后保存"}
            />
            <button onClick={() => void save(s.key)} disabled={!drafts[s.key]?.trim()}>
              保存
            </button>
          </div>
        ))}
        {message && <p className="banner">{message}</p>}
      </section>
    </div>
  );
}
