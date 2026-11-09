import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "../../../lib/api";

export interface Achievement {
  id: string;
  name: string;
  description: string;
  icon: string;
  kind: string;
  target: number;
  reward_role_id: string | null;
  reward_points: number;
  announce_channel_id: string | null;
  secret: boolean;
  enabled: boolean;
  module_off: boolean;
}
export type AchievementDraft = Omit<Achievement, "id" | "module_off">;
export interface AchievementPage {
  items: Achievement[];
  max: number;
  icons: string[];
  kinds: string[];
  language: string;
  strings: Record<string, string>;
}
export interface Holder {
  user_id: string;
  earned_at: string;
}

const base = (guild: string) => `/api/guilds/${guild}/achievements`;

export function useAchievements(guild: string) {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ["achievements", guild],
    queryFn: () => api<AchievementPage>(base(guild)),
  });
  const refresh = () => client.invalidateQueries({ queryKey: ["achievements", guild] });
  const save = useMutation({
    mutationFn: ({ id, draft }: { id?: string; draft: AchievementDraft }) =>
      api<Achievement>(id ? `${base(guild)}/${id}` : base(guild), id ? "PUT" : "POST", draft),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: (id: string) => api<void>(`${base(guild)}/${id}`, "DELETE"),
    onSuccess: refresh,
  });
  return { query, save, remove };
}

export function useAchievementHolders(guild: string, id: string | null) {
  return useQuery({
    queryKey: ["achievement-holders", guild, id],
    enabled: !!id,
    queryFn: () => api<{ members: Holder[] }>(`${base(guild)}/${id}/holders`),
  });
}
