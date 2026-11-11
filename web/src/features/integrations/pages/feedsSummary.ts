import type { GuildInfo } from "../../../lib/api";
import type { Feed, FeedList } from "../api/feeds";
import { t } from "../../../i18n/dashboard";

export function feedsSummary(items: Feed[]) {
  return items.length
    ? t(items.length === 1 ? "{count} feed" : "{count} feeds", { count: items.length })
    : t("No feeds yet");
}

export function linkedChannelsSummary(count: number) {
  return count
    ? t(count === 1 ? "{count} linked channel" : "{count} linked channels", { count })
    : t("No channels linked");
}

export function feedSummary(feed: Feed, list: FeedList, info?: GuildInfo) {
  if (!feed.enabled) return t("Off");
  const kind = list.kinds.find((item) => item.key === feed.kind)?.label ?? feed.kind;
  const channel = info?.channels.find((item) => item.id === feed.channel_id);
  return `${kind} · ${channel ? `#${channel.name}` : t("Channel selected")} · ${feed.health.interval_minutes} ${t("min")}`;
}
