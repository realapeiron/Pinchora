import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
export interface RobloxSettings {
  enabled: boolean;
  verified_role_id: string | null;
  unverified_role_id: string | null;
  nickname_template: string | null;
  verify_channel_id: string | null;
  require_verified: boolean;
}
export interface RobloxBind {
  id: string;
  kind: "verified" | "group_member" | "group_rank";
  group_id: string | null;
  rank_min: number | null;
  rank_max: number | null;
  role_id: string;
  remove_if_unmet: boolean;
}
export interface RobloxData {
  settings: RobloxSettings;
  binds: RobloxBind[];
}
export type NewBind = Omit<RobloxBind, "id">;
const path = (guild: string) => `/api/guilds/${guild}/roblox`;
export function useRoblox(guild: string) {
  return useQuery({ queryKey: [path(guild)], queryFn: () => api<RobloxData>(path(guild)) });
}
export function useRobloxWrite(guild: string) {
  const cache = useQueryClient();
  const update = async (result: RobloxData) => {
    cache.setQueryData([path(guild)], result);
    await cache.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
  };
  return {
    save: useMutation({
      mutationFn: (settings: RobloxSettings) => api<RobloxData>(`${path(guild)}/settings`, "PUT", settings),
      onSuccess: update,
    }),
    add: useMutation({
      mutationFn: (bind: NewBind) => api<RobloxData>(`${path(guild)}/binds`, "POST", bind),
      onSuccess: update,
    }),
    remove: useMutation({
      mutationFn: (id: string) => api<RobloxData>(`${path(guild)}/binds/${id}`, "DELETE"),
      onSuccess: update,
    }),
  };
}
export async function lookupRanks(guild: string, group: string) {
  return api<{ roles: { id: number; name: string; rank: number }[] }>(`${path(guild)}/groups/${group}/ranks`);
}
export function nicknamePreview(template: string, discordName: string, robloxName: string): string {
  const expanded = template.replaceAll("{roblox-name}", robloxName).replaceAll("{discord-name}", discordName);
  const segments = new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(expanded);
  let result = "";
  let chars = 0;
  for (const { segment } of segments) {
    const width = Array.from(segment).length;
    if (chars + width > 32) break;
    result += segment;
    chars += width;
  }
  return result;
}
