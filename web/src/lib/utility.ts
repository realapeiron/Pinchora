import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type EmbedSpec } from "./api";

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
  sticky: Sticky[];
  scheduled: Scheduled[];
  max_scheduled: number;
  utc_offset: string;
  fixed_offset_notice?: string;
}

export type ScheduledDraft = Omit<
  Scheduled,
  "id" | "last_run_at" | "runs" | "enabled" | "ping_allowed" | "pause_reason" | "pause_reason_text"
>;

export const FIXED_OFFSET_NOTICE =
  "UTC offsets are fixed and do not adjust for daylight saving time. Update the offset when local clocks change.";

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

export function newRecurrence(kind: Recurrence["kind"], first: string, offset: string): Recurrence {
  if (kind === "once") return { kind };
  if (kind === "interval") return { kind, seconds: 3600 };
  const match = /^([+-])(\d{2}):(\d{2})$/.exec(offset);
  const minutes = match ? (Number(match[2]) * 60 + Number(match[3])) * (match[1] === "-" ? -1 : 1) : 0;
  const at = Date.parse(first);
  const date = new Date((Number.isFinite(at) ? at : 0) + minutes * 60_000);
  const hour = date.getUTCHours();
  const minute = date.getUTCMinutes();
  if (kind === "daily") return { kind, hour, minute };
  if (kind === "weekly") return { kind, weekday: date.getUTCDay(), hour, minute };
  return { kind, minute: String(minute), hour: String(hour), day: "*", month: "*", weekday: "*" };
}

export function pauseReason(reason: string | null | undefined) {
  switch (reason) {
    case "permissions_lost":
      return "Paused: Pinchora no longer has the required channel permissions.";
    case "channel_deleted":
      return "Paused: the channel was deleted.";
    case "invalid_schedule":
      return "Paused: edit the invalid recurrence.";
    case "delivery_uncertain":
      return "Paused: delivery could not be confirmed. Check the channel before resuming to avoid a duplicate.";
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
      mutationFn: (utc_offset: string) =>
        api<{ utc_offset: string }>(`${path}/timezone`, "PUT", { utc_offset }),
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
