import { translate } from "../i18n";

export function t(english: string): string {
  return translate(english);
}

export function tf(english: string, values: Record<string, string | number>): string {
  return translate(english, values);
}
