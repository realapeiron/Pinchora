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
  welcome_enabled: "welcome messages",
  points_enabled: "points",
};

/** What an audit row did, as a sentence after the actor's name. */
export function describeChange(entry: AuditEntry): string {
  const after = entry.after ?? {};
  const parts = Object.entries(after).map(([field, value]) => {
    const name = FIELD_NAMES[field] ?? field.replaceAll("_", " ");
    if (typeof value === "boolean") return `turned ${name} ${value ? "on" : "off"}`;
    return `changed ${name}`;
  });
  if (parts.length === 0) return `made a change (${entry.action})`;
  return parts.join(", ");
}

/** One or two letters for a server without an icon. */
export function initials(name: string): string {
  const words = name.split(/\s+/).filter(Boolean);
  const letters = words.length > 1 ? words[0]![0]! + words[1]![0]! : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}
