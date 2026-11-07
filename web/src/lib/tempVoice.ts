// Temp voice: the creators a server set up and the rooms alive right now. The names a creator gives its
// rooms are checked here the way the bot checks them (crates/pinchora-core/src/temp_voice.rs).

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "./api";
import { t, tf } from "./tempVoiceI18n";

export interface Creator {
  channel_id: string;
  category_id: string | null;
  name_template: string;
  default_limit: number;
  default_locked: boolean;
  max_per_user: number;
  panel: boolean;
  panel_channel_id: string | null;
  enabled: boolean;
  /** Rooms this creator made that are alive now. */
  rooms: number;
}

export interface Room {
  channel_id: string;
  source_id: string;
  owner_id: string | null;
  owner_name: string | null;
  created_at: string;
  locked: boolean;
  hidden: boolean;
  user_limit: number;
  /** People sitting in it, as the bot's voice tracking has it. */
  members: number;
}

export interface TempVoice {
  /** The Temp voice module's switch. */
  enabled: boolean;
  creators: Creator[];
  rooms: Room[];
  /** Permissions the bot lacks that rooms need, by Discord's names; `null` when Discord would not say. */
  missing: string[] | null;
  limits: {
    max_creators: number;
    max_rooms: number;
    max_per_user: number;
    max_template: number;
    max_limit: number;
    default_template: string;
  };
}

/** What `PUT /temp-voice/creators/{channel}` takes: any subset. */
export type CreatorPatch = Partial<Omit<Creator, "channel_id" | "rooms">>;

export const tempVoiceKey = (guild: string) => [`/api/guilds/${guild}/temp-voice`];

/** The page's data, asked for again every so often: the list of rooms is live. */
export function useTempVoice(guild: string) {
  return useQuery({
    queryKey: tempVoiceKey(guild),
    queryFn: () => api<TempVoice>(`/api/guilds/${guild}/temp-voice`),
    refetchInterval: 15_000,
  });
}

export function useSaveCreator(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ channel, patch }: { channel: string; patch: CreatorPatch }) =>
      api<TempVoice>(`/api/guilds/${guild}/temp-voice/creators/${channel}`, "PUT", patch),
    onSuccess: async (data) => {
      client.setQueryData(tempVoiceKey(guild), data);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}

export function useRemoveCreator(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (channel: string) =>
      api<TempVoice>(`/api/guilds/${guild}/temp-voice/creators/${channel}`, "DELETE"),
    onSuccess: async (data) => {
      client.setQueryData(tempVoiceKey(guild), data);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}

// /Names

export const MAX_NAME = 100;
export const MAX_TEMPLATE = 80;
export const PLACEHOLDERS = ["user", "n"] as const;
export const FALLBACK_NAME = "Voice room";
const DEFAULT_TEMPLATE = "{user}'s room";

/** Whitespace collapsed to single spaces, control characters dropped. */
function tidy(text: string): string {
  return text
    .replace(/\s/g, " ")
    .replace(/\p{Cc}/gu, "")
    .replaceAll("@", "＠")
    .replaceAll("<", "‹")
    .replaceAll(">", "›")
    .split(" ")
    .filter(Boolean)
    .join(" ");
}

/** The first `max` characters (code points, as Discord counts them), never empty. */
function clip(name: string): string {
  const cut = [...name].slice(0, MAX_NAME).join("").trim();
  return cut === "" ? t(FALLBACK_NAME) : cut;
}

/** What a room is called: the owner's name where `{user}` is, the room's number where `{n}` is. */
export function roomName(template: string, owner: string, number: number): string {
  const shown = template === DEFAULT_TEMPLATE ? t(template) : template;
  return clip(tidy(shown.replaceAll("{n}", String(number)).replaceAll("{user}", owner)));
}

/** Why a name template is not good enough, as the bot would say it, or `null` when it is fine. */
export function templateProblem(raw: string): string | null {
  const template = tidy(raw);
  if (template === "") return t("Give the rooms a name, like {user}'s room.");
  if ([...template].length > MAX_TEMPLATE)
    return tf("A name template is at most {max} characters.", { max: MAX_TEMPLATE });
  for (const match of template.matchAll(/\{([^{}]*)\}/g)) {
    const word = match[1] ?? "";
    if (!(PLACEHOLDERS as readonly string[]).includes(word)) {
      return tf("{word} isn't one of the words a name can use: {user} and {n}.", {
        word: `{${word}}`,
        user: "{user}",
        n: "{n}",
      });
    }
  }
  const open = template.replace(/\{[^{}]*\}/g, "");
  if (open.includes("{") || open.includes("}")) return t("There is a { without its }.");
  return null;
}

/** How long ago, in the units a person says it: "just now", "12 min ago", "3 h ago", "2 d ago". */
export function ago(iso: string, now: number = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - Date.parse(iso)) / 1000));
  if (seconds < 45) return t("just now");
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return tf("{count} min ago", { count: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 48) return tf("{count} h ago", { count: hours });
  return tf("{count} d ago", { count: Math.round(hours / 24) });
}
