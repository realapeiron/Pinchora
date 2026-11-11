import { locale, number, translate } from ".";
import { dashboardEn } from "./dashboard-en";
import { dashboardTr } from "./dashboard-tr";

export { dashboardEn, dashboardTr };

/** Loaded with guild dashboard routes, outside the playable room cold path. */
export function t(key: string, values: Record<string, string | number> = {}) {
  const choice = locale();
  const catalog: Record<string, string> = choice === "tr" ? dashboardTr : dashboardEn;
  const template = catalog[key];
  if (template === undefined) return translate(key, values, choice);
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = values[name];
    return typeof value === "number" ? number(value, choice) : (value ?? match);
  });
}
