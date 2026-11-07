import { locale, number, translate, translate as spaTranslate } from "../i18n";
// The settings rules, for instant feedback while typing. The server checks again on save with the
// same rules (pinchora_core::settings); rules.json is written from them, and rules.test.ts checks
// these functions give the server's exact answers for the samples in it.

import rules from "./rules.json";

const validationNumber = (value: number) => (locale() === "tr" ? number(value) : String(value));

export const LIMITS = rules.limits;
export const THEMES = rules.themes;
export const ACCENTS = rules.accents;
export const LEVELUP_MODES = rules.levelup_modes;
export const DEFAULTS = rules.defaults;

export type MessageKind = "welcome" | "goodbye" | "levelup";

export const MESSAGES: Record<MessageKind, { field: string; placeholders: string[]; default: string }> =
  rules.messages;

/** A check's answer: the cleaned value, or the field at fault and why. */
export type Check<T> = { ok: true; value: T } | { ok: false; field: string; message: string };

function fail<T>(field: string, message: string): Check<T> {
  return { ok: false, field, message: translate(message) };
}

function ok<T>(value: T): Check<T> {
  return { ok: true, value };
}

const characters = (s: string) => [...s].length;

/** `#ff6b4a` (the `#` optional) as a number. */
export function parseHexColor(s: string): number | null {
  const trimmed = s.trim();
  const hex = trimmed.startsWith("#") ? trimmed.slice(1) : trimmed;
  return /^[0-9a-fA-F]{6}$/.test(hex) ? parseInt(hex, 16) : null;
}

export function checkAccent(s: string): Check<number> {
  const rgb = parseHexColor(s);
  return rgb === null ? fail("welcome_accent", "Colours look like `#ff6b4a`.") : ok(rgb);
}

export function checkMessage(kind: MessageKind, text: string): Check<true> {
  const field = MESSAGES[kind].field;
  if (text.trim() === "") return fail(field, "Write a message, or reset it to use the default.");
  const length = characters(text);
  if (length > LIMITS.message_max) {
    return fail(
      field,
      spaTranslate("A message is at most {arg1} characters; this one has {arg2}.", {
        arg1: LIMITS.message_max,
        arg2: length,
      }),
    );
  }
  return ok(true);
}

export function checkTitle(s: string): Check<string> {
  const title = s.trim();
  if (title === "") return fail("welcome_title", "Write a headline, e.g. WELCOME.");
  if (characters(title) > LIMITS.welcome_title_max) {
    return fail(
      "welcome_title",
      spaTranslate("A headline is at most {arg1} characters.", { arg1: LIMITS.welcome_title_max }),
    );
  }
  return ok(title);
}

export function checkPointsName(s: string): Check<string> {
  const name = s.trim();
  if (name === "" || characters(name) > LIMITS.points_name_max) {
    return fail(
      "points_name",
      spaTranslate("Points need a name of 1 to {arg1} characters, e.g. coins.", {
        arg1: LIMITS.points_name_max,
      }),
    );
  }
  return ok(name);
}

export function checkPrefix(s: string): Check<string> {
  const prefix = s.trim();
  const length = characters(prefix);
  if (length === 0 || length > LIMITS.prefix_max) {
    return fail(
      "prefix",
      spaTranslate("A prefix is 1 to {arg1} characters, e.g. p! or ?.", { arg1: LIMITS.prefix_max }),
    );
  }
  if (/[\s@#`]/u.test(prefix)) return fail("prefix", "A prefix can't hold spaces, @, # or backticks.");
  return ok(prefix);
}

function isCustomEmoji(s: string): boolean {
  return /^<a?:[A-Za-z0-9_]{2,32}:[0-9]{17,20}>$/.test(s);
}

function graphemes(s: string): number {
  return [...new Intl.Segmenter("en", { granularity: "grapheme" }).segment(s)].length;
}

export function checkPointsEmoji(s: string): Check<string> {
  const emoji = s.trim();
  const one = graphemes(emoji) === 1 || isCustomEmoji(emoji);
  if (emoji === "" || characters(emoji) > LIMITS.points_emoji_max || !one) {
    return fail(
      "points_emoji",
      "Use one emoji: a Unicode one like 🪙, or a custom one like <:coin:123456789012345678>.",
    );
  }
  return ok(emoji);
}

export function checkPoints(min: number, max: number): Check<true> {
  const [low, high] = LIMITS.points as [number, number];
  for (const [field, value] of [
    ["points_min", min],
    ["points_max", max],
  ] as const) {
    if (!Number.isInteger(value) || value < low || value > high) {
      return fail(
        field,
        spaTranslate("Points per message go from {arg1} to {arg2}.", {
          arg1: validationNumber(low),
          arg2: validationNumber(high),
        }),
      );
    }
  }
  if (min > max) return fail("points_min", "The least points per message can't be above the most.");
  return ok(true);
}

/** Rust's `f64::round`: halves go away from zero. */
function round(x: number): number {
  return Math.sign(x) * Math.round(Math.abs(x));
}

export function checkMultiplier(m: number): Check<number> {
  const value = round(m * 100) / 100;
  const [, max] = LIMITS.multiplier as [number, number];
  if (!Number.isFinite(value) || value < 0 || value > max) {
    return fail("multiplier", spaTranslate("A multiplier goes from 0 to {arg1}.", { arg1: max }));
  }
  return ok(value);
}

function inRange(field: string, n: number, range: number[], what: string): Check<number> {
  const [low, high] = range as [number, number];
  return Number.isInteger(n) && n >= low && n <= high
    ? ok(n)
    : fail(field, `${what} goes from ${low} to ${high}.`);
}

export function checkVoiceAmount(n: number): Check<number> {
  const [low, high] = LIMITS.voice_xp as [number, number];
  return Number.isInteger(n) && n >= low && n <= high
    ? ok(n)
    : fail(
        "voice_xp_amount",
        spaTranslate("Voice XP is {arg1} to {arg2} a minute.", { arg1: low, arg2: high }),
      );
}

export const checkLevel = (n: number) => inRange("level", n, LIMITS.level, "A level role's level");
export const checkReputation = (n: number) =>
  inRange("reputation", n, LIMITS.reputation, "A reputation role's count");
export const checkPrice = (n: number) => inRange("price", n, LIMITS.price, "A price");
export const checkStock = (n: number) => inRange("stock", n, LIMITS.stock, "Stock");

/** The message of a failed check, or null. */
export function problem<T>(check: Check<T>): string | null {
  return check.ok ? null : check.message;
}
