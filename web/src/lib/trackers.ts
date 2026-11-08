import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { serverText } from "../i18n";

export interface Watch {
  host: string;
  channel_id: string;
  message_id?: string | null;
}
export interface TrackersData {
  patch_channels: Record<string, string>;
  mc_watch: Watch[];
  mc_news_channel_id: string | null;
  language: string;
  strings: Record<string, string>;
}
export type TrackersDraft = Pick<TrackersData, "patch_channels" | "mc_news_channel_id"> & {
  mc_watch: Pick<Watch, "host" | "channel_id">[];
};

/** Only editable fields, with stable channel ordering for change detection. */
export function trackersDraft(value: TrackersDraft): TrackersDraft {
  return {
    patch_channels: Object.fromEntries(
      Object.entries(value.patch_channels).sort(([a], [b]) => a.localeCompare(b)),
    ),
    mc_watch: value.mc_watch.map(({ host, channel_id }) => ({ host, channel_id })),
    mc_news_channel_id: value.mc_news_channel_id,
  };
}
const key = (guild: string) => [`/api/guilds/${guild}/trackers`];
export function useTrackers(guild: string) {
  return useQuery({
    queryKey: key(guild),
    queryFn: () => api<TrackersData>(`/api/guilds/${guild}/trackers`),
  });
}
export function useSaveTrackers(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (draft: TrackersDraft) =>
      api<TrackersData>(`/api/guilds/${guild}/trackers`, "PUT", trackersDraft(draft)),
    onSuccess: async (data) => {
      client.setQueryData(key(guild), data);
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
    },
  });
}
export function words(data: TrackersData | undefined) {
  return (key: string) => serverText(key, data?.strings ?? {}, {}, data?.language ?? "en");
}
