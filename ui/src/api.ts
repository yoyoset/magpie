// Typed wrappers over the Tauri commands in app/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";
import type { AckKind, AppInfo, EventDetail, EventView, ReminderView, Schedule, EventDraft } from "./types";

export const api = {
  listEvents: (open: boolean, limit = 200) =>
    invoke<EventView[]>("list_events", { query: { open, limit } }),
  eventDetail: (id: string) => invoke<EventDetail>("event_detail", { id }),
  ack: (id: string, kind: AckKind, via: "desktop" | "pet" = "desktop") =>
    invoke<void>("ack_event", { id, kind, via }),
  listReminders: () => invoke<ReminderView[]>("list_reminders"),
  createReminder: (schedule: Schedule, template: EventDraft) =>
    invoke<ReminderView>("create_reminder", { reminder: { schedule, template, enabled: true } }),
  deleteReminder: (id: string) => invoke<void>("delete_reminder", { id }),
  appInfo: () => invoke<AppInfo>("app_info"),
  setSecret: (key: string, value: string) => invoke<void>("set_secret", { key, value }),
  openDataFolder: () => invoke<void>("open_data_folder"),
};

/**
 * Tauri rejects with the Rust error string. Argument-decoding failures come
 * wrapped as "invalid args `x` for command `y`: <reason>"; users only need the reason.
 */
export function errorText(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
  return raw.replace(/^invalid args `[^`]+` for command `[^`]+`: /, "");
}
