// Data hooks. Query keys are the API paths.

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  type AuditPage,
  type Community,
  type GuildEntry,
  type GuildInfo,
  type Job,
  type LevelRoles,
  type Me,
  type Overview,
  type RepRoles,
  type Settings,
  type SettingsPatch,
  type Store,
  type XpRules,
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
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}

/** The server's channels and roles as the bot sees them. */
export function useDiscord(guild: string) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/discord`],
    queryFn: () => api<GuildInfo>(`/api/guilds/${guild}/discord`),
    staleTime: 60_000,
  });
}

export function useRefreshDiscord(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api<GuildInfo>(`/api/guilds/${guild}/discord?refresh=1`),
    onSuccess: (info) => client.setQueryData([`/api/guilds/${guild}/discord`], info),
  });
}

/** A list resource (`GET path`) whose writes answer with the whole new list. */
function useList<T>(path: string) {
  const client = useQueryClient();
  const query = useQuery({ queryKey: [path], queryFn: () => api<T>(path) });
  const write = useMutation({
    mutationFn: ({ method, sub, body }: { method: "PUT" | "DELETE"; sub: string; body?: unknown }) =>
      api<T>(path + sub, method, body),
    onSuccess: async (list) => {
      client.setQueryData([path], list);
      await client.invalidateQueries({ queryKey: [path.replace(/\/[^/]+$/, "/audit")] });
    },
  });
  return { query, write };
}

export const useXpRules = (guild: string) => useList<XpRules>(`/api/guilds/${guild}/xp-rules`);
export const useLevelRoles = (guild: string) => useList<LevelRoles>(`/api/guilds/${guild}/level-roles`);
export const useRepRoles = (guild: string) => useList<RepRoles>(`/api/guilds/${guild}/rep-roles`);
export const useStore = (guild: string) => useList<Store>(`/api/guilds/${guild}/store`);

export function useCommunity(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/community`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<Community>(path) });
  const save = useMutation({
    mutationFn: async (next: Partial<Community>) => {
      const now = client.getQueryData<Community>([path]);
      const out = { ...now } as Community;
      if (next.suggestions) {
        out.suggestions = await api(`/api/guilds/${guild}/suggestions/settings`, "PUT", next.suggestions);
      }
      if (next.critters) {
        out.critters = await api(`/api/guilds/${guild}/critters/settings`, "PUT", next.critters);
      }
      return out;
    },
    onSuccess: async (community) => {
      client.setQueryData([path], community);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
      await client.invalidateQueries({ queryKey: overviewKey(guild) });
    },
  });
  return { query, save };
}

/** Starts the level role sync; resolves with the job to follow. */
export function useStartSync(guild: string) {
  return useMutation({
    mutationFn: () => api<{ job_id: string }>(`/api/guilds/${guild}/level-roles/sync`, "POST"),
  });
}

/** A running job, polled every second until it ends. */
export function useJob(guild: string, id: string | null) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/jobs/${id}`],
    queryFn: () => api<Job>(`/api/guilds/${guild}/jobs/${id}`),
    enabled: id !== null,
    refetchInterval: (query) => (query.state.data?.status === "running" ? 1000 : false),
  });
}

export function useAudit(guild: string) {
  return useInfiniteQuery({
    queryKey: [`/api/guilds/${guild}/audit`],
    queryFn: ({ pageParam }) =>
      api<AuditPage>(`/api/guilds/${guild}/audit${pageParam ? `?before=${pageParam}` : ""}`),
    initialPageParam: "",
    getNextPageParam: (last) => last.next ?? undefined,
  });
}

export function useMessagePreview(guild: string, kind: string, message: string | null, enabled: boolean) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/preview/message`, kind, message],
    queryFn: () =>
      api<{ text: string }>(`/api/guilds/${guild}/preview/message`, "POST", { kind, message }).then(
        (r) => r.text,
      ),
    enabled,
    staleTime: Infinity,
    retry: false,
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
