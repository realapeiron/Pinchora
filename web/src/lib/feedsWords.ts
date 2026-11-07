import { createContext, useContext } from "react";
export type FeedWords = (key: string, values?: Record<string, string | number>) => string;
export const FeedWordsContext = createContext<Record<string, string>>({});
export function feedWords(strings: Record<string, string>): FeedWords {
  return (key, values = {}) =>
    (strings[key] ?? key).replace(/\{(\w+)\}/g, (match, name: string) => String(values[name] ?? match));
}
export function useFeedWords(): FeedWords {
  return feedWords(useContext(FeedWordsContext));
}
