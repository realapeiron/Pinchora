import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../lib/api";
import { serverText } from "../../../i18n";

export interface BirthdaySettings {
  enabled: boolean;
  channel_id: string | null;
  role_id: string | null;
  message: string;
  announce_hour: number;
  server_tz: string;
  shells_gift: number;
  allow_age: boolean;
  badge: boolean;
}
export interface UpcomingBirthday {
  user_id: string;
  name: string;
  date: string;
  month: number;
  day: number;
}
export interface BirthdaysData {
  language: string;
  strings: Record<string, string>;
  settings: BirthdaySettings;
  upcoming: UpcomingBirthday[];
  limit: number;
  today: string;
}
export const birthdaysKey = (guild: string) => [`/api/guilds/${guild}/birthdays`];
export function useBirthdays(guild: string) {
  return useQuery({
    queryKey: birthdaysKey(guild),
    queryFn: () => api<BirthdaysData>(`/api/guilds/${guild}/birthdays`),
  });
}
export function useSaveBirthdays(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (settings: BirthdaySettings) =>
      api<BirthdaysData>(`/api/guilds/${guild}/birthdays`, "PUT", settings),
    onSuccess: async (data) => {
      client.setQueryData(birthdaysKey(guild), data);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}

export function birthdayWords(strings: Record<string, string> = {}, language = "en") {
  return (key: string, args: Record<string, string | number> = {}) =>
    serverText(key, strings, args, language);
}
