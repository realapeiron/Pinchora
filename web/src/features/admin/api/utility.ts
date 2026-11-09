import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type EmbedSpec } from "../../../lib/api";
import { translate as spaTranslate } from "../../../i18n";

export type Recurrence =
  | { kind: "once" }
  | { kind: "interval"; seconds: number }
  | { kind: "daily"; hour: number; minute: number }
  | { kind: "weekly"; weekday: number; hour: number; minute: number }
  | { kind: "cron"; minute: string; hour: string; day: string; month: string; weekday: string };

export interface Sticky {
  channel_id: string;
  content: string;
  embed: EmbedSpec | null;
  min_gap_secs: number;
  min_messages: number;
  message_count: number;
  enabled: boolean;
  pause_reason?: string | null;
  pause_reason_text?: string | null;
}

export interface Scheduled {
  id: string;
  channel_id: string;
  content: string;
  embed: EmbedSpec | null;
  schedule: Recurrence;
  tz: string;
  next_run_at: string;
  last_run_at: string | null;
  runs: number;
  enabled: boolean;
  pause_reason?: string | null;
  pause_reason_text?: string | null;
  ping_allowed: boolean;
}

export interface UtilityList {
  language: string;
  strings: Record<string, string>;
  sticky: Sticky[];
  scheduled: Scheduled[];
  max_scheduled: number;
  timezone: string;
  timezone_notice?: string;
}

export type ScheduledDraft = Omit<
  Scheduled,
  "id" | "last_run_at" | "runs" | "enabled" | "ping_allowed" | "pause_reason" | "pause_reason_text"
>;

export const TIMEZONE_NOTICE = "IANA time zones follow daylight saving changes automatically.";

export function stickyBody({ channel_id, content, embed, min_gap_secs, min_messages, enabled }: Sticky) {
  return { channel_id, content, embed, min_gap_secs, min_messages, enabled };
}

export function previewBody({
  schedule,
  tz,
  next_run_at,
}: Pick<ScheduledDraft, "schedule" | "tz" | "next_run_at">) {
  return { schedule, tz, next_run_at };
}

export function firstRunUtc(input: string): string {
  if (!input) return "";
  const date = new Date(`${input}Z`);
  if (!Number.isFinite(date.getTime())) return "";
  const iso = date.toISOString();
  return iso.slice(0, input.length) === input ? iso : "";
}

export function validTimezone(value: string): boolean {
  if (/^[+-]/.test(value)) return false;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: value });
    return value.length > 0;
  } catch {
    return false;
  }
}

export function newRecurrence(kind: Recurrence["kind"], first: string, offset: string): Recurrence {
  if (kind === "once") return { kind };
  if (kind === "interval") return { kind, seconds: 3600 };
  const at = Date.parse(first);
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: validTimezone(offset) ? offset : "UTC",
    hourCycle: "h23",
    hour: "numeric",
    minute: "numeric",
    weekday: "short",
  }).formatToParts(new Date(Number.isFinite(at) ? at : 0));
  const part = (type: string) => parts.find((p) => p.type === type)?.value ?? "";
  const hour = Number(part("hour"));
  const minute = Number(part("minute"));
  const weekday = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].indexOf(part("weekday"));
  if (kind === "daily") return { kind, hour, minute };
  if (kind === "weekly") return { kind, weekday, hour, minute };
  return { kind, minute: String(minute), hour: String(hour), day: "*", month: "*", weekday: "*" };
}

export function pauseReason(reason: string | null | undefined) {
  switch (reason) {
    case "permissions_lost":
      return spaTranslate("Paused: Pinchora no longer has the required channel permissions.");
    case "channel_deleted":
      return spaTranslate("Paused: the channel was deleted.");
    case "invalid_schedule":
      return spaTranslate("Paused: edit the invalid recurrence.");
    case "delivery_uncertain":
      return spaTranslate(
        "Paused: delivery could not be confirmed. Check the channel before resuming to avoid a duplicate.",
      );
    default:
      return null;
  }
}

export function useUtility(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/utility`;
  const key = [path];
  const refresh = () => client.invalidateQueries({ queryKey: key });
  return {
    query: useQuery({ queryKey: key, queryFn: () => api<UtilityList>(path) }),
    saveSticky: useMutation({
      mutationFn: (body: Sticky) => api<Sticky>(`${path}/sticky`, "POST", stickyBody(body)),
      onSuccess: refresh,
    }),
    removeSticky: useMutation({
      mutationFn: (channel: string) => api(`${path}/sticky/${channel}`, "DELETE"),
      onSuccess: refresh,
    }),
    addScheduled: useMutation({
      mutationFn: (body: ScheduledDraft) => api<{ id: string }>(`${path}/scheduled`, "POST", body),
      onSuccess: refresh,
    }),
    saveScheduled: useMutation({
      mutationFn: ({ id, draft }: { id: string; draft: ScheduledDraft }) =>
        api<{ id: string }>(`${path}/scheduled/${id}`, "PUT", draft),
      onSuccess: refresh,
    }),
    saveTimezone: useMutation({
      mutationFn: (timezone: string) => api<{ timezone: string }>(`${path}/timezone`, "PUT", { timezone }),
      onSuccess: refresh,
    }),
    removeScheduled: useMutation({
      mutationFn: (id: string) => api(`${path}/scheduled/${id}`, "DELETE"),
      onSuccess: refresh,
    }),
    enableScheduled: useMutation({
      mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
        api(`${path}/scheduled/${id}`, "PATCH", { enabled }),
      onSuccess: refresh,
    }),
    preview: useMutation({
      mutationFn: (id: string) => api<{ runs: string[] }>(`${path}/scheduled/${id}/preview`),
    }),
    previewDraft: useMutation({
      mutationFn: (body: Pick<ScheduledDraft, "schedule" | "tz" | "next_run_at">) =>
        api<{ runs: string[] }>(`${path}/scheduled/preview`, "POST", previewBody(body)),
    }),
  };
}
