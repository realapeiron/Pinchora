// Small wording helpers shared by the pages.

import type { AuditEntry } from "./api";

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

const FIELD_NAMES: Record<string, string> = {
  leveling_enabled: "leveling",
  voice_xp_enabled: "voice XP",
  welcome_enabled: "welcome cards",
  points_enabled: "points",
  level_roles_stack: "stacking level roles",
  log_members: "join and leave logs",
  log_names: "name change logs",
  log_bans: "ban logs",
  log_voice: "voice logs",
  enabled: "critter drops",
};

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
};

/** What an audit row did, as a sentence after the actor's name. */
export function describeChange(entry: AuditEntry): string {
  const action = ACTIONS[entry.action];
  if (action) return action;
  const after = entry.after ?? {};
  const parts = Object.entries(after).map(([field, value]) => {
    const name = FIELD_NAMES[field] ?? field.replaceAll("_", " ");
    if (typeof value === "boolean") return `turned ${name} ${value ? "on" : "off"}`;
    return `changed ${name}`;
  });
  if (parts.length === 0) return `made a change (${entry.action})`;
  if (parts.length > 3) return `${parts.slice(0, 2).join(", ")} and ${parts.length - 2} more`;
  return parts.join(", ");
}

/** One or two letters for a server without an icon. */
export function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean);
  const letters = words.length > 1 ? words[0]![0]! + words[1]![0]! : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}
