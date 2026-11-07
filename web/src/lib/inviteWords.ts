import { createContext, useContext } from "react";

export const InviteWordsContext = createContext({ language: "en", strings: {} as Record<string, string> });
export function useInviteWords() {
  const { language, strings } = useContext(InviteWordsContext);
  const n = (value: number) => value.toLocaleString(language === "tr" ? "tr-TR" : "en-US");
  const t = (key: string, values: Record<string, string | number> = {}) =>
    (strings[key] ?? key).replace(/\{(\w+)\}/g, (match, name: string) => {
      const value = values[name];
      return typeof value === "number" ? n(value) : (value ?? match);
    });
  return { t, n };
}
