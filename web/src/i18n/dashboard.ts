import { locale, number, serverText, translate } from ".";
import { dashboardEn as baseEn } from "./dashboard-en";
import { dashboardTr as baseTr } from "./dashboard-tr";
import { adminEn } from "./dashboard-admin-en";
import { adminTr } from "./dashboard-admin-tr";
import { communityEn } from "./dashboard-community-en";
import { communityTr } from "./dashboard-community-tr";
import { settingsEn } from "./dashboard-settings-en";
import { settingsTr } from "./dashboard-settings-tr";

export const dashboardEn = { ...baseEn, ...adminEn, ...communityEn, ...settingsEn };
export const dashboardTr = { ...baseTr, ...adminTr, ...communityTr, ...settingsTr };
const englishCatalog: Record<string, string> = dashboardEn;
const turkishCatalog: Record<string, string> = dashboardTr;
const englishForTurkish = new Map(Object.entries(dashboardTr).map(([key, value]) => [value, key]));

export function dashboardServerText(
  key: string,
  strings: Record<string, string> = {},
  values: Record<string, string | number> = {},
  fallbackLanguage?: string,
): string {
  const supplied = strings[key];
  const catalogKey =
    key in englishCatalog &&
    (!supplied || supplied === englishCatalog[key] || supplied === turkishCatalog[key])
      ? key
      : supplied
        ? supplied in englishCatalog
          ? supplied
          : englishForTurkish.get(supplied)
        : undefined;
  return catalogKey ? t(catalogKey, values) : serverText(key, strings, values, fallbackLanguage);
}

/** Loaded with guild dashboard routes, outside the playable room cold path. */
export function t(key: string, values: Record<string, string | number> = {}, legacyKey = key) {
  const choice = locale();
  const catalog: Record<string, string> = choice === "tr" ? dashboardTr : dashboardEn;
  const template = catalog[key];
  if (template === undefined) return translate(legacyKey, values, choice);
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = values[name];
    return typeof value === "number" ? number(value, choice) : (value ?? match);
  });
}
