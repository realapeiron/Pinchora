import { translate as spaTranslate } from "../i18n";
// The roles page's data: the shapes the API answers with, the hooks that read and write them, and the
// small rules the forms check before they ask (the server checks them again, with the command's sentences).

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "./api";

export type AutoKind = "join" | "bot" | "after_verify";
export type PanelStyle = "buttons" | "select" | "reactions";
export type ColorMode = "palette" | "hex";

export interface AutoRole {
  role_id: string;
  kind: AutoKind;
  delay_secs: number;
  remove_after_secs: number;
  enabled: boolean;
}

export interface PanelRole {
  role_id: string;
  label: string;
  emoji: string | null;
  description: string | null;
  temp_secs: number | null;
}

export interface Panel {
  id: string;
  title: string;
  description: string | null;
  style: PanelStyle;
  exclusive: boolean;
  required_role: string | null;
  min_picks: number;
  max_picks: number;
  channel_id: string | null;
  message_id: string | null;
  roles: PanelRole[];
}

export interface PaletteColor {
  name: string;
  /** `#rrggbb`. */
  hex: string;
}

export interface ColorRole {
  role_id: string;
  hex: string;
  name: string;
}

export interface RolesView {
  auto: {
    items: AutoRole[];
    max: number;
    sticky: boolean;
    max_delay_secs: number;
    max_remove_after_secs: number;
  };
  panels: { items: Panel[]; max_roles: number };
  colors: {
    enabled: boolean;
    mode: ColorMode;
    allowed_roles: string[];
    anchor_role_id: string | null;
    roles: ColorRole[];
    palette: PaletteColor[];
    max: number;
    max_allowed: number;
  };
}

/** What a post or a "create missing" adds to the page's answer. */
export interface RolesAnswer extends RolesView {
  posted?: {
    channel_id: string;
    message_id: string;
    url: string;
    updated: boolean;
    moved_from: string | null;
    unusable: string[];
  };
  report?: { created: string[]; existing: number; stopped: string | null };
}

export const rolesPath = (guild: string) => `/api/guilds/${guild}/roles`;

export function useRoles(guild: string) {
  return useQuery({ queryKey: [rolesPath(guild)], queryFn: () => api<RolesView>(rolesPath(guild)) });
}

