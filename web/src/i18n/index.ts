import { useSyncExternalStore } from "react";

import type { en as EnglishCatalog } from "./en";

type Catalog = Record<string, string>;
let en: Catalog = {};
let tr: Catalog = {};
let englishForTurkish = new Map<string, string>();
const loaded = new Map<Locale, Promise<void>>();

export function loadLocale(choice: Locale): Promise<void> {
  const existing = loaded.get(choice);
  if (existing) return existing;
  const pending =
    choice === "en"
      ? import("./en").then((module) => {
          en = module.en;
        })
      : import("./tr").then((module) => {
          tr = module.tr;
          englishForTurkish = new Map(Object.entries(tr).map(([key, value]) => [value, key]));
        });
  const retryable = pending.catch((error: unknown) => {
    loaded.delete(choice);
    throw error;
  });
  loaded.set(choice, retryable);
  return retryable;
}

export async function ready(): Promise<void> {
  await Promise.all([loadLocale(locale()), loadLocale(locale() === "en" ? "tr" : "en")]);
}

export function hasMessage(key: string): boolean {
  return key in en;
}

export type Locale = "en" | "tr";
export type MessageKey = keyof typeof EnglishCatalog;
type Values = Record<string, string | number>;

let selected: Locale | null = stored(null);
let account: string | null = null;
let guild: Locale | null = null;
const listeners = new Set<() => void>();
let selection = 0;
let saving: Promise<unknown> = Promise.resolve();

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
  selection++;
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
  selection++;
  account = null;
  selected = stored(null);
  guild = null;
  announce();
}

/** Publish only the latest loaded choice. Authenticated users also save to /language. */
export async function setLocale(choice: Locale, save?: (choice: Locale) => Promise<unknown>) {
  const request = ++selection;
  const owner = account;
  const previous = selected;
  try {
    await ready();
  } catch (error) {
    if (request !== selection) return;
    throw error;
  }
  if (request !== selection) return;
  selected = choice;
  try {
    localStorage.setItem(`pinchora.language.${account ?? "visitor"}`, choice);
  } catch {
    // Storage can be disabled by the browser; the current page still switches.
  }
  announce();
  try {
    if (save) {
      const pending = saving.then(() => (request === selection ? save(choice) : undefined));
      saving = pending.catch(() => undefined);
      await pending;
    }
  } catch (error) {
    if (request !== selection || owner !== account) return;
    selected = previous;
    try {
      const key = `pinchora.language.${owner ?? "visitor"}`;
      if (previous) localStorage.setItem(key, previous);
      else localStorage.removeItem(key);
    } catch {
      /* Storage can be disabled. */
    }
    announce();
    throw error;
  }
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

export function dateTime(value: Date | string | number, choice: Locale = locale()): string {
  return new Intl.DateTimeFormat(choice === "tr" ? "tr-TR" : "en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

export function translate(key: string, values: Values = {}, choice: Locale = locale()): string {
  const catalog: Record<string, string> = choice === "tr" ? tr : en;
  const template = catalog[key] ?? en[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = values[name];
    return typeof value === "number" ? number(value, choice) : (value ?? match);
  });
}

/** Prefer the active dashboard language over the guild language used by API UI catalogs. */
export function serverText(
  key: string,
  strings: Record<string, string> = {},
  values: Values = {},
  fallbackLanguage?: string,
): string {
  const supplied = strings[key];
  const catalogKey = supplied
    ? supplied in en
      ? supplied
      : englishForTurkish.get(supplied)
    : key in en
      ? key
      : undefined;
  if (catalogKey) return translate(catalogKey, values);
  return (supplied ?? key).replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = values[name];
    return typeof value === "number"
      ? number(value, fallbackLanguage === "tr" ? "tr" : fallbackLanguage === "en" ? "en" : locale())
      : (value ?? match);
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
