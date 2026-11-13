import { t as deferredText } from "../../../i18n/dashboard";
// Pure helpers of the moderation pages: what a staff log line says, and what a draft of the log or
// moderation settings changes.

import type {
  LadderAction,
  LogSettings,
  LogsPatch,
  ModerationPatch,
  ModerationSettings,
  StaffEntry,
} from "../../../lib/api";
import { translate } from "../../../i18n";

/** What a staff action did, after the actor's name. */
const DID: Record<string, string> = {
  ban: "banned",
  unban: "unbanned",
  kick: "kicked",
  timeout: "timed out",
  untimeout: "ended the timeout of",
  warn: "warned",
  unwarn: "took back a warn of",
  purge: "purged messages in",
  slowmode: "set slowmode in",
  lock: "locked",
  unlock: "unlocked",
  nick: "renamed",
  role_add: "gave a role to",
  role_remove: "took a role from",
  member_roles: "changed the roles of",
  member_update: "edited",
  reason: "gave a new reason to",
  prune: "pruned members",
  voice_move: "moved someone in voice",
  voice_disconnect: "disconnected someone from voice",
  bot_add: "added a bot:",
  server_update: "changed the server settings",
  channel_create: "created the channel",
  channel_update: "changed the channel",
  channel_delete: "deleted the channel",
  channel_permissions: "changed permissions in",
  role_create: "created the role",
  role_update: "changed the role",
  role_delete: "deleted the role",
  invite_create: "made an invite",
  invite_delete: "deleted an invite",
  webhook_create: "made a webhook",
  webhook_update: "changed a webhook",
  webhook_delete: "deleted a webhook",
  emojis: "changed an emoji:",
  stickers: "changed a sticker:",
  message_delete: "deleted a message by",
  message_bulk_delete: "bulk deleted messages in",
  thread_create: "started the thread",
  thread_update: "changed the thread",
  thread_delete: "deleted the thread",
};

/** What a dashboard change did, by its audit action. */
const CHANGED: Record<string, string> = {
  "settings.patch": "changed the settings",
  "module.set": "changed a module:",
  "moderation.set": "changed the moderation settings",
  "logs.set": "changed the server log",
  "level_roles.put": "set a level role",
  "level_roles.delete": "removed a level role",
  "level_roles.sync": "synced the level roles",
  "rep_roles.put": "set a reputation role",
  "rep_roles.delete": "removed a reputation role",
  "xp_rules.put": "set an XP multiplier",
  "xp_rules.delete": "removed an XP multiplier",
  "xp_rules.clear": "cleared the XP multipliers",
  "store.put": "put a role on sale",
  "store.delete": "took a role off sale",
  "suggestions.set": "changed the suggestion channel",
  "critters.set": "changed critter drops",
  "surge.set": "changed timed storms",
};

/** Where an action was made, in words. */
export const SOURCES: Record<StaffEntry["source"], string> = {
  command: "with Pinchora",
  discord: "in Discord",
  web: "on the dashboard",
};

/** "banned Mia", "changed the channel lobby", "changed the server settings". */
export function staffLine(entry: StaffEntry): string {
  if (entry.source === "web" && !DID[entry.action]) {
    const changed = translate(
      CHANGED[entry.action] ?? deferredText("changed the settings ({arg1})", { arg1: entry.action }),
    );
    return changed.endsWith(":") && entry.target_name
      ? `${changed} ${entry.target_name}`
      : changed.replace(/:$/, "");
  }
  const did = translate(DID[entry.action] ?? entry.action.replaceAll("_", " "));
  return entry.target_name ? `${did} ${entry.target_name}` : did;
}

/** A rung as the ladder editor keeps it. */
export interface DraftRung {
  warns: number;
  action: LadderAction;
  duration: string;
}

export function draftLadder(saved: ModerationSettings): DraftRung[] {
  return saved.ladder.map((r) => ({ warns: r.warns, action: r.action, duration: r.duration ?? "" }));
}

/** The moderation settings a draft changes, and how many things that is. */
export function moderationPatch(
  saved: ModerationSettings,
  channel: string | null,
  dm: boolean,
  ladder: DraftRung[],
): { patch: ModerationPatch; count: number } {
  const patch: ModerationPatch = {};
  let count = 0;
  if (channel !== saved.log_channel_id) {
    patch.log_channel_id = channel;
    count += 1;
  }
  if (dm !== saved.dm_members) {
    patch.dm_members = dm;
    count += 1;
  }
  const before = JSON.stringify(draftLadder(saved));
  const after = JSON.stringify(ladder);
  if (before !== after) {
    patch.ladder = ladder.map((r) => ({
      warns: r.warns,
      action: r.action,
      duration: r.duration.trim() || null,
    }));
    count += 1;
  }
  return { patch, count };
}

/** The log settings as the page edits them. */
export interface LogDraft {
  channel_id: string | null;
  category_channels: Record<string, string | null>;
  events: Record<string, boolean>;
  ignored_channels: string[];
  ignored_roles: string[];
  ignored_users: string[];
  ignore_bots: boolean;
}

export function logDraft(saved: LogSettings): LogDraft {
  const category_channels: Record<string, string | null> = {};
  const events: Record<string, boolean> = {};
  for (const category of saved.categories) {
    category_channels[category.key] = category.channel_id;
    for (const event of category.events) events[event.key] = event.enabled;
  }
  return {
    channel_id: saved.channel_id,
    category_channels,
    events,
    ignored_channels: saved.ignored_channels,
    ignored_roles: saved.ignored_roles,
    ignored_users: saved.ignored_users,
    ignore_bots: saved.ignore_bots,
  };
}

const sameList = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/** Only what the draft changes, and how many changes that is. */
export function logsPatch(saved: LogSettings, draft: LogDraft): { patch: LogsPatch; count: number } {
  const now = logDraft(saved);
  const patch: LogsPatch = {};
  let count = 0;
  if (draft.channel_id !== now.channel_id) {
    patch.channel_id = draft.channel_id;
    count += 1;
  }
  for (const [key, channel] of Object.entries(draft.category_channels)) {
    if (channel !== now.category_channels[key]) {
      patch.category_channels = { ...patch.category_channels, [key]: channel };
      count += 1;
    }
  }
  for (const [key, on] of Object.entries(draft.events)) {
    if (on !== now.events[key]) {
      patch.events = { ...patch.events, [key]: on };
      count += 1;
    }
  }
  for (const list of ["ignored_channels", "ignored_roles", "ignored_users"] as const) {
    if (!sameList(draft[list], now[list])) {
      patch[list] = draft[list];
      count += 1;
    }
  }
  if (draft.ignore_bots !== now.ignore_bots) {
    patch.ignore_bots = draft.ignore_bots;
    count += 1;
  }
  return { patch, count };
}