/** Every write answers with the whole page again, which replaces what is cached. */
export function useRolesWrite(guild: string) {
  const client = useQueryClient();
  const path = rolesPath(guild);
  return useMutation({
    mutationFn: ({ method, sub, body }: { method: "PUT" | "POST" | "DELETE"; sub: string; body?: unknown }) =>
      api<RolesAnswer>(path + sub, method, body),
    onSuccess: async (answer) => {
      const view: RolesAnswer = { ...answer };
      delete view.posted;
      delete view.report;
      client.setQueryData<RolesView>([path], view);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}

// /Times

export type Unit = "m" | "h" | "d";
export const UNITS: { unit: Unit; secs: number; name: string }[] = [
  { unit: "m", secs: 60, name: "minutes" },
  { unit: "h", secs: 3_600, name: "hours" },
  { unit: "d", secs: 86_400, name: "days" },
];

/** A time as the biggest whole unit it is: 7200 is 2 hours. 0 is none. */
export function splitSpan(secs: number): { amount: number; unit: Unit } {
  if (secs <= 0) return { amount: 0, unit: "h" };
  for (const { unit, secs: size } of [...UNITS].reverse()) {
    if (secs % size === 0) return { amount: secs / size, unit };
  }
  return { amount: Math.round(secs / 60), unit: "m" };
}

export function joinSpan(amount: number, unit: Unit): number {
  const size = UNITS.find((u) => u.unit === unit)?.secs ?? 60;
  return Math.max(0, Math.round(amount)) * size;
}

/** A time in a few words, like the bot writes it: `1d 12h`, `45m`; `none` for 0. */
export function spanText(secs: number): string {
  if (secs <= 0) return "none";
  const parts: string[] = [];
  let rest = secs;
  for (const [size, unit] of [
    [86_400, "d"],
    [3_600, "h"],
    [60, "m"],
    [1, "s"],
  ] as const) {
    const n = Math.floor(rest / size);
    rest -= n * size;
    if (n > 0) parts.push(`${n}${unit}`);
  }
  return parts.join(" ");
}

export const LIMITS = {
  delay: [30, 30 * 86_400],
  expiry: [60, 365 * 86_400],
  title: 80,
  description: 500,
  label: 80,
  line: 100,
  entries: 25,
} as const;

/** Why a delay is not allowed, or null; 0 means none. */
export function problemWithDelay(secs: number): string | null {
  if (secs === 0) return null;
  if (secs < LIMITS.delay[0] || secs > LIMITS.delay[1]) return "A delay is none, or 30 seconds to 30 days.";
  return null;
}

export function problemWithExpiry(secs: number): string | null {
  if (secs === 0) return null;
  if (secs < LIMITS.expiry[0] || secs > LIMITS.expiry[1])
    return "How long a role lasts is none, or a minute to a year.";
  return null;
}

/** The rules a panel asks, checked as `/roles panel` checks them. */
export function problemWithRules(exclusive: boolean, min: number, max: number): string | null {
  if (min > LIMITS.entries || max > LIMITS.entries || min < 0 || max < 0)
    return spaTranslate("A limit is 0 to {arg1}.", { arg1: LIMITS.entries });
  if (max > 0 && min > max) return "The fewest roles can't be more than the most.";
  if (exclusive && min > 1) return "One at a time can ask members to keep at most one role.";
  return null;
}

// /Panels

export const STYLES: { value: PanelStyle; title: string; detail: string }[] = [
  { value: "buttons", title: "Buttons", detail: "A button per role; click again to drop it." },
  { value: "select", title: "Menu", detail: "One dropdown; members pick what they want." },
  { value: "reactions", title: "Reactions", detail: "An emoji per role under the message." },
];

/** A panel being edited: the saved fields, and a key per role row so the list can be reordered. */
export interface PanelDraft {
  title: string;
  description: string;
  style: PanelStyle;
  exclusive: boolean;
  required_role: string | null;
  min_picks: number;
  max_picks: number;
  roles: (PanelRole & { key: string })[];
}

let rowKeys = 0;
export const newKey = () => `row-${++rowKeys}`;

export function emptyDraft(): PanelDraft {
  return {
    title: "",
    description: "",
    style: "buttons",
    exclusive: false,
    required_role: null,
    min_picks: 0,
    max_picks: 0,
    roles: [],
  };
}

export function draftOf(panel: Panel): PanelDraft {
  return {
    title: panel.title,
    description: panel.description ?? "",
    style: panel.style,
    exclusive: panel.exclusive,
    required_role: panel.required_role,
    min_picks: panel.min_picks,
    max_picks: panel.max_picks,
    roles: panel.roles.map((r) => ({ ...r, key: newKey() })),
  };
}

/** What `PUT /roles/panels/{id}` takes. */
export function bodyOf(draft: PanelDraft) {
  return {
    title: draft.title.trim(),
    description: draft.description.trim() || null,
    style: draft.style,
    exclusive: draft.exclusive,
    required_role: draft.required_role,
    min_picks: draft.min_picks,
    max_picks: draft.max_picks,
    roles: draft.roles.map((r) => ({
      role_id: r.role_id,
      label: r.label.trim(),
      emoji: r.emoji?.trim() || null,
      description: r.description?.trim() || null,
      temp_secs: r.temp_secs && r.temp_secs > 0 ? r.temp_secs : null,
    })),
  };
}

/** How many things differ between the saved panel and the editor, for the "unsaved" count. */
export function editsOf(saved: PanelDraft, now: PanelDraft): number {
  let n = 0;
  for (const key of [
    "title",
    "description",
    "style",
    "exclusive",
    "required_role",
    "min_picks",
    "max_picks",
  ] as const) {
    if (saved[key] !== now[key]) n++;
  }
  const strip = (d: PanelDraft) => JSON.stringify(bodyOf(d).roles);
  if (strip(saved) !== strip(now)) n++;
  return n;
}

/** The first thing wrong with a draft the page can see without asking the server, or null. */
export function problemWithDraft(draft: PanelDraft): { field: string; message: string } | null {
  const title = draft.title.trim();
  if (title.length === 0 || [...title].length > LIMITS.title)
    return {
      field: "title",
      message: spaTranslate("The title is 1 to {arg1} characters.", { arg1: LIMITS.title }),
    };
  if ([...draft.description].length > LIMITS.description)
    return {
      field: "description",
      message: spaTranslate("The text is at most {arg1} characters.", { arg1: LIMITS.description }),
    };
  const rules = problemWithRules(draft.exclusive, draft.min_picks, draft.max_picks);
  if (rules) return { field: "rules", message: rules };
  if (draft.roles.length > LIMITS.entries)
    return {
      field: "roles",
      message: spaTranslate("A panel holds at most {arg1} roles.", { arg1: LIMITS.entries }),
    };
  const seen = new Set<string>();
  for (const row of draft.roles) {
    if (!row.role_id) return { field: "roles", message: "Pick a role for every row." };
    if (seen.has(row.role_id)) return { field: "roles", message: "A role is on the panel twice." };
    seen.add(row.role_id);
    const label = row.label.trim();
    if (label.length === 0 || [...label].length > LIMITS.label)
      return {
        field: "roles",
        message: spaTranslate("Every label is 1 to {arg1} characters.", { arg1: LIMITS.label }),
      };
    if (row.temp_secs) {
      const why = problemWithExpiry(row.temp_secs);
      if (why) return { field: "roles", message: why };
    }
  }
  return null;
}

/** The footer a posted panel carries, as the bot words it. */
export function footerOf(draft: Pick<PanelDraft, "style" | "exclusive" | "max_picks">): string {
  const base = {
    buttons: "Click a button to get a role; click it again to drop it",
    select: "Pick your roles from the menu",
    reactions: "React with an emoji to get its role; remove the reaction to drop it",
  }[draft.style];
  if (draft.exclusive) return `${base} · one at a time`;
  if (draft.max_picks > 0) return `${base} · up to ${draft.max_picks} roles`;
  return base;
}

/** What the reactions panel needs that the draft lacks: the labels of roles with no emoji, and repeats. */
export function reactionProblems(draft: PanelDraft): string[] {
  if (draft.style !== "reactions") return [];
  const out: string[] = [];
  const seen = new Set<string>();
  for (const row of draft.roles) {
    const emoji = row.emoji?.trim().replaceAll("️", "");
    if (!emoji) out.push(`${row.label || "A role"} needs an emoji.`);
    else if (seen.has(emoji)) out.push(`${row.label || "A role"} shares its emoji.`);
    else seen.add(emoji);
  }
  return out;
}

/** A custom emoji as Discord writes it, `<:name:id>` or `<a:name:id>`; its CDN picture's URL, else null. */
export function customEmojiUrl(text: string | null): string | null {
  const match = /^<(a?):[A-Za-z0-9_]{2,32}:(\d{1,20})>$/.exec((text ?? "").trim());
  return match ? `https://cdn.discordapp.com/emojis/${match[2]}.${match[1] ? "gif" : "png"}?size=48` : null;
}

// /Colours

export const COLOR_MODES: { value: ColorMode; title: string; detail: string }[] = [
  { value: "palette", title: "Palette", detail: "Members pick from the colours below (and your own)." },
  { value: "hex", title: "Any colour", detail: "Members can also type a hex code with /color set." },
];

/** `#rrggbb` text colour that reads on the dark preview, for names in the strip. */
export function isHex(text: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(text);
}
