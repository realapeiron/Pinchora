import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "../../../lib/api";

export interface InviteSettings {
  enabled: boolean;
  log_channel_id: string | null;
  fake_days: number;
  temp_enabled: boolean;
  temp_channel_id: string | null;
  temp_max_age_secs: number;
  temp_max_uses: number;
  temp_per_user_daily: number;
  temp_message_id: string | null;
}
export interface Reward {
  invites: number;
  role_id: string;
}
export interface InviteScore {
  user_id: string;
  regular: number;
  left: number;
  fake: number;
  bonus: number;
  total: number;
}
export interface InviteJoin {
  user_id: string;
  joined_at: string;
  left_at: string | null;
  fake: boolean;
  source: string;
  code: string | null;
}
export interface InviteOverview {
  language: string;
  strings: Record<string, string>;
  settings: InviteSettings;
  can_read_invites: boolean;
  rewards: Reward[];
  limits: { rewards: number; fake_days: number; temp_daily: number };
}
export type InvitePeriod = "all" | "week" | "month";
const base = (guild: string) => `/api/guilds/${guild}/invites`;
export function useInvites(guild: string) {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ["invites", guild],
    queryFn: () => api<InviteOverview>(base(guild)),
  });
  const save = useMutation({
    mutationFn: (body: InviteSettings) => api<InviteSettings>(base(guild) + "/settings", "PUT", body),
    onSuccess: () => client.invalidateQueries({ queryKey: ["invites", guild] }),
  });
  const reward = useMutation({
    mutationFn: ({ invites, role_id }: Reward) =>
      api<Reward>(base(guild) + `/rewards/${invites}`, "PUT", { role_id }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["invites", guild] }),
  });
  const remove = useMutation({
    mutationFn: (invites: number) => api<void>(base(guild) + `/rewards/${invites}`, "DELETE"),
    onSuccess: () => client.invalidateQueries({ queryKey: ["invites", guild] }),
  });
  return { query, save, reward, remove };
}
export function useInviteLeaders(guild: string, period: InvitePeriod, page: number) {
  return useQuery({
    queryKey: ["invite-leaders", guild, period, page],
    queryFn: () =>
      api<{ page: number; rows: InviteScore[] }>(base(guild) + `/leaders?period=${period}&page=${page}`),
  });
}
export function useInviteMember(guild: string, user: string | null) {
  return useQuery({
    queryKey: ["invite-member", guild, user],
    enabled: !!user,
    queryFn: () => api<{ score: InviteScore; joins: InviteJoin[] }>(base(guild) + `/member/${user}`),
  });
}
export function inviteExportUrl(guild: string) {
  return base(guild) + "/export.csv";
}
