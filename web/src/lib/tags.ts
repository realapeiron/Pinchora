import { locale, number, translate as spaTranslate } from "../i18n";
// Custom commands and auto-responders in the browser: the limits and placeholders (written from
// pinchora_core::community::tags into rules.json), blank drafts, and the instant checks. The server checks again
// on save with the same rules; rules.test.ts keeps `checkCommandName` in step with it.

import type {
  CommandDef,
  CustomCommand,
  Destination,
  EmbedSpec,
  MatchKind,
  ResponderDef,
  TagAction,
  TriggerKind,
} from "./api";
import rules from "./rules.json";
import { type Check } from "./rules";

export const TAGS = rules.tags;
export const LIMITS = TAGS.limits;

export interface Trigger {
  key: TriggerKind;
  label: string;
  needs_message_content: boolean;
}
export const TRIGGERS = TAGS.triggers as Trigger[];

export interface Placeholder {
  token: string;
  hint: string;
}

/** The placeholder chips of a command, or of a responder of `kind`. */
export function placeholdersFor(kind: "command" | TriggerKind): Placeholder[] {
  if (kind === "command") return TAGS.command_placeholders;
  return (TAGS.responder_placeholders as Record<string, Placeholder[]>)[kind] ?? [];
}

export const MATCH_KINDS: { value: MatchKind; title: string; detail: string }[] = [
  { value: "contains", title: "Contains", detail: "The word or phrase is anywhere in the message." },
  { value: "exact", title: "Exactly", detail: "The whole message is the word or phrase." },
  { value: "starts_with", title: "Starts with", detail: "The message begins with it." },
  { value: "ends_with", title: "Ends with", detail: "The message ends with it." },
];

export const DESTINATIONS: { value: Destination; title: string; detail: string }[] = [
  { value: "current", title: "Where it happened", detail: "The channel of the message or the level-up." },
  { value: "channel", title: "One channel", detail: "Always the same channel." },
  { value: "dm", title: "A direct message", detail: "To the member. Closed DMs are skipped quietly." },
];

/** Where a trigger can answer: a member who left has no DM, and only chat has a "here". */
export function destinationsFor(kind: TriggerKind) {
  return DESTINATIONS.filter((d) => {
    if (d.value === "current") return kind === "keyword" || kind === "level_up";
    if (d.value === "dm") return kind !== "member_leave";
    return true;
  });
}

export function blankEmbed(): EmbedSpec {
  return {
    title: null,
    description: null,
    url: null,
    color: null,
    author: null,
    footer: null,
    image: null,
    thumbnail: null,
    fields: [],
  };
}

export function blankCommand(): CommandDef {
  return {
    name: "",
    description: "",
    responses: [""],
    embed: null,
    reply_mode: "public",
    actions: [],
    allow_roles: [],
    deny_roles: [],
    allow_channels: [],
    deny_channels: [],
    cooldown_secs: 0,
    enabled: true,
    allow_mentions: false,
  };
}

export function blankResponder(kind: TriggerKind = "member_join"): ResponderDef {
  return {
    name: "",
    trigger_kind: kind,
    trigger_value: null,
    match_kind: kind === "keyword" ? "contains" : null,
    responses: [""],
    embed: null,
    destination: kind === "keyword" || kind === "level_up" ? "current" : "channel",
    channel_id: null,
    actions: [],
    chance: 100,
    cooldown_secs: 0,
    allow_channels: [],
    deny_channels: [],
    enabled: true,
    allow_mentions: false,
  };
}

/** What a saved command holds of what the server wrote: the ids and counts the database adds are dropped. */
export function defOf(item: CustomCommand): CommandDef {
  return {
    name: item.name,
    description: item.description,
    responses: item.responses,
    embed: item.embed,
    reply_mode: item.reply_mode,
    actions: item.actions,
    allow_roles: item.allow_roles,
    deny_roles: item.deny_roles,
    allow_channels: item.allow_channels,
    deny_channels: item.deny_channels,
    cooldown_secs: item.cooldown_secs,
    enabled: item.enabled,
    allow_mentions: item.allow_mentions,
  };
}

/** The same, for a responder (or any def: unknown keys such as the id are dropped by key list). */
export function responderDefOf<T extends ResponderDef>(item: T): ResponderDef {
  return {
    name: item.name,
    trigger_kind: item.trigger_kind,
    trigger_value: item.trigger_value,
    match_kind: item.match_kind,
    responses: item.responses,
    embed: item.embed,
    destination: item.destination,
    channel_id: item.channel_id,
    actions: item.actions,
    chance: item.chance,
    cooldown_secs: item.cooldown_secs,
    allow_channels: item.allow_channels,
    deny_channels: item.deny_channels,
    enabled: item.enabled,
    allow_mentions: item.allow_mentions,
  };
}

