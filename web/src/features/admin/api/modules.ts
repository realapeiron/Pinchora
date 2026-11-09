import { number, translate as spaTranslate } from "../../../i18n";
// Module edits: what the page lets you change, and the request that saves it.

import type { ChannelMode, ModuleEntry, ModulePatch } from "../../../lib/api";

/** Unsaved edits to one module. A field left out is as saved. */
export interface ModuleDraft {
  enabled?: boolean;
  channel_mode?: ChannelMode;
  channels?: string[];
}

/** What a module would be if the draft were saved. */
export function effective(saved: ModuleEntry, draft: ModuleDraft | undefined) {
  const mode = draft?.channel_mode ?? saved.channel_mode;
  return {
    enabled: draft?.enabled ?? saved.enabled,
    mode,
    // "All channels" names none, whatever the list held.
    channels: mode === "all" ? [] : (draft?.channels ?? saved.channels),
  };
}

function sameChannels(a: string[], b: string[]): boolean {
  return a.length === b.length && [...a].sort().join() === [...b].sort().join();
}

/** The request that saves the draft, or `null` when it changes nothing. */
export function patchFor(saved: ModuleEntry, draft: ModuleDraft | undefined): ModulePatch | null {
  const now = effective(saved, draft);
  const patch: ModulePatch = {};
  if (now.enabled !== saved.enabled) patch.enabled = now.enabled;
  if (
    !saved.server_only &&
    (now.mode !== saved.channel_mode || !sameChannels(now.channels, saved.channels))
  ) {
    patch.channel_mode = now.mode;
    patch.channels = now.channels;
  }
  return Object.keys(patch).length > 0 ? patch : null;
}

/** How many edits the page has: a switch and a channel rule each count once. */
export function changeCount(modules: ModuleEntry[], drafts: Record<string, ModuleDraft>): number {
  return modules.reduce((count, saved) => {
    const patch = patchFor(saved, drafts[saved.key]);
    return count + (patch ? Number("enabled" in patch) + Number("channel_mode" in patch) : 0);
  }, 0);
}

/** Why the draft cannot be saved yet, in the words of the form. */
export function problem(saved: ModuleEntry, draft: ModuleDraft | undefined, max: number): string | null {
  const now = effective(saved, draft);
  if (!saved.server_only && now.mode !== "all" && now.channels.length === 0) {
    return "Pick at least one channel, or choose all channels.";
  }
  if (now.channels.length > max)
    return spaTranslate("A module can name at most {arg1} channels.", { arg1: max });
  return null;
}

/** A short word and a tone for the state of a module, for badges. */
export function badge(
  enabled: boolean,
  mode: ChannelMode,
  channels: number,
): { text: string; tone: "mint" | "gold" | "dim" } {
  if (!enabled) return { text: spaTranslate("OFF"), tone: "dim" };
  if (mode === "all") return { text: spaTranslate("ON"), tone: "mint" };
  const n = `${number(channels)} ${spaTranslate(channels === 1 ? "CHANNEL" : "CHANNELS")}`;
  return mode === "only"
    ? { text: spaTranslate("ONLY IN {arg1}", { arg1: n }), tone: "gold" }
    : { text: spaTranslate("OFF IN {arg1}", { arg1: n }), tone: "gold" };
}

/** Picks a mode, keeping the channels already chosen so switching between the two lists loses nothing. */
export function withMode(saved: ModuleEntry, draft: ModuleDraft | undefined, mode: ChannelMode): ModuleDraft {
  return { ...draft, channel_mode: mode, channels: draft?.channels ?? saved.channels };
}

export function withChannels(draft: ModuleDraft | undefined, channels: string[]): ModuleDraft {
  return { ...draft, channels };
}
