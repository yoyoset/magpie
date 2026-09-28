// Mirrors the serde shapes of magpie-core / magpie-server. Keep in sync with Rust.

export type Severity = "info" | "normal" | "urgent" | "critical";
export type Tier = "desktop" | "phone" | "takeover";

export interface EventView {
  id: string;
  source: string;
  title: string;
  body: string;
  severity: Severity;
  tags: string[];
  created_at: string;
  closed_at: string | null;
}

export type AckKind =
  | { kind: "acknowledged" }
  | { kind: "snoozed"; until: string }
  | { kind: "replied"; text: string }
  | { kind: "dismissed" };

export interface Ack {
  event_id: string;
  kind: AckKind;
  via: string;
  at: string;
}

export type DeliveryOutcome = { status: "sent" } | { status: "failed"; error: string };

export interface Delivery {
  event_id: string;
  channel: string;
  tier: Tier;
  at: string;
  outcome: DeliveryOutcome;
}

export interface EventDetail extends EventView {
  latest_ack: Ack | null;
  deliveries: Delivery[];
}

export type Schedule =
  | { kind: "once"; at: string }
  | { kind: "interval"; anchor: string; every_secs: number }
  | { kind: "cron"; expr: string };

export interface EventDraft {
  source: string;
  title: string;
  body: string;
  severity: Severity;
  tags: string[];
}

export interface ReminderView {
  id: string;
  schedule: Schedule;
  template: EventDraft;
  enabled: boolean;
  created_at: string;
  last_fired: string | null;
  next_fire: string | null;
}

export interface AppInfo {
  version: string;
  port: number;
  data_dir: string;
  config_file: string;
  channels: string[];
  secrets: { key: string; label: string; present: boolean }[];
}

export type ItemKind = "idea" | "todo";
export type ItemStatus = "open" | "done" | "dropped";

export interface Item {
  id: string;
  kind: ItemKind;
  status: ItemStatus;
  title: string;
  why: string;
  action: string;
  source: string;
  area: string;
  day: string | null; // local YYYY-MM-DD
  at: string | null; // HH:MM:SS
  deadline: string | null;
  estimate_min: number | null;
  actual_min: number | null;
  blocking: boolean;
  deferrals: number;
  created_at: string;
  updated_at: string;
  done_at: string | null;
}

export interface ItemDraft {
  kind?: ItemKind;
  title: string;
  why?: string;
  action?: string;
  source?: string;
  area?: string;
  day?: string | null;
  at?: string | null;
  deadline?: string | null;
  estimate_min?: number | null;
  blocking?: boolean;
}

export type ItemChange =
  | { op: "done"; actual_min?: number | null }
  | { op: "schedule"; day: string; at?: string | null }
  | { op: "defer"; to: string }
  | { op: "unschedule" }
  | { op: "estimate"; minutes: number }
  | { op: "drop" }
  | { op: "reopen" };

export interface ItemListQuery {
  status?: ItemStatus;
  kind?: ItemKind;
  day?: string;
  before?: string;
  from?: string;
  to?: string;
  unscheduled?: boolean;
  limit?: number;
}

export type Reason = "fixed_time" | "deadline" | "blocking" | "long_deferred" | "overdue" | "scheduled" | "spare_time";

export interface Plan {
  day: string;
  capacity_min: number;
  used_min: number;
  planned: { item: string; title: string; minutes: number; reason: Reason }[];
  pushed: { item: string; title: string; to: string; why: string }[];
  nudges: { item: string; title: string; text: string }[];
  over_capacity: boolean;
}
