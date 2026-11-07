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
  ping_allowed: boolean;
}

export interface UtilityList {
  sticky: Sticky[];
  scheduled: Scheduled[];
  max_scheduled: number;
  utc_offset: string;
}

export type ScheduledDraft = Omit<Scheduled, "id" | "last_run_at" | "runs" | "enabled" | "ping_allowed">;

export function useUtility(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/utility`;
  const key = [path];
  const refresh = () => client.invalidateQueries({ queryKey: key });
  return {
    query: useQuery({ queryKey: key, queryFn: () => api<UtilityList>(path) }),
    saveSticky: useMutation({
      mutationFn: (body: Sticky) => api<Sticky>(`${path}/sticky`, "POST", body),
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
        api<{ runs: string[] }>(`${path}/scheduled/preview`, "POST", body),
    }),
  };
}
