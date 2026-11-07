import type { FeedWords } from "./feedsWords";
// The Feeds page: its shapes, its requests, and the small rules the form follows before the server
// checks everything again.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, type Preview } from "./api";

export type FeedKind = "youtube" | "rss" | "reddit" | "bluesky" | "bridge" | "twitch" | "kick";

export type HealthState = "ok" | "waiting" | "failing" | "cannot_post" | "off";

export interface FeedHealth {
  state: HealthState;
  /** One sentence for the list. */
  text: string;
  last_checked_at: string | null;
  last_item_at: string | null;
  last_sent_at: string | null;
  next_poll_at: string;
  fail_count: number;
  interval_minutes: number;
}

export interface Feed {
  id: string;
  kind: FeedKind;
  source: string;
  /** How the source is named: the channel, the feed's title, r/name, the account. */
  account: string;
  link: string | null;
  channel_id: string;
  /** `null`: the kind's default message. */
  message: string | null;
  embed: boolean;
  ping_role_id: string | null;
  live_role_id: string | null;
  live_message_id: string | null;
  filter_include: string[];
  filter_exclude: string[];
  enabled: boolean;
  created_at: string;
  health: FeedHealth;
}

export interface KindInfo {
  key: FeedKind;
  label: string;
  available: boolean;
  why_not: string | null;
  default_message: string;
  interval_minutes: number;
}

export interface FeedList {
  strings: Record<string, string>;
  items: Feed[];
  max: number;
  message_max: number;
  filter_words_max: number;
  filter_word_max: number;
  kinds: KindInfo[];
  placeholders: { token: string; hint: string }[];
  /** The id of the feed a create just made. */
  created?: string;
}

export interface FoundItem {
  title: string;
  url: string | null;
  author: string | null;
  published: string | null;
  image: string | null;
  summary: string | null;
}

/** What a check found: the source as it will be stored, its name and its latest post. */
export interface Found {
  kind: FeedKind;
  source: string;
  account: string;
  title: string | null;
  link: string | null;
  latest: FoundItem | null;
  interval_minutes: number;
}

/** What the form edits. Ids are strings; `null` message is the default, `null` role pings nobody. */
export interface FeedDraft {
  kind: FeedKind;
  source: string;
  channel_id: string | null;
  message: string | null;
  embed: boolean;
  ping_role_id: string | null;
  live_role_id: string | null;
  filter_include: string;
  filter_exclude: string;
  enabled: boolean;
}

/** Catalog keys for the source fields, by kind; URL examples are language-independent. */
export const SOURCE_HELP: Record<FeedKind, { label: string; placeholder: string; hint: string }> = {
  youtube: {
    label: "youtube-channel",
    placeholder: "handle-or-youtube-com-channel-uc",
    hint: "the-channel-s-link-its-handle-or-its",
  },
  rss: {
    label: "feed-address",
    placeholder: "https://example.com/feed.xml",
    hint: "an-rss-or-atom-address-blogs-news-sites",
  },
  reddit: {
    label: "subreddit",
    placeholder: "r/aww",
    hint: "the-newest-posts-of-a-public-subreddit-about",
  },
  bluesky: {
    label: "account",
    placeholder: "name.bsky.social",
    hint: "a-handle-or-a-profile-link-posts-show",
  },
  bridge: {
    label: "bridge-route",
    placeholder: "twitter/user/name",
    hint: "the-path-your-rss-bridge-serves-for-an",
  },
  twitch: { label: "account", placeholder: "twitch.tv/streamer", hint: "twitch-live-help" },
  kick: { label: "account", placeholder: "kick.com/streamer", hint: "kick-live-help" },
};

/** A short tag for each kind, for the list. */
export const KIND_TAG: Record<FeedKind, string> = {
  youtube: "YT",
  rss: "RSS",
  reddit: "RED",
  bluesky: "SKY",
  bridge: "BRG",
  twitch: "TTV",
  kick: "KCK",
};

export function blankDraft(kind: FeedKind = "youtube"): FeedDraft {
  return {
    kind,
    source: "",
    channel_id: null,
    message: null,
    embed: kind !== "youtube",
    ping_role_id: null,
    live_role_id: null,
    filter_include: "",
    filter_exclude: "",
    enabled: true,
  };
}

export function draftOf(feed: Feed): FeedDraft {
  return {
    kind: feed.kind,
    source: feed.source,
    channel_id: feed.channel_id,
    message: feed.message,
    embed: feed.embed,
    ping_role_id: feed.ping_role_id,
    live_role_id: feed.live_role_id,
    filter_include: feed.filter_include.join(", "),
    filter_exclude: feed.filter_exclude.join(", "),
    enabled: feed.enabled,
  };
}

/** Filter words from what was typed: comma-separated, trimmed, lower case, no repeats. */
export function words(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(",")) {
    const word = raw.trim().toLowerCase();
    if (word && !out.includes(word)) out.push(word);
  }
  return out;
}

/** Why a filter list can't be saved, in the form's words, or `null`. */
export function wordsProblem(text: string, max: number, longest: number, t: FeedWords): string | null {
  const list = words(text);
  if (list.length > max) return t("at-most-max-words", { max });
  const long = list.find((w) => [...w].length > longest);
  return long ? t("word-is-longer-than-max-characters", { word: long, max: longest }) : null;
}

