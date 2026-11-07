// Data hooks. Query keys are the API paths.

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { channelLabel } from "../components/Pickers";
import { clearUser, configureUser } from "../i18n";

import {
  type AuditPage,
  type AutoResponder,
  type Case,
  type CaseDetail,
  type CasesPage,
  type Community,
  type GuildEntry,
  type CommandDef,
  type CustomCommand,
  type GuildInfo,
  type ImportReport,
  type Job,
  type LevelRoles,
  type LogSettings,
  type LogsPatch,
  type Me,
  type ModerationPatch,
  type ModerationSettings,
  type ModulePatch,
  type Modules,
  type Overview,
  type Preview,
  type RepRoles,
  type ResponderDef,
  type Settings,
  type SettingsPatch,
  type StaffPage,
  type Store,
  type TagList,
  type XpRules,
  ApiError,
  api,
  setCsrfToken,
} from "./api";
import { ATLAS_URL, type AtlasIndex } from "../components/card/CardSvg";
import type { Cards } from "./card";
import { type Names, plainNames } from "./text";

export function useMe(enabled = true) {
  return useQuery({
    queryKey: ["/api/me"],
    queryFn: async () => {
      const me = await api<Me>("/api/me");
      setCsrfToken(me.csrf);
      configureUser(me.user.id, me.language);
      return me;
    },
    enabled,
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
      clearUser();
      client.clear();
      window.location.assign("/login");
    },
  });
}

// /Moderation

export function useModeration(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/moderation`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<ModerationSettings>(path) });
  const save = useMutation({
    mutationFn: (patch: ModerationPatch) => api<ModerationSettings>(path, "PUT", patch),
    onSuccess: async (settings) => {
      client.setQueryData([path], settings);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
  return { query, save };
}

export function useLogs(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/logs`;
  const query = useQuery({ queryKey: [path], queryFn: () => api<LogSettings>(path) });
  const save = useMutation({
    mutationFn: (patch: LogsPatch) => api<LogSettings>(path, "PUT", patch),
    onSuccess: async (settings) => {
      client.setQueryData([path], settings);
      await client.invalidateQueries({ queryKey: overviewKey(guild) });
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
  return { query, save };
}

/** What the cases list is narrowed to. */
export interface CaseFilters {
  q: string;
  kind: string;
  member: string;
  active: boolean;
}

function search(params: Record<string, string | number | boolean | null | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== null && value !== undefined && value !== "" && value !== false)
      query.set(key, String(value));
  }
  const text = query.toString();
  return text ? `?${text}` : "";
}

export function useCases(guild: string, filters: CaseFilters) {
  return useInfiniteQuery({
    queryKey: [`/api/guilds/${guild}/cases`, filters],
    queryFn: ({ pageParam }) =>
      api<CasesPage>(`/api/guilds/${guild}/cases${search({ ...filters, before: pageParam || undefined })}`),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next ?? undefined,
  });
}

export function useCase(guild: string, number: number | null) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/cases/${number}`],
    queryFn: () => api<CaseDetail>(`/api/guilds/${guild}/cases/${number}`),
    enabled: number !== null,
  });
}

export function useSaveReason(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ number, reason }: { number: number; reason: string | null }) =>
      api<Case>(`/api/guilds/${guild}/cases/${number}`, "PATCH", { reason }),
    onSuccess: async (saved) => {
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/cases`] });
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/cases/${saved.number}`] });
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/staff-log`] });
    },
  });
}

/** What the staff log is narrowed to. */
export interface StaffFilters {
  q: string;
  source: string;
}

export function useStaffLog(guild: string, filters: StaffFilters) {
  return useInfiniteQuery({
    queryKey: [`/api/guilds/${guild}/staff-log`, filters],
    queryFn: ({ pageParam }) =>
      api<StaffPage>(
        `/api/guilds/${guild}/staff-log${search({ ...filters, before: pageParam || undefined })}`,
      ),
    initialPageParam: "",
    getNextPageParam: (last) => last.next ?? undefined,
  });
}

/** The welcome and goodbye images, the template gallery and the uploads (`GET /cards`). */
export function cardsKey(guild: string) {
  return [`/api/guilds/${guild}/cards`];
}

export function useCards(guild: string) {
  return useQuery({
    queryKey: cardsKey(guild),
    queryFn: () => api<Cards>(`/api/guilds/${guild}/cards`),
    staleTime: 60_000,
  });
}

