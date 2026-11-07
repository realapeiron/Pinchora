import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
export interface Gate {
  enabled: boolean;
  mode: string;
  verified_role_id: string | null;
  unverified_role_id: string | null;
  channel_id: string | null;
  message_id: string | null;
  kick_after_mins: number;
  min_account_age_hours: number;
  block_default_avatar: boolean;
  block_name_patterns: boolean;
  dm_on_block: boolean;
  log_channel_id: string | null;
  message: string;
  rule_action: string;
  exempt_roles: string[];
  exempt_users: string[];
}
export interface Raid {
  enabled: boolean;
  join_threshold: number;
  window_secs: number;
  action: string;
  lockdown_minutes: number;
  alert_channel_id: string | null;
  similar_threshold: number;
  similar_names: boolean;
  similar_avatars: boolean;
  new_account_hours: number;
  channels: string[];
  slowmode_secs: number;
  lock_channels: boolean;
  pause_invites: boolean;
}
export interface SafetyConfig {
  verification: Gate;
  raid: Raid;
}
export interface RaidEvent {
  id: string;
  started_at: string;
  ended_at: string | null;
  expires_at: string;
  joins: number;
  action: string;
  error: string | null;
}
export interface SafetyData {
  language: string;
  config: SafetyConfig;
  events: RaidEvent[];
  joins: { user_id: string; joined_at: string; status: string }[];
  strings: Record<string, string>;
}
export function words(strings: Record<string, string>, language = "en") {
  return (key: string, args: Record<string, string | number> = {}) =>
    (strings[key] ?? key).replace(/\{(\w+)\}/g, (match, name: string) => {
      const value = args[name];
      return typeof value === "number"
        ? value.toLocaleString(language === "tr" ? "tr-TR" : "en-US")
        : (value ?? match);
    });
}
export function idList(raw: string) {
  return [...new Set(raw.split(/[\s,]+/).filter(Boolean))];
}
export function useSafety(guild: string) {
  const client = useQueryClient();
  const key = ["safety", guild];
  const path = `/api/guilds/${guild}/safety`;
  const query = useQuery({ queryKey: key, queryFn: () => api<SafetyData>(path), refetchInterval: 10000 });
  const save = useMutation({
    mutationFn: (config: SafetyConfig) => api<SafetyData>(path, "PUT", config),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const raid = useMutation({
    mutationFn: (action: string) => api<SafetyData>(`${path}/raid`, "POST", { action }),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const post = useMutation({
    mutationFn: () => api<SafetyData>(`${path}/post`, "POST"),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const preview = useMutation({ mutationFn: () => api<{ image: string }>(`${path}/preview`, "POST") });
  return { query, save, raid, post, preview };
}
