import { serverText } from "../i18n";

/** UI catalogs come from the server's t! catalog in the guild language. */
export function catalogWords(strings: Record<string, string> = {}, language = "en") {
  return (key: string, args: Record<string, string | number> = {}) =>
    serverText(key, strings, args, language);
}
