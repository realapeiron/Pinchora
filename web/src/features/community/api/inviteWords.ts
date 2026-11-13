import { createContext, useContext } from "react";
import { number } from "../../../i18n";
import { dashboardServerText } from "../../../i18n/dashboard";

export const InviteWordsContext = createContext({ language: "en", strings: {} as Record<string, string> });
export function useInviteWords() {
  const { language, strings } = useContext(InviteWordsContext);
  void language;
  const n = number;
  const t = (key: string, values: Record<string, string | number> = {}) =>
    dashboardServerText(key, strings, values);
  return { t, n };
}
