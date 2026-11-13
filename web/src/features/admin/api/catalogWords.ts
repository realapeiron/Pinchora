import { dashboardServerText } from "../../../i18n/dashboard";

/** UI catalogs come from the server's t! catalog in the guild language. */
export function catalogWords(strings: Record<string, string> = {}, language = "en") {
  return (key: string, args: Record<string, string | number> = {}) =>
    dashboardServerText(key, strings, args, language);
}
