// Small wording helpers shared by the pages.

import type { AuditEntry, TagAction } from "./api";
import { describeAction } from "./tags";

/** "just now", "5 min ago", "3 h ago", "2 d ago", then the date. */
export function timeAgo(iso: string, now: Date = new Date()): string {
  const seconds = Math.max(0, Math.round((now.getTime() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(iso).toISOString().slice(0, 10);
}

/** Looks up what an id is called, so the log reads "#general" instead of a number. */
export interface Names {
  channel: (id: string) => string;
  role: (id: string) => string;
  module: (key: string) => string;
}

/** Names when nothing is known about the server: the id itself, worded as what it is. */
export const plainNames: Names = {
  channel: (id) => `channel ${id}`,
  role: (id) => `role ${id}`,
  module: (key) => key.replaceAll("_", " "),
};

/** What each settings field is called in the log. */
const FIELD_NAMES: Record<string, string> = {
  leveling_enabled: "leveling",
  levelup_channel_id: "the level-up channel",
  levelup_message: "the level-up message",
  levelup_mode: "level-up mode",
  level_roles_stack: "stacking level roles",
  voice_xp_enabled: "voice XP",
  voice_xp_amount: "voice XP a minute",
  welcome_enabled: "welcome cards",
  welcome_channel_id: "the welcome channel",
  welcome_message: "the welcome message",
  welcome_theme: "the welcome card theme",
  welcome_title: "the welcome card headline",
  welcome_accent: "the welcome card colour",
  join_role_id: "the join role",
  goodbye_channel_id: "the goodbye channel",
  goodbye_message: "the goodbye message",
  points_enabled: "points",
  points_name: "the points name",
  points_emoji: "the points emoji",
  points_min: "the least points a message earns",
  points_max: "the most points a message earns",
  log_channel_id: "the log channel",
  log_members: "join and leave logs",
  log_names: "name change logs",
  log_bans: "ban logs",
  log_voice: "voice logs",
  prefix: "the text command prefix",
};

/** What the fields of a custom command or an auto-responder are called in the log. */
const TAG_FIELDS: Record<string, string> = {
  name: "the name",
  description: "the description",
  responses: "the replies",
  embed: "the embed",
  reply_mode: "who sees the answer",
  actions: "the actions",
  allow_roles: "the roles that may use it",
  deny_roles: "the roles that may not",
  allow_channels: "the channels it works in",
  deny_channels: "the channels it doesn't work in",
  cooldown_secs: "the cooldown",
  enabled: "the switch",
  allow_mentions: "mentions",
  trigger_kind: "the trigger",
  trigger_value: "the trigger's word, level or role",
  match_kind: "how it matches",
  destination: "where it answers",
  channel_id: "the channel it answers in",
  chance: "the chance",
};

/** Fields whose meaning depends on the action that wrote them. */
const ACTION_FIELDS: Record<string, Record<string, string>> = {
  "critters.set": { enabled: "critter drops", channel_id: "the critter channel" },
  "suggestions.set": { channel_id: "the suggestion channel" },
  "surge.set": {
    enabled: "timed storms",
    channel_id: "the storm channel",
    every_hours: "how often storms come",
  },
  "module.set": { enabled: "the switch", channel_mode: "where it works", channels: "the channels" },
  "level_roles.put": { level: "the level" },
  "rep_roles.put": { reputation: "the reputation" },
  "xp_rules.put": { multiplier: "the multiplier" },
  "store.put": { price: "the price", stock: "the stock" },
  "card.put": { delivery: "how it is sent", design: "the design" },
  "command.update": TAG_FIELDS,
  "responder.update": TAG_FIELDS,
};

/** What a field is called, in words. */
export function fieldLabel(action: string, field: string): string {
  return ACTION_FIELDS[action]?.[field] ?? FIELD_NAMES[field] ?? field.replaceAll("_", " ");
}

/** What list writes did, by audit action. */
const ACTIONS: Record<string, string> = {
  "xp_rules.put": "set an XP multiplier",
  "xp_rules.delete": "removed an XP multiplier",
  "xp_rules.clear": "cleared the XP multipliers",
  "level_roles.put": "set a level role",
  "level_roles.delete": "removed a level role",
  "level_roles.sync": "synced the level roles",
  "rep_roles.put": "set a reputation role",
  "rep_roles.delete": "removed a reputation role",
  "store.put": "put a role on sale",
  "store.delete": "took a role off sale",
  "suggestions.set": "changed the suggestion channel",
  "critters.set": "changed critter drops",
  "card.put": "saved an image design",
  "card.upload": "uploaded a background picture",
};

const MODES: Record<string, string> = {
  all: "all channels",
  only: "only some channels",
  except: "all but some channels",
};

/** One value from an audit row, as a person would say it: ids become names, flags on or off. */
export function showValue(field: string, value: unknown, names: Names = plainNames): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "boolean") return value ? "on" : "off";
  if (field === "channels" && Array.isArray(value)) {
    return value.length === 0 ? "none" : value.map((id) => names.channel(String(id))).join(", ");
  }
  if (field === "embed") return "an embed";
  if (Array.isArray(value)) {
    if (value.length === 0) return "none";
    if (field.endsWith("_roles")) return value.map((id) => names.role(String(id))).join(", ");
    if (field.endsWith("_channels")) return value.map((id) => names.channel(String(id))).join(", ");
    if (field === "actions") return value.map((a) => describeAction(a as TagAction)).join(", ");
    if (value.every((v) => typeof v === "string")) return value.map((v) => `“${v}”`).join(" · ");
  }
  if (field === "channel_mode" && typeof value === "string") return MODES[value] ?? value;
  if (field === "design" && typeof value === "object") {
    const design = value as { width?: number; height?: number; layers?: unknown[] };
    return `${design.width}×${design.height}, ${design.layers?.length ?? 0} layers`;
  }
  if (field === "design" && value === "classic") return "the classic card";
  if (typeof value === "string" && /^\d+$/.test(value)) {
    if (field.endsWith("channel_id")) return names.channel(value);
    if (field.endsWith("role_id")) return names.role(value);
  }
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return JSON.stringify(value);
}

/** What an audit row was done to: a role, a channel, a module; `null` when it was the server itself. */
export function targetLabel(entry: AuditEntry, names: Names = plainNames): string | null {
  const target = entry.target;
  if (!target) return null;
  if (entry.action === "module.set") return names.module(target);
  if (entry.action === "card.upload") return null;
  // Custom commands and responders are named in the sentence itself.
  if (/^(command|responder)\./.test(entry.action)) return null;
  const [kind, id] = target.split(":");
  if (id && (kind === "channel" || kind === "role"))
    return kind === "channel" ? names.channel(id) : names.role(id);
  if (/^\d+$/.test(target)) return names.role(target);
  return target;
}

/** What an audit row did, as a sentence after the actor's name. */
export function describeChange(entry: AuditEntry, names: Names = plainNames): string {
  if (entry.action === "module.set" && entry.target) return describeModule(entry, names);
  const tag = describeTag(entry);
  if (tag) return tag;
  const action = ACTIONS[entry.action];
  if (action) return action;
  const after = entry.after ?? {};
  const parts = Object.entries(after).map(([field, value]) => {
    const name = fieldLabel(entry.action, field);
    if (typeof value === "boolean") return `turned ${name} ${value ? "on" : "off"}`;
    return `changed ${name}`;
  });
  if (parts.length === 0) return `made a change (${entry.action})`;
  if (parts.length > 3) return `${parts.slice(0, 2).join(", ")} and ${parts.length - 2} more`;
  return parts.join(", ");
}

/** Custom commands and auto-responders: "wrote /tag rules", "changed /tag rules: the replies, the cooldown". */
function describeTag(entry: AuditEntry): string | null {
  const after = entry.after ?? {};
  const name = entry.target ?? "";
  const count = (key: string) => (Array.isArray(after[key]) ? (after[key] as unknown[]).length : 0);
  const [kind] = entry.action.split(".");
  if (kind !== "command" && kind !== "responder" && kind !== "commands" && kind !== "responders") return null;
  const what = kind.startsWith("command") ? `/tag ${name}` : `the auto-responder “${name}”`;
  switch (entry.action) {
    case "command.create":
      return `wrote the custom command ${what}`;
    case "responder.create":
      return `wrote ${what}`;
    case "command.delete":
    case "responder.delete":
      return `deleted ${what}`;
    case "commands.import":
    case "responders.import": {
      const noun = kind === "commands" ? "custom command" : "auto-responder";
      const added = count("created");
      const replaced = count("replaced");
      const parts = [added > 0 && `${added} new`, replaced > 0 && `${replaced} replaced`].filter(Boolean);
      return `imported ${noun}s (${parts.join(", ") || "none"})`;
    }
    case "command.update":
    case "responder.update": {
      if (typeof after.enabled === "boolean" && Object.keys(after).length === 1) {
        return `turned ${what} ${after.enabled ? "on" : "off"}`;
      }
      const fields = Object.keys(after).map((f) => fieldLabel(entry.action, f));
      return `changed ${what}: ${fields.join(", ")}`;
    }
    default:
      return null;
  }
}

/** "turned Pets off", "kept Pets to 2 channels", "moved Pets back to all channels". */
function describeModule(entry: AuditEntry, names: Names): string {
  const label = names.module(entry.target ?? "");
  const after = entry.after ?? {};
  const said: string[] = [];
  if (typeof after.enabled === "boolean") said.push(`turned ${label} ${after.enabled ? "on" : "off"}`);
  if (typeof after.channel_mode === "string" || Array.isArray(after.channels)) {
    const count = Array.isArray(after.channels) ? after.channels.length : 0;
    const noun = count === 1 ? "channel" : "channels";
    if (after.channel_mode === "all") said.push(`opened ${label} to all channels`);
    else if (after.channel_mode === "only") said.push(`limited ${label} to ${count} ${noun}`);
    else if (after.channel_mode === "except") said.push(`switched ${label} off in ${count} ${noun}`);
    else said.push(`changed the channels of ${label}`);
  }
  return said.length > 0 ? said.join(" and ") : `changed ${label}`;
}

/** One or two letters for a server without an icon. */
export function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean);
  const letters = words.length > 1 ? words[0]![0]! + words[1]![0]! : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}

