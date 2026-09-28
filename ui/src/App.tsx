import { useState } from "react";
import { Ideas } from "./components/Ideas";
import { Inbox } from "./components/Inbox";
import { Reminders } from "./components/Reminders";
import { Settings } from "./components/Settings";
import { Today } from "./components/Today";

type Tab = "today" | "inbox" | "ideas" | "reminders" | "settings";

const tabs: { id: Tab; label: string }[] = [
  { id: "today", label: "今天" },
  { id: "inbox", label: "收件箱" },
  { id: "ideas", label: "点子池" },
  { id: "reminders", label: "提醒" },
  { id: "settings", label: "设置" },
];

export function App() {
  const [tab, setTab] = useState<Tab>("today");
  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="brand">
          <span className="brand-mark">鹊</span>
          <span>Magpie</span>
        </div>
        {tabs.map((t) => (
          <button key={t.id} className={`nav-item ${tab === t.id ? "active" : ""}`} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <main className="content">
        {tab === "today" && <Today goInbox={() => setTab("inbox")} />}
        {tab === "inbox" && <Inbox />}
        {tab === "ideas" && <Ideas />}
        {tab === "reminders" && <Reminders />}
        {tab === "settings" && <Settings />}
      </main>
    </div>
  );
}
