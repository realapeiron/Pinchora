import { createContext, useContext } from "react";
import { number, serverText } from "../../../i18n";

export const InviteWordsContext = createContext({ language: "en", strings: {} as Record<string, string> });
export function useInviteWords() {
  const { language, strings } = useContext(InviteWordsContext);
  void language;
  const n = number;
  const t = (key: string, values: Record<string, string | number> = {}) => serverText(key, strings, values);
  return { t, n };
}
