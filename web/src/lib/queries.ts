// Data hooks. Query keys are the API paths.

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { channelLabel } from "../components/Pickers";

import {
  type AuditPage,
  type Community,
  type GuildEntry,
  type GuildInfo,
  type Job,
  type LevelRoles,
  type Me,
  type ModulePatch,
  type Modules,
  type Overview,
  type RepRoles,
  type Settings,
  type SettingsPatch,
  type Store,
  type XpRules,
  ApiError,
  api,
  setCsrfToken,
} from "./api";
import { type Names, plainNames } from "./text";

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

/** Which part of the Community page a refused save was about, with the refusal. */
export class CommunitySaveError extends Error {
  readonly section: "suggestions" | "critters" | "surge";
  readonly reason: ApiError;
  constructor(section: CommunitySaveError["section"], reason: ApiError) {
    super(reason.message);
    this.name = "CommunitySaveError";
    this.section = section;
    this.reason = reason;
  }
}

async function inSection<T>(section: CommunitySaveError["section"], run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    throw error instanceof ApiError ? new CommunitySaveError(section, error) : error;
  }
}

export function useCommunity(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/community`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<Community>(path) });
  const save = useMutation({
    mutationFn: async (next: Partial<Community>) => {
      const now = client.getQueryData<Community>([path]);
      const out = { ...now } as Community;
      if (next.suggestions) {
        out.suggestions = await inSection("suggestions", () =>
          api(`/api/guilds/${guild}/suggestions/settings`, "PUT", next.suggestions),
        );
      }
      if (next.critters) {
        out.critters = await inSection("critters", () =>
          api(`/api/guilds/${guild}/critters/settings`, "PUT", next.critters),
        );
      }
      if (next.surge) {
        const { enabled, channel_id, every_hours } = next.surge;
        out.surge = await inSection("surge", () =>
          api(`/api/guilds/${guild}/surge/settings`, "PUT", {
            enabled,
            channel_id: channel_id ?? undefined,
            every_hours,
          }),
        );
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

export function modulesKey(guild: string) {
  return [`/api/guilds/${guild}/modules`];
}

export function useModules(guild: string) {
  return useQuery({
    queryKey: modulesKey(guild),
    queryFn: () => api<Modules>(`/api/guilds/${guild}/modules`),
  });
}

/** One change to a module, by key. */
export interface ModuleChange {
  key: string;
  patch: ModulePatch;
}

/**
 * Saves module changes one after the other, stopping at the first the server refuses. Whatever was
 * saved before that stays saved, and the lists are read again either way.
 */
export function useSaveModules(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (changes: ModuleChange[]) => {
      for (const { key, patch } of changes) {
        try {
          await api(`/api/guilds/${guild}/modules/${key}`, "PUT", patch);
        } catch (error) {
          throw error instanceof ApiError ? new ModuleSaveError(key, error) : error;
        }
      }
    },
    onSettled: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: modulesKey(guild) }),
        client.invalidateQueries({ queryKey: overviewKey(guild) }),
        client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] }),
      ]);
    },
  });
}

/** A module the server refused to save, with the refusal. */
export class ModuleSaveError extends Error {
  readonly key: string;
  readonly reason: ApiError;
  constructor(key: string, cause: ApiError) {
    super(cause.message);
    this.name = "ModuleSaveError";
    this.key = key;
    this.reason = cause;
  }
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

/** What the server calls its channels, roles and modules, for lists that would otherwise show ids. */
export function useNames(guild: string): Names {
  const discord = useDiscord(guild).data;
  const modules = useModules(guild).data;
  return useMemo(
    () => ({
      channel: (id) => {
        const channel = discord?.channels.find((c) => c.id === id);
        if (channel) return channelLabel(channel);
        return discord ? "a deleted channel" : plainNames.channel(id);
      },
      role: (id) => {
        const role = discord?.roles.find((r) => r.id === id);
        if (role) return role.everyone ? "@everyone" : `@${role.name}`;
        return discord ? "a deleted role" : plainNames.role(id);
      },
      module: (key) => modules?.items.find((m) => m.key === key)?.label ?? plainNames.module(key),
    }),
    [discord, modules],
  );
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
