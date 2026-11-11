import { Link } from "react-router";
import type { GuildInfo } from "../../../lib/api";
import { ChannelTag, RoleTag } from "../../../components/Pickers";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { KIND_TAG, ago, useSaveFeed, type Feed, type FeedList } from "../api/feeds";
import { useFeedWords } from "../api/feedsWords";
import { kindLabel } from "./feedLabels";
import { feedSummary } from "./feedsSummary";

interface FeedRowProps {
  feed: Feed;
  guild: string;
  list: FeedList;
  info: GuildInfo | undefined;
  save: ReturnType<typeof useSaveFeed>;
  testing: string | null;
  runTest: (feed: Feed) => void;
  setAsking: (feed: Feed) => void;
}
export function FeedRow({ feed, guild, list, info, save, testing, runTest, setAsking }: FeedRowProps) {
  const text = useFeedWords();
  return (
    <li className="tag-row feed-row" data-feed={feed.account}>
      <SettingsRow
        title={feed.account}
        description={dt("Review this subscription and its delivery.")}
        summary={feedSummary(feed, list, info)}
        enabled={feed.enabled}
        onToggle={(enabled) => save.mutate({ id: feed.id, body: { enabled } })}
        toggleLabel={text("feed-account", { account: feed.account })}
        toggleDisabled={save.isPending}
      >
        <span className={`feed-kind feed-kind-${feed.kind}`} aria-hidden="true">
          {KIND_TAG[feed.kind]}
        </span>
        <div className="tag-main feed-main">
          <Link to={`/g/${guild}/feeds/${feed.id}`} className="tag-name">
            {feed.account}
          </Link>
          <p className="tag-detail muted">
            <span className="feed-kind-name">{kindLabel(list, feed.kind)}</span>
            <span aria-hidden="true"> → </span>
            <span className="visually-hidden"> {text("posting-in")} </span>
            <ChannelTag info={info} id={feed.channel_id} />
            {feed.ping_role_id && (
              <>
                <span aria-hidden="true"> · </span>
                <span className="visually-hidden">{text("pinging")} </span>
                <RoleTag info={info} id={feed.ping_role_id} />
              </>
            )}
            {feed.live_role_id && (
              <>
                <span aria-hidden="true"> · </span>
                <span className="visually-hidden">{text("live-now-role")} </span>
                <RoleTag info={info} id={feed.live_role_id} />
              </>
            )}
          </p>
          <p className={`feed-health is-${feed.health.state}`}>
            <span className="feed-dot" aria-hidden="true" />
            {feed.health.text}
          </p>
          <p className="tag-facts muted">
            {text("checked")} {ago(feed.health.last_checked_at, text)} {text("last-post")}{" "}
            {ago(feed.health.last_sent_at, text)} {text("every")} {feed.health.interval_minutes} {text("min")}
            {filterCount(feed) > 0 && text("count-filter-words", { count: filterCount(feed) })}
          </p>
        </div>
        <div className="tag-actions">
          <PixelButton
            variant="ghost"
            className="button-small"
            aria-label={text("send-a-test-post-of-account", { account: feed.account })}
            disabled={testing !== null}
            onClick={() => runTest(feed)}
          >
            {testing === feed.id ? text("posting") : text("test")}
          </PixelButton>
          <PixelLink variant="secondary" className="button-small" href={`/g/${guild}/feeds/${feed.id}`}>
            {text("edit")}
          </PixelLink>
          <PixelButton
            variant="ghost"
            className="button-small"
            aria-label={text("delete-account", { account: feed.account })}
            onClick={() => setAsking(feed)}
          >
            {text("delete")}
          </PixelButton>
        </div>
      </SettingsRow>
    </li>
  );
}
function filterCount(feed: Feed): number {
  return feed.filter_include.length + feed.filter_exclude.length;
}
