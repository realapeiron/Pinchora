// Server stats: the API shapes, the hooks, and the small pure helpers the charts draw with.

import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "./api";
import { locale, number } from "../i18n";

export const PERIODS = [7, 30, 90] as const;
export type Days = (typeof PERIODS)[number];

export interface DayRow {
  day: string;
  messages: number;
  voice_minutes: number;
  joins: number;
  leaves: number;
  commands: number;
}

export interface Totals {
  messages: number;
  voice_minutes: number;
  joins: number;
  leaves: number;
  commands: number;
  active_members: number | null;
}

export interface TopChannel {
  id: string | null;
  name: string | null;
  messages: number;
  voice_minutes: number;
}

export interface TopMember {
  id: string;
  name: string | null;
  avatar_url: string | null;
  messages: number;
  voice_minutes: number;
}

/** One period of a server, as both the dashboard and the public page get it. */
export interface PeriodStats {
  days: Days;
  from: string;
  to: string;
  totals: Totals;
  /** The period before, for the change; `null` for 90 days (nothing older is kept). */
  previous: Totals | null;
  daily: DayRow[];
  /** `[weekday][hour]`, Monday first, UTC. */
  heatmap: number[][];
  top_channels: TopChannel[];
}

/** `GET /api/guilds/{g}/stats`. */
export interface GuildStats extends PeriodStats {
  top_members: TopMember[];
  public: boolean;
  voice_counted: boolean;
  module_on: boolean;
}

/** `GET /api/public/stats/{g}`. */
export interface PublicGuildStats extends PeriodStats {
  guild: { id: string; name: string; icon_url: string | null };
}

export interface CounterKind {
  key: string;
  label: string;
  needs_role: boolean;
  template: string;
}

export interface Counter {
  channel_id: string;
  channel_name: string | null;
  kind: string;
  role_id: string | null;
  template: string;
  value: number | null;
  name: string | null;
  renamed_at: string | null;
  problem: string | null;
}

export interface Counters {
  counters: Counter[];
  max: number;
  template_max: number;
  kinds: CounterKind[];
}

export interface CounterDraft {
  channel_id?: string | null;
  kind: string;
  role_id?: string | null;
  template?: string | null;
}

export interface CounterPreview {
  name: string | null;
  value: number | null;
  problem: string | null;
}

/** `GET /api/public/stats`. */
export interface GlobalStats {
  totals: { servers: number; players: number; houses: number; pets: number; fish: number; games: number };
  history: { day: string; servers: number; players: number; messages: number }[];
}

const base = (guild: string) => `/api/guilds/${guild}/stats`;

export function useGuildStats(guild: string, days: Days) {
  return useQuery({
    queryKey: [base(guild), days],
    queryFn: () => api<GuildStats>(`${base(guild)}?days=${days}`),
    // A new period keeps the old charts on screen, dimmed, until it arrives.
    placeholderData: keepPreviousData,
  });
}

export function useCounters(guild: string) {
  const client = useQueryClient();
  const path = `${base(guild)}/counters`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<Counters>(path) });
  const done = async (list: Counters) => {
    client.setQueryData([path], list);
    await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
  };
  const add = useMutation({
    mutationFn: (draft: CounterDraft) => api<Counters>(path, "POST", draft),
    onSuccess: done,
  });
  const remove = useMutation({
    mutationFn: (channel: string) => api<Counters>(`${path}/${channel}`, "DELETE"),
    onSuccess: done,
  });
  return { query, add, remove };
}

export function previewCounter(guild: string, draft: CounterDraft): Promise<CounterPreview> {
  return api<CounterPreview>(`${base(guild)}/counters/preview`, "POST", draft);
}

export function useSetPublic(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (on: boolean) => api<{ public: boolean }>(`${base(guild)}/settings`, "PUT", { public: on }),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: [base(guild)] });
    },
  });
}

export function usePublicStats(guild: string) {
  return useQuery({
    queryKey: ["/api/public/stats", guild],
    queryFn: () => api<PublicGuildStats>(`/api/public/stats/${guild}`),
    retry: false,
  });
}

export function useGlobalStats() {
  return useQuery({
    queryKey: ["/api/public/stats"],
    queryFn: () => api<GlobalStats>("/api/public/stats"),
    staleTime: 5 * 60_000,
    retry: false,
  });
}

// /Numbers

/** `1,234`. */
export function thousands(n: number): string {
  return number(n);
}

/** `999`, `1.2K`, `48K`, `1.3M`: for axis ticks and tight tiles. */
export function compact(n: number): string {
  const a = Math.abs(n);
  const unit = (value: number, suffix: string) => {
    const text = number(value >= 100 ? Math.round(value) : Math.round(value * 10) / 10);
    return `${n < 0 ? "-" : ""}${text}${suffix}`;
  };
  if (a >= 1e9) return unit(a / 1e9, "B");
  if (a >= 1e6) return unit(a / 1e6, "M");
  if (a >= 1e3) return unit(a / 1e3, "K");
  return String(n);
}

/** Minutes as `45m`, `3h 20m` or `1,204h`, the way the cards write them. */
export function minutes(n: number): string {
  if (n < 60) return `${Math.max(0, n)}m`;
  if (n < 6000 && n % 60 !== 0) return `${Math.floor(n / 60)}h ${n % 60}m`;
  return `${thousands(Math.floor(n / 60))}h`;
}

/** The smallest of 1, 2, 4, 6 or 8 times a power of ten at least `n`, as the cards' scale. */
export function niceMax(n: number): number {
  if (n <= 1) return 1;
  for (let step = 1; ; step *= 10) {
    for (const m of [1, 2, 4, 6, 8]) {
      if (step * m >= n) return step * m;
    }
  }
}

/** The change from `before` to `now` in whole percent; `null` when there was nothing before. */
export function change(now: number, before: number | null | undefined): number | null {
  if (before === null || before === undefined || before <= 0) return null;
  return Math.round(((now - before) / before) * 100);
}

/** The heat shade (0..steps-1) for `n` when the busiest cell has `max`; `null` for none. */
export function heatStep(n: number, max: number, steps = 5): number | null {
  if (n <= 0 || max <= 0) return null;
  return Math.min(steps, Math.max(1, Math.ceil((n / max) * steps))) - 1;
}

export const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/** `2026-11-06` as `6 Nov`, read as a UTC day. */
export function dayLabel(day: string, withYear = false): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Intl.DateTimeFormat(locale() === "tr" ? "tr-TR" : "en-US", {
    day: "numeric",
    month: "short",
    ...(withYear ? { year: "numeric" as const } : {}),
    timeZone: "UTC",
  }).format(new Date(Date.UTC(y!, (m ?? 1) - 1, d!)));
}

/** `09:00–10:00 UTC`. */
export function hourLabel(hour: number): string {
  const pad = (h: number) => String(h).padStart(2, "0");
  return `${pad(hour)}:00–${pad((hour + 1) % 24)}:00 UTC`;
}

/** The busiest weekday and hour of a heatmap, if anything happened. */
export function peak(grid: number[][]): { weekday: number; hour: number; messages: number } | null {
  let best: { weekday: number; hour: number; messages: number } | null = null;
  grid.forEach((hours, weekday) =>
    hours.forEach((messages, hour) => {
      if (messages > 0 && (!best || messages > best.messages)) best = { weekday, hour, messages };
    }),
  );
  return best;
}

/** A counter's name with `{count}` filled in, as the bot writes it. */
export function counterName(template: string, value: number): string {
  return template.replaceAll("{count}", thousands(value)).slice(0, 100);
}
