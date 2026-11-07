import { createContext, useContext } from "react";
import { serverText } from "../i18n";
export type FeedWords = (key: string, values?: Record<string, string | number>) => string;
export const FeedWordsContext = createContext<Record<string, string>>({});
export function feedWords(strings: Record<string, string>): FeedWords {
  return (key, values = {}) => serverText(key, strings, values);
}
export function useFeedWords(): FeedWords {
  return feedWords(useContext(FeedWordsContext));
}
