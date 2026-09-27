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