/** A piece of a message as Discord shows it. */
export interface RichPart {
  kind: "text" | "bold" | "italic" | "code";
  text: string;
}

// An escape (a backslash before punctuation) comes first, so an escaped `*` never opens bold or italic.
const RICH = /(\\[!-/:-@[-`{-~]|\*\*[^*]+\*\*|`[^`]+`|\*[^*\s][^*]*\*)/g;

/** The bold, italic and code of a message, with Discord's backslash escapes taken out. */
export function richParts(text: string): RichPart[] {
  const parts: RichPart[] = [];
  const push = (kind: RichPart["kind"], piece: string) => {
    const last = parts[parts.length - 1];
    if (kind === "text" && last?.kind === "text") last.text += piece;
    else if (piece) parts.push({ kind, text: piece });
  };
  for (const part of text.split(RICH)) {
    if (part.length === 2 && part.startsWith("\\")) push("text", part.slice(1));
    else if (part.startsWith("**") && part.endsWith("**") && part.length > 4) push("bold", part.slice(2, -2));
    else if (part.startsWith("`") && part.endsWith("`") && part.length > 2) push("code", part.slice(1, -1));
    else if (part.startsWith("*") && part.endsWith("*") && part.length > 2) push("italic", part.slice(1, -1));
    else push("text", part);
  }
  return parts;
}
