import { translate } from "../i18n";

export function t(english: string): string {
  const leading = english.startsWith(" ") ? " " : "";
  const trailing = english.endsWith(" ") ? " " : "";
  return `${leading}${translate(english.trim()).trim().replaceAll("&rsquo;", "’")}${trailing}`;
}

export function tf(english: string, values: Record<string, string | number>): string {
  const leading = english.startsWith(" ") ? " " : "";
  const trailing = english.endsWith(" ") ? " " : "";
  return `${leading}${translate(english.trim(), values).trim().replaceAll("&rsquo;", "’")}${trailing}`;
}
