import type { GuildInfo } from "../../../lib/api";
import type { FeedKind, FeedList } from "../api/feeds";
import type { FeedWords } from "../api/feedsWords";

export function kindLabel(list: FeedList, kind: FeedKind): string {
  const label = list.kinds.find((k) => k.key === kind)?.label ?? kind;
  return kind === "rss" ? "RSS" : label;
}

export const KIND_LINE: Record<FeedKind, string> = {
  youtube: "new-videos-from-a-channel",
  rss: "blogs-news-sites-and-podcasts",
  reddit: "the-newest-posts-of-a-subreddit",
  bluesky: "new-posts-of-an-account",
  bridge: "x-instagram-and-tiktok-through-an-rss-bridge",
  twitch: "twitch-live-help",
  kick: "kick-live-help",
};

export const CARD_LABEL: Record<FeedKind, string> = {
  youtube: "youtube",
  rss: "rss-podcasts",
  reddit: "reddit",
  bluesky: "bluesky",
  bridge: "x-instagram-tiktok",
  twitch: "twitch",
  kick: "kick",
};

export function channelName(info: GuildInfo | undefined, id: string, t: FeedWords): string {
  const channel = info?.channels.find((c) => c.id === id);
  return channel ? `#${channel.name}` : t("its-channel");
}
