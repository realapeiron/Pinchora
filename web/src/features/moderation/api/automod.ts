import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { serverText } from "../../../i18n";
import { api } from "../../../lib/api";
export interface AutoAction {
  kind: string;
  secs: number;
  role: string | null;
}
export interface AutoRule {
  id: string | null;
  name: string;
  kind: string;
  enabled: boolean;
  log_only: boolean;
  native: boolean;
  discord_rule_id: string | null;
  params: {
    threshold: number;
    window_secs: number;
    words: string[];
    regex: string[];
    allow_domains: string[];
    block_domains: string[];
    presets: number[];
  };
  actions: AutoAction[];
  exempt_roles: string[];
  exempt_channels: string[];
  only_channels: string[];
}
export interface AutoConfig {
  profile: string;
  escalation: { strikes: number; window_secs: number; action: AutoAction }[];
  dm_on_action: boolean;
  alert_channel_id: string | null;
  observe_until: string | null;
  rules: AutoRule[];
}
export interface AutoHit {
  id: string;
  rule_id: string | null;
  user_id: string;
  channel_id: string;
  message_id: string | null;
  action: string;
  snippet: string | null;
  created_at: string;
  reviewed: boolean;
}
export interface AutoData {
  config: AutoConfig;
  hits: AutoHit[];
  profiles: Record<string, AutoConfig>;
  message_content: boolean;
  max_rules: number;
  kinds: string[];
  actions: string[];
  strings: Record<string, string>;
  sync_error: string | null;
  synced_at: string | null;
}
export type Words = (key: string, args?: Record<string, string | number>) => string;
export function words(strings: Record<string, string>): Words {
  return (key, args = {}) => serverText(key, strings, args);
}
export function contentLocked(rule: AutoRule, content: boolean): boolean {
  return !content && !["spam", "mentions", "honeypot"].includes(rule.kind) && !rule.discord_rule_id;
}
export function actionLabel(value: string, t: Words, actions: string[]): string {
  return value
    .split(" · ")
    .map((action) => {
      if (action === "log_only") return t("ui.dry_result");
      if (action === "pending") return t("pending");
      if (action.startsWith("ladder:")) return t("ladder_action", { action: t(`action.${action.slice(7)}`) });
      return actions.includes(action) ? t(`action.${action}`) : action;
    })
    .join(" · ");
}
export function moveAction(actions: AutoAction[], index: number, offset: number): AutoAction[] {
  const next = [...actions];
  const target = index + offset;
  if (target < 0 || target >= next.length) return next;
  const current = next[index];
  const other = next[target];
  if (!current || !other) return next;
  [next[index], next[target]] = [other, current];
  return next;
}
export function useAutoMod(guild: string) {
  const client = useQueryClient();
  const key = ["automod", guild];
  const path = `/api/guilds/${guild}/automod`;
  const query = useQuery({ queryKey: key, queryFn: () => api<AutoData>(path) });
  const save = useMutation({
    mutationFn: (config: AutoConfig) => api<AutoData>(path, "PUT", config),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const profile = useMutation({
    mutationFn: (profile: string) => api<AutoData>(`${path}/profile`, "POST", { profile }),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const test = useMutation({
    mutationFn: (body: { text: string; config: AutoConfig; channel_id?: string | null }) =>
      api<{
        matches: { name: string; kind: string; log_only: boolean; actions: AutoAction[] }[];
        note: string;
      }>(`${path}/test`, "POST", body),
  });
  const reset = useMutation({ mutationFn: (user: string) => api(`${path}/strikes/${user}`, "DELETE") });
  return { query, save, profile, test, reset };
}