/** Where every sprite sits in the sheet the bot draws from; ready once the sheet itself is decoded, so
 * every sticker and background shows at once. */
export function useAtlasIndex() {
  return useQuery({
    queryKey: ["/assets/atlas.json"],
    queryFn: async () => {
      const sheet = new Image();
      sheet.src = ATLAS_URL;
      const [response] = await Promise.all([
        fetch("/assets/atlas.json", { credentials: "same-origin" }),
        sheet.decode().catch(() => undefined),
      ]);
      if (!response.ok) throw new Error(`atlas.json: ${response.status}`);
      return (await response.json()) as AtlasIndex;
    },
    staleTime: Infinity,
    gcTime: Infinity,
  });
}
// /Custom commands and auto-responders

/** The two lists share every route shape: `/commands` and `/responders`. */
export type TagKind = "commands" | "responders";

interface TagTypes {
  commands: { item: CustomCommand; def: CommandDef };
  responders: { item: AutoResponder; def: ResponderDef };
}

export function tagsKey(guild: string, kind: TagKind) {
  return [`/api/guilds/${guild}/${kind}`];
}

/** A server's commands (or responders), with the most it may have. */
export function useTags<K extends TagKind>(guild: string, kind: K) {
  return useQuery({
    queryKey: tagsKey(guild, kind),
    queryFn: () => api<TagList<TagTypes[K]["item"]>>(`/api/guilds/${guild}/${kind}`),
  });
}

/** Creates (no `id`) or replaces (`id`) one, and reads the list and the audit log again. */
export function useSaveTag<K extends TagKind>(guild: string, kind: K) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, def }: { id?: string; def: TagTypes[K]["def"] }) =>
      id
        ? api<TagTypes[K]["item"]>(`/api/guilds/${guild}/${kind}/${id}`, "PUT", def)
        : api<TagTypes[K]["item"]>(`/api/guilds/${guild}/${kind}`, "POST", def),
    onSuccess: () => settled(client, guild, kind),
  });
}

export function useDeleteTag(guild: string, kind: TagKind) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api<{ deleted: string }>(`/api/guilds/${guild}/${kind}/${id}`, "DELETE"),
    onSuccess: () => settled(client, guild, kind),
  });
}

export function useToggleTag(guild: string, kind: TagKind) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api(`/api/guilds/${guild}/${kind}/${id}`, "PATCH", { enabled }),
    onSuccess: () => settled(client, guild, kind),
  });
}

async function settled(client: ReturnType<typeof useQueryClient>, guild: string, kind: TagKind) {
  await Promise.all([
    client.invalidateQueries({ queryKey: tagsKey(guild, kind) }),
    client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] }),
  ]);
}

/** One item by id, for the editor (the list has it too, but a link may open the editor first). */
export function useTag<K extends TagKind>(guild: string, kind: K, id: string | null) {
  return useQuery({
    queryKey: [...tagsKey(guild, kind), id],
    queryFn: () => api<TagTypes[K]["item"]>(`/api/guilds/${guild}/${kind}/${id}`),
    enabled: id !== null,
  });
}

/**
 * The message a draft would send, drawn by the bot's own code. `body` is the draft (and the arguments
 * for a command); `reroll` asks again for another random pick. A draft the bot would refuse comes back
 * as an error, shown where the preview would be.
 */
export function useTagPreview(guild: string, kind: TagKind, body: unknown, enabled: boolean, reroll = 0) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/${kind}/preview`, body, reroll],
    queryFn: () => api<Preview>(`/api/guilds/${guild}/${kind}/preview`, "POST", body),
    enabled,
    staleTime: Infinity,
    retry: false,
    placeholderData: (previous) => previous,
  });
}

export function useImportTags(guild: string, kind: TagKind) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: { version?: number; items: unknown[]; on_conflict?: "skip" | "replace" }) =>
      api<ImportReport>(`/api/guilds/${guild}/${kind}/import`, "POST", body),
    onSuccess: () => settled(client, guild, kind),
  });
}

/** The export file of a server's commands (or responders), as the server wrote it. */
export function exportTags(guild: string, kind: TagKind) {
  return api<{ version: number; items: unknown[] }>(`/api/guilds/${guild}/${kind}/export`);
}
