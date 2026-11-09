import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type AuditEntry, type Overview } from "../../../lib/api";
import { translate as t } from "../../../i18n";
import { type Names, plainNames, showValue } from "../../../lib/text";

export const AREA_LABELS: Record<string, string> = {
  configuration: "Configuration",
  economy: "Economy",
  roles: "Roles",
  safety: "Safety",
  community: "Community",
  feeds: "Feeds",
  stats: "Stats",
  audit: "Audit log",
};
export type AccessOverview = Overview & { access?: { full: boolean; areas: string[] | null } };
export interface ManagerGrant {
  role_id: string;
  areas: string[];
}
export interface AccessView {
  items: ManagerGrant[];
  areas: string[];
  max: number;
}
export function canonicalGrants(items: ManagerGrant[]) {
  return items
    .map((r) => ({ role_id: r.role_id, areas: [...new Set(r.areas)].sort() }))
    .sort((a, b) => a.role_id.localeCompare(b.role_id));
}
export function useAccess(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/access`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<AccessView>(path) });
  const save = useMutation({
    mutationFn: (items: ManagerGrant[]) => api<AccessView>(path, "PUT", { items: canonicalGrants(items) }),
    onSuccess: async (data) => {
      client.setQueryData([path], data);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
  return { query, save };
}
const PAGE_AREAS: Record<string, string> = {
  modules: "configuration",
  branding: "configuration",
  welcome: "configuration",
  "welcome-image": "configuration",
  "message-studio": "configuration",
  leveling: "configuration",
  points: "configuration",
  economy: "economy",
  commands: "configuration",
  responders: "configuration",
  roles: "roles",
  moderation: "safety",
  "mod-parity": "safety",
  "moderation-desk": "safety",
  safety: "safety",
  automod: "safety",
  "anti-nuke": "safety",
  cases: "safety",
  log: "safety",
  "staff-log": "safety",
  community: "community",
  "community-pack": "community",
  "community-engagement": "community",
  giveaways: "community",
  polls: "community",
  suggestions: "community",
  reminders: "community",
  highlights: "community",
  "auto-channels": "community",
  anniversaries: "community",
  confessions: "community",
  tickets: "community",
  events: "community",
  "temp-voice": "community",
  starboard: "community",
  birthdays: "community",
  achievements: "community",
  utility: "community",
  feeds: "feeds",
  roblox: "feeds",
  trackers: "feeds",
  stats: "stats",
  invites: "stats",
  audit: "audit",
};
export function canOpenPage(data: AccessOverview | undefined, path: string): boolean {
  if (!data?.access) return false;
  if (data.access.full) return true;
  if (!path) return true;
  const area = PAGE_AREAS[path.split("/")[0] ?? ""];
  return !!area && !!data.access.areas?.includes(area);
}
export function areaLabel(area: string) {
  return t(AREA_LABELS[area] ?? area);
}

export interface AuditFilters {
  actor: string;
  area: string;
  from: string;
  until: string;
  text: string;
}
export const EMPTY_FILTERS: AuditFilters = { actor: "", area: "", from: "", until: "", text: "" };
/** Date inputs are calendar dates in UTC; the final selected day is included. */
export function auditParams(filters: AuditFilters): URLSearchParams {
  const params = new URLSearchParams();
  for (const key of ["actor", "area", "text"] as const)
    if (filters[key].trim()) params.set(key, filters[key].trim());
  if (filters.from) params.set("from", new Date(`${filters.from}T00:00:00Z`).toISOString());
  if (filters.until) {
    const end = new Date(`${filters.until}T00:00:00Z`);
    end.setUTCDate(end.getUTCDate() + 1);
    params.set("until", end.toISOString());
  }
  return params;
}
export function useFilteredAudit(guild: string, filters: AuditFilters) {
  const search = auditParams(filters).toString();
  return useInfiniteQuery({
    queryKey: [`/api/guilds/${guild}/audit`, "filters", search],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => {
      const params = new URLSearchParams(search);
      if (pageParam) params.set("before", pageParam);
      return api<{ items: AuditEntry[]; next: string | null }>(`/api/guilds/${guild}/audit-tools?${params}`);
    },
    getNextPageParam: (page) => page.next,
  });
}
export interface DiffLine {
  path: string;
  before: unknown;
  after: unknown;
}
function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
export function auditDiff(before: unknown, after: unknown, path = ""): DiffLine[] {
  if (JSON.stringify(before) === JSON.stringify(after)) return [];
  if ((object(before) && (object(after) || after == null)) || (object(after) && before == null)) {
    const left = object(before) ? before : {};
    const right = object(after) ? after : {};
    return [...new Set([...Object.keys(left), ...Object.keys(right)])]
      .sort()
      .flatMap((key) => auditDiff(left[key], right[key], path ? `${path}.${key}` : key));
  }
  return [{ path: path || t("Value"), before, after }];
}
export function diffText(value: unknown, path = "", names: Names = plainNames) {
  const field = path.split(".").at(-1) ?? "";
  // Keep structured changes inspectable, but preserve the log's readable Discord names.
  // Arrays of objects (for example access grants) must not be summarized as ID lists.
  if (
    typeof value === "boolean" ||
    (typeof value === "string" &&
      (field.endsWith("channel_id") || field.endsWith("role_id") || field === "channel_mode")) ||
    (Array.isArray(value) &&
      value.every((id) => typeof id === "string") &&
      (field === "channels" || field.endsWith("_channels") || field.endsWith("_roles")))
  ) {
    return showValue(field, value, names);
  }
  return value === undefined || value === null
    ? "—"
    : typeof value === "string"
      ? value
      : JSON.stringify(value, null, 2);
}
