import { useSyncExternalStore } from "react";

import { en } from "./en";
import { tr } from "./tr";

export type Locale = "en" | "tr";
export type MessageKey = keyof typeof en;
type Values = Record<string, string | number>;

let selected: Locale | null = stored(null);
let account: string | null = null;
let guild: Locale | null = null;
const listeners = new Set<() => void>();

function stored(id: string | null): Locale | null {
  try {
    const value = localStorage.getItem(`pinchora.language.${id ?? "visitor"}`);
    return value === "en" || value === "tr" ? value : null;
  } catch {
    return null;
  }
}

function announce() {
  if (typeof document !== "undefined") document.documentElement.lang = locale();
  for (const listener of listeners) listener();
}

/** A user's explicit choice wins; otherwise use the managed server, then the browser. */
export function locale(): Locale {
  if (selected) return selected;
  return (
    guild ??
    (typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("tr") ? "tr" : "en")
  );
}

export function useLocale(): Locale {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    locale,
    () => "en",
  );
}

/** Called after /api/me loads; the bot's /language selection is authoritative. */
export function configureUser(id: string, choice: Locale | null) {
  if (account === id && selected === (choice ?? stored(id))) return;
  account = id;
  selected = choice ?? stored(id);
  announce();
}

export function configureGuild(choice: Locale | null) {
  if (guild === choice) return;
  guild = choice;
  announce();
}

export function clearUser() {
  account = null;
  selected = stored(null);
  guild = null;
  announce();
}

/** Switch immediately and save locally. Authenticated users also save to /language. */
export async function setLocale(choice: Locale, save?: (choice: Locale) => Promise<unknown>) {
  selected = choice;
  try {
    localStorage.setItem(`pinchora.language.${account ?? "visitor"}`, choice);
  } catch {
    // Storage can be disabled by the browser; the current page still switches.
  }
  announce();
  if (save) await save(choice);
}

export function number(value: number, choice: Locale = locale()): string {
  return new Intl.NumberFormat(choice === "tr" ? "tr-TR" : "en-US").format(value);
}

export function date(value: Date | string | number, choice: Locale = locale()): string {
  return new Intl.DateTimeFormat(choice === "tr" ? "tr-TR" : "en-US", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(value));
}

export function translate(key: string, values: Values = {}, choice: Locale = locale()): string {
  const catalog: Record<string, string> = choice === "tr" ? tr : en;
  const template = catalog[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = values[name];
    return typeof value === "number" ? number(value, choice) : (value ?? match);
  });
}

export function t(key: MessageKey, values?: Values): string {
  return translate(key, values);
}

export function plural(
  count: number,
  forms: { one: MessageKey; other: MessageKey },
  values: Values = {},
): string {
  const choice = locale();
  const category = new Intl.PluralRules(choice === "tr" ? "tr-TR" : "en-US").select(count);
  return translate(category === "one" ? forms.one : forms.other, { count, ...values }, choice);
}

export function useI18n() {
  const choice = useLocale();
  return {
    locale: choice,
    t: (key: MessageKey, values?: Values) => translate(key, values, choice),
    number: (value: number) => number(value, choice),
    date: (value: Date | string | number) => date(value, choice),
    plural: (count: number, forms: { one: MessageKey; other: MessageKey }, values: Values = {}) => {
      const category = new Intl.PluralRules(choice === "tr" ? "tr-TR" : "en-US").select(count);
      return translate(category === "one" ? forms.one : forms.other, { count, ...values }, choice);
    },
  };
}
