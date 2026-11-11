import type { GuildInfo } from "../../../lib/api";
import type { TrackersDraft } from "../api/trackers";
import { t } from "../../../i18n/dashboard";

function channel(info: GuildInfo | undefined, id: string | null) {
  if (!id) return t("Off");
  const found = info?.channels.find((item) => item.id === id);
  return found ? `#${found.name}` : t("Channel selected");
}

export function trackersSummary(value: TrackersDraft, info?: GuildInfo) {
  return {
    lol: channel(info, value.patch_channels.lol ?? null),
    valorant: channel(info, value.patch_channels.valorant ?? null),
    mc: channel(info, value.patch_channels.mc ?? null),
    news: channel(info, value.mc_news_channel_id),
    watch: value.mc_watch.length
      ? t(value.mc_watch.length === 1 ? "{count} server watched" : "{count} servers watched", {
          count: value.mc_watch.length,
        })
      : t("No servers watched"),
  };
}

export function watchSummary(channelId: string, info?: GuildInfo) {
  return channel(info, channelId);
}