/** Whether two drafts say the same thing. */
export function sameDef(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Which top-level fields of a draft differ from what was saved, for the unsaved-changes count. */
export function changedKeys<T extends object>(saved: T, draft: T): (keyof T)[] {
  return (Object.keys(draft) as (keyof T)[]).filter((key) => !sameDef(saved[key], draft[key]));
}

/** A command name made tidy: trimmed, lowercase, a leading `/` dropped. */
export function tidyName(name: string): string {
  return name.trim().replace(/^\/+/, "").trim().toLowerCase();
}

const characters = (s: string) => [...s].length;

/** The instant check of a command's name; the sentences are the server's. */
export function checkCommandName(raw: string): Check<string> {
  const name = tidyName(raw);
  if (!new RegExp(`^[a-z0-9][a-z0-9_-]{0,${LIMITS.name_max - 1}}$`).test(name)) {
    return {
      ok: false,
      field: "name",
      message: spaTranslate(
        "A name is 1 to {arg1} characters: lowercase letters, numbers, `-` and `_`, starting with a letter or number.",
        { arg1: LIMITS.name_max },
      ),
    };
  }
  if (TAGS.reserved.includes(name)) {
    return { ok: false, field: "name", message: "That name belongs to one of Pinchora's own commands." };
  }
  return { ok: true, value: name };
}

/** How many characters an embed shows in all, as Discord counts its 6,000. */
export function embedChars(embed: EmbedSpec | null): number {
  if (!embed) return 0;
  const n = (t: string | null) => (t ? characters(t) : 0);
  return (
    n(embed.title) +
    n(embed.description) +
    n(embed.author) +
    n(embed.footer) +
    embed.fields.reduce((sum, f) => sum + characters(f.name) + characters(f.value), 0)
  );
}

/** An embed with nothing in it is no embed. */
export function embedIsBlank(embed: EmbedSpec | null): boolean {
  if (!embed) return true;
  const blank = (t: string | null) => !t || t.trim() === "";
  return (
    blank(embed.title) &&
    blank(embed.description) &&
    blank(embed.footer) &&
    blank(embed.author) &&
    blank(embed.image) &&
    blank(embed.thumbnail) &&
    embed.fields.length === 0
  );
}

/** What the server is sent: texts empty become unset, an embed with nothing is left out. */
export function tidyEmbed(embed: EmbedSpec | null): EmbedSpec | null {
  if (!embed || embedIsBlank(embed)) return null;
  const text = (t: string | null) => (t && t.trim() !== "" ? t : null);
  return {
    ...embed,
    title: text(embed.title),
    description: text(embed.description),
    url: text(embed.url),
    color: text(embed.color),
    author: text(embed.author),
    footer: text(embed.footer),
    image: text(embed.image),
    thumbnail: text(embed.thumbnail),
  };
}

/** Whether an action hands out or takes a role (and so needs one picked). */
export function actionNeedsRole(action: TagAction): action is Extract<TagAction, { role_id: string }> {
  return action.kind === "add_role" || action.kind === "remove_role" || action.kind === "toggle_role";
}

/** What an unfinished action lacks, in the server's words, or `null`: saving waits until it is none. */
export function actionsProblem(actions: TagAction[]): string | null {
  for (const [index, action] of actions.entries()) {
    const n = index + 1;
    if (actionNeedsRole(action) && action.role_id === "")
      return spaTranslate("Action {arg1} needs a role.", { arg1: n });
    if (
      action.kind === "dm" &&
      (action.text.trim() === "" || [...action.text].length > LIMITS.response_max)
    ) {
      return spaTranslate("Action {arg1}: a direct message is 1 to {arg2} characters.", {
        arg1: n,
        arg2: LIMITS.response_max,
      });
    }
    if (action.kind === "give_points") {
      const [low, high] = LIMITS.points_action as [number, number];
      if (
        !Number.isInteger(action.amount) ||
        action.amount === 0 ||
        action.amount < low ||
        action.amount > high
      ) {
        return spaTranslate("Action {arg1}: points go from {arg2} to {arg3}, and not 0.", {
          arg1: n,
          arg2: locale() === "tr" ? number(low) : String(low),
          arg3: locale() === "tr" ? number(high) : String(high),
        });
      }
    }
    if (action.kind === "react" && action.emoji.trim() === "")
      return spaTranslate("Action {arg1} needs an emoji.", { arg1: n });
  }
  return null;
}

/** The actions the preview can use: the unfinished ones are left out until they are. */
export function finishedActions(actions: TagAction[]): TagAction[] {
  return actions.filter((action) => actionsProblem([action]) === null);
}

/** A short sentence for an action, for lists and the audit log. */
export function describeAction(action: TagAction): string {
  switch (action.kind) {
    case "add_role":
      return spaTranslate("gives a role");
    case "remove_role":
      return spaTranslate("takes a role");
    case "toggle_role":
      return spaTranslate("toggles a role");
    case "give_points":
      return action.amount >= 0
        ? spaTranslate("gives {arg1} points", { arg1: action.amount })
        : spaTranslate("takes {arg1} points", { arg1: -action.amount });
    case "dm":
      return spaTranslate("sends a direct message");
    case "react":
      return spaTranslate("reacts with {arg1}", { arg1: action.emoji });
  }
}

/** The cooldown a draft keeps when its actions change: raised to the floor once it does something. */
export function cooldownFor(actions: TagAction[], seconds: number): number {
  return actions.length > 0 ? Math.max(seconds, LIMITS.action_cooldown_min) : seconds;
}

/** A cooldown in the biggest unit that reads well: "30 s", "5 min", "2 h". */
export function cooldownLabel(seconds: number): string {
  if (seconds <= 0) return spaTranslate("none");
  if (seconds < 90) return `${seconds} s`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min`;
  return `${Math.round(seconds / 3600)} h`;
}

/** The file name an export is downloaded as. */
export function exportFileName(kind: "commands" | "responders", guild: string): string {
  return `pinchora-${kind}-${guild}.json`;
}
