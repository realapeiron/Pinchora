import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
export interface AntiNukeConfig {
  enabled: boolean;
  limits: Record<string, { count: number; window_secs: number }>;
  response: string;
  log_channel_id: string | null;
  quarantine_role_id: string | null;
  vip_roles: string[];
  trusted_users: string[];
  owner_only_config: boolean;
  auto_restore: boolean;
  revert_vip_bans: boolean;
  remove_bots: boolean;
}
export interface AntiNukeEvent {
  id: string;
  actor_id: string;
  kind: string;
  count: number;
  action: string;
  created_at: string;
  reverted: boolean;
  recovery_state: string;
  restorable: boolean;
}
export interface AntiNukeData {
  config: AntiNukeConfig;
  events: AntiNukeEvent[];
  strings: Record<string, string>;
  kinds: string[];
  responses: string[];
  max_list: number;
  can_edit: boolean;
  can_trust: boolean;
  can_restore: boolean;
  viewer: string;
}
export function words(strings: Record<string, string>) {
  return (key: string, args: Record<string, string | number> = {}) =>
    (strings[key] ?? key).replace(/\{(\w+)\}/g, (match, name: string) => String(args[name] ?? match));
}
export function validUserId(id: string, viewer: string): boolean {
  if (!/^[1-9]\d{0,18}$/.test(id) || id === viewer) return false;
  return BigInt(id) <= 9223372036854775807n;
}
export function draftProblem(c: AntiNukeConfig): boolean {
  return (
    Object.values(c.limits).some(
      (l) =>
        !Number.isInteger(l.count) ||
        l.count < 1 ||
        l.count > 100 ||
        !Number.isInteger(l.window_secs) ||
        l.window_secs < 5 ||
        l.window_secs > 300,
    ) ||
    (c.response === "quarantine" && !c.quarantine_role_id)
  );
}
export function useAntiNuke(guild: string) {
  const client = useQueryClient();
  const key = ["anti_nuke", guild];
  const path = `/api/guilds/${guild}/anti_nuke`;
  const query = useQuery({ queryKey: key, queryFn: () => api<AntiNukeData>(path) });
  const save = useMutation({
    mutationFn: (config: AntiNukeConfig) => api<AntiNukeData>(path, "PUT", config),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const preview = useMutation({
    mutationFn: (body: { config: AntiNukeConfig; kind: string; count: number }) =>
      api<{ message: string }>(`${path}/preview`, "POST", body),
  });
  const restore = useMutation({
    mutationFn: (id: string) =>
      api<{ message: string }>(`${path}/events/${id}/restore`, "POST", { confirm: true }),
    onSuccess: () => client.invalidateQueries({ queryKey: key }),
  });
  return { query, save, preview, restore };
}