/** The fields that differ between two drafts. */
export function changedFields(saved: FeedDraft, draft: FeedDraft): (keyof FeedDraft)[] {
  return (Object.keys(draft) as (keyof FeedDraft)[]).filter((key) => draft[key] !== saved[key]);
}

/** The body of a save: everything for a new feed, only what changed for an edit (kind and source never). */
export function bodyOf(draft: FeedDraft, saved: FeedDraft | null): Record<string, unknown> {
  const message = draft.message === null || draft.message.trim() === "" ? null : draft.message;
  const full: Record<string, unknown> = {
    channel_id: draft.channel_id,
    message,
    embed: draft.embed,
    ping_role_id: draft.ping_role_id,
    live_role_id: draft.live_role_id,
    filter_include: words(draft.filter_include),
    filter_exclude: words(draft.filter_exclude),
    enabled: draft.enabled,
  };
  if (!saved) return { kind: draft.kind, source: draft.source.trim(), ...full };
  const changed = changedFields(saved, draft);
  const body: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(full)) {
    if (changed.includes(key as keyof FeedDraft)) body[key] = value;
  }
  return body;
}

/** "3 min ago", "2 h ago", "4 days ago"; `null` reads as "not yet". */
export function ago(iso: string | null, t: FeedWords, now: number = Date.now()): string {
  if (!iso) return t("not-yet");
  const secs = Math.max(0, Math.round((now - new Date(iso).getTime()) / 1000));
  if (secs < 60) return t("just-now");
  const mins = Math.round(secs / 60);
  if (mins < 60) return t("count-min-ago", { count: mins });
  const hours = Math.round(mins / 60);
  if (hours < 48) return t("count-h-ago", { count: hours });
  return t("count-days-ago", { count: Math.round(hours / 24) });
}

// /Requests

export function feedsKey(guild: string) {
  return [`/api/guilds/${guild}/feeds`];
}

export function useFeeds(guild: string) {
  return useQuery({
    queryKey: feedsKey(guild),
    queryFn: () => api<FeedList>(`/api/guilds/${guild}/feeds`),
  });
}

async function settled(client: ReturnType<typeof useQueryClient>, guild: string, list?: FeedList) {
  if (list) client.setQueryData(feedsKey(guild), list);
  await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
}

/** Creates (no `id`) or changes (`id`) a feed; answers with the list. */
export function useSaveFeed(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id?: string; body: Record<string, unknown> }) =>
      id
        ? api<FeedList>(`/api/guilds/${guild}/feeds/${id}`, "PUT", body)
        : api<FeedList>(`/api/guilds/${guild}/feeds`, "POST", body),
    onSuccess: (list) => settled(client, guild, list),
  });
}

export function useDeleteFeed(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api<FeedList>(`/api/guilds/${guild}/feeds/${id}`, "DELETE"),
    onSuccess: (list) => settled(client, guild, list),
  });
}

export function useCheckFeed(guild: string) {
  return useMutation({
    mutationFn: (body: { kind: FeedKind; source: string }) =>
      api<Found>(`/api/guilds/${guild}/feeds/check`, "POST", body),
  });
}

export function useTestFeed(guild: string) {
  return useMutation({
    mutationFn: (id: string) =>
      api<{ posted: boolean; channel_id: string }>(`/api/guilds/${guild}/feeds/${id}/test`, "POST", {}),
  });
}

/** The post a draft would make, drawn by the bot's own code (nothing is fetched or sent). */
export function useFeedPreview(guild: string, body: unknown, enabled: boolean) {
  return useQuery({
    queryKey: [`/api/guilds/${guild}/feeds/preview`, body],
    queryFn: () => api<Preview>(`/api/guilds/${guild}/feeds/preview`, "POST", body),
    enabled,
    staleTime: Infinity,
    retry: false,
    placeholderData: (previous) => previous,
  });
}

export interface MyChannel {
  kind: "twitch" | "kick";
  source: string;
  code: string | null;
  expires_at: string | null;
  verified_at: string | null;
}

export interface MyChannelList {
  items: MyChannel[];
  kinds: { key: "twitch" | "kick"; available: boolean; why_not: string | null }[];
  strings: Record<string, string>;
}

const myChannelsKey = ["/api/feeds/my-channels"];
export function useMyChannels() {
  return useQuery({
    queryKey: myChannelsKey,
    queryFn: () => api<MyChannelList>("/api/feeds/my-channels"),
  });
}
export function useStartChannel() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: { kind: "twitch" | "kick"; source: string }) =>
      api<MyChannel>("/api/feeds/my-channels", "POST", body),
    onSuccess: () => client.invalidateQueries({ queryKey: myChannelsKey }),
  });
}
export function useVerifyChannel() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (kind: "twitch" | "kick") =>
      api<{ verified: boolean }>(`/api/feeds/my-channels/${kind}/verify`, "POST", {}),
    onSuccess: () => client.invalidateQueries({ queryKey: myChannelsKey }),
  });
}
export function useUnlinkChannel() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (kind: "twitch" | "kick") =>
      api<{ removed: boolean }>(`/api/feeds/my-channels/${kind}`, "DELETE"),
    onSuccess: () => client.invalidateQueries({ queryKey: myChannelsKey }),
  });
}
