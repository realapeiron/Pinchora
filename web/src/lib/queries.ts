// Data hooks. Query keys are the API paths.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  type GuildEntry,
  type Me,
  type Overview,
  type Settings,
  type SettingsPatch,
  api,
  setCsrfToken,
} from "./api";

export function useMe() {
  return useQuery({
    queryKey: ["/api/me"],
    queryFn: async () => {
      const me = await api<Me>("/api/me");
      setCsrfToken(me.csrf);
      return me;
    },
    staleTime: 5 * 60_000,
  });
}

export function useGuilds() {
  return useQuery({
    queryKey: ["/api/me/guilds"],
    queryFn: () => api<{ items: GuildEntry[] }>("/api/me/guilds").then((r) => r.items),
  });
}

export function useRefreshGuilds() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api<{ items: GuildEntry[] }>("/api/me/guilds/refresh", "POST").then((r) => r.items),
    onSuccess: (items) => client.setQueryData(["/api/me/guilds"], items),
  });
}

export function overviewKey(guild: string) {
  return [`/api/guilds/${guild}`];
}

export function useOverview(guild: string) {
  return useQuery({
    queryKey: overviewKey(guild),
    queryFn: () => api<Overview>(`/api/guilds/${guild}`),
  });
}

export function useSaveSettings(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (patch: SettingsPatch) => api<Settings>(`/api/guilds/${guild}/settings`, "PATCH", patch),
    onSuccess: async (settings) => {
      client.setQueryData<Overview>(overviewKey(guild), (old) => (old ? { ...old, settings } : old));
      await client.invalidateQueries({ queryKey: overviewKey(guild) });
    },
  });
}

export function useLogout() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api<void>("/auth/logout", "POST"),
    onSettled: () => {
      setCsrfToken(null);
      client.clear();
      window.location.assign("/login");
    },
  });
}
