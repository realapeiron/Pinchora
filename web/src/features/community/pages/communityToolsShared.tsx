import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useOutletContext } from "react-router";
import { translate as t } from "../../../i18n";
import { Loading, Problem } from "../../../components/States";
import { type Overview, api } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";

export interface Giveaway {
  id: string;
  prize: string;
  status: string;
  ends_at: string;
  starts_at: string | null;
  winners: number;
  entries: number;
}
export interface Poll {
  id: string;
  question: string;
  status: string;
  options: string[];
  multiple: boolean;
  anonymous: boolean;
  ends_at: string;
}
export interface Suggestion {
  id: string;
  number: number;
  body: string;
  status: string;
  staff_note: string | null;
  thread_id: string | null;
  up: number;
  down: number;
}
export interface AutoChannel {
  channel_id: string;
  thread: boolean;
  crosspost: boolean;
}
export interface Confession {
  id: string;
  body: string;
  status: string;
  created_at: string;
  reports: number;
}
export interface Data {
  giveaways: Giveaway[];
  polls: Poll[];
  suggestions: Suggestion[];
  autopost: AutoChannel[];
  anniversary: { channel_id: string | null; enabled: boolean };
  confession_settings: {
    review_channel_id: string | null;
    publish_channel_id: string | null;
    enabled: boolean;
  };
  confessions: Confession[];
  message_content: boolean;
}
export interface Personal {
  reminders: { id: string; body: string; interval_minutes: number; next_at: string }[];
  checklist: { id: string; body: string; done: boolean }[];
  highlights: string[];
}
export function useTools() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useQuery({
    queryKey: ["community-v2", guild],
    queryFn: () => api<Data>(`/api/guilds/${guild}/community-v2`),
  });
  return { guild, query, discord: useDiscord(guild), client: useQueryClient() };
}
export function useWrite(guild: string, client: ReturnType<typeof useQueryClient>) {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  async function write(path: string, method: "PUT" | "DELETE", payload?: unknown) {
    setBusy(true);
    setNotice("");
    try {
      await api(`/api/guilds/${guild}/community-v2${path}`, method, payload);
      await client.invalidateQueries({ queryKey: ["community-v2", guild] });
      setNotice(t("Saved."));
      return true;
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not save."));
      return false;
    } finally {
      setBusy(false);
    }
  }
  return { busy, notice, write };
}
export function State({ query }: { query: ReturnType<typeof useTools>["query"] }) {
  if (query.isPending) return <Loading label={t("Loading community tools")} />;
  if (query.isError) return <Problem error={query.error} />;
  return null;
}
