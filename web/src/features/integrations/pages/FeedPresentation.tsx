import { Link } from "react-router";
import { serverText } from "../../../i18n";
import type { GuildInfo } from "../../../lib/api";
import { useFeedWords } from "../api/feedsWords";
import { CARD_LABEL, KIND_LINE, kindLabel } from "./feedLabels";

import { ChannelTag, RoleTag } from "../../../components/Pickers";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { KIND_TAG, ago, useDeleteFeed, useSaveFeed, type Feed, type FeedList } from "../api/feeds";

import { words, type Found } from "../api/feeds";

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
  const t = useFeedWords();
  return (
    <li className="tag-row feed-row" data-feed={feed.account}>
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
          <span className="visually-hidden"> {t("posting-in")} </span>
          <ChannelTag info={info} id={feed.channel_id} />
          {feed.ping_role_id && (
            <>
              <span aria-hidden="true"> · </span>
              <span className="visually-hidden">{t("pinging")} </span>
              <RoleTag info={info} id={feed.ping_role_id} />
            </>
          )}
          {feed.live_role_id && (
            <>
              <span aria-hidden="true"> · </span>
              <span className="visually-hidden">{t("live-now-role")} </span>
              <RoleTag info={info} id={feed.live_role_id} />
            </>
          )}
        </p>
        <p className={`feed-health is-${feed.health.state}`}>
          <span className="feed-dot" aria-hidden="true" />
          {feed.health.text}
        </p>
        <p className="tag-facts muted">
          {t("checked")} {ago(feed.health.last_checked_at, t)} {t("last-post")}{" "}
          {ago(feed.health.last_sent_at, t)} {t("every")} {feed.health.interval_minutes} {t("min")}
          {filterCount(feed) > 0 && t("count-filter-words", { count: filterCount(feed) })}
        </p>
      </div>
      <div className="tag-actions">
        <PixelToggle
          label={t("feed-account", { account: feed.account })}
          checked={feed.enabled}
          disabled={save.isPending}
          onChange={(enabled) => save.mutate({ id: feed.id, body: { enabled } })}
        />
        <PixelButton
          variant="ghost"
          className="button-small"
          aria-label={t("send-a-test-post-of-account", { account: feed.account })}
          disabled={testing !== null}
          onClick={() => runTest(feed)}
        >
          {testing === feed.id ? t("posting") : t("test")}
        </PixelButton>
        <PixelLink variant="secondary" className="button-small" href={`/g/${guild}/feeds/${feed.id}`}>
          {t("edit")}
        </PixelLink>
        <PixelButton
          variant="ghost"
          className="button-small"
          aria-label={t("delete-account", { account: feed.account })}
          onClick={() => setAsking(feed)}
        >
          {t("delete")}
        </PixelButton>
      </div>
    </li>
  );
}
interface DeleteFeedProps {
  asking: Feed;
  remove: ReturnType<typeof useDeleteFeed>;
  setAsking: (feed: Feed | null) => void;
  setToast: (text: string) => void;
}
export function DeleteFeedDialog({ asking, remove, setAsking, setToast }: DeleteFeedProps) {
  const t = useFeedWords();
  return (
    <div className="dialog-backdrop" role="presentation">
      <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="delete-title">
        <div className="panel-body">
          <h2 id="delete-title" className="pixel-title">
            {t("stop-following-account", { account: asking.account })}
          </h2>
          <p>{t("nothing-more-is-posted-from-it-posts-already")}</p>
          {remove.isError && (
            <p className="field-error" role="alert">
              {remove.error.message}
            </p>
          )}
          <div className="dialog-actions">
            <PixelButton
              variant="secondary"
              autoFocus
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  setAsking(null);
                  remove.reset();
                }
              }}
              onClick={() => {
                setAsking(null);
                remove.reset();
              }}
            >
              {t("keep-it")}
            </PixelButton>
            <PixelButton
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(asking.id, {
                  onSuccess: () => {
                    setToast(t("stopped-following-account", { account: asking.account }));
                    setAsking(null);
                  },
                })
              }
            >
              {remove.isPending ? t("removing") : t("stop-following")}
            </PixelButton>
          </div>
        </div>
      </div>
    </div>
  );
}
export function Empty({ guild, list }: { guild: string; list: FeedList }) {
  const t = useFeedWords();
  return (
    <div className="feed-empty">
      <p className="feed-empty-lead">{t("nothing-followed-yet-pick-a-place-to-start")}</p>
      <ul className="feed-kinds">
        {list.kinds.map((kind) => (
          <li key={kind.key}>
            {kind.available ? (
              <Link className="feed-kind-card" to={`/g/${guild}/feeds/new?kind=${kind.key}`}>
                <span className={`feed-kind feed-kind-${kind.key}`} aria-hidden="true">
                  {KIND_TAG[kind.key]}
                </span>
                <span className="feed-kind-card-text">
                  <strong>{t(CARD_LABEL[kind.key])}</strong>
                  <span className="muted">{t(KIND_LINE[kind.key])}</span>
                </span>
              </Link>
            ) : (
              <span
                className="feed-kind-card is-locked"
                title={kind.why_not ? serverText("unavailable", { unavailable: kind.why_not }) : undefined}
              >
                <span className={`feed-kind feed-kind-${kind.key}`} aria-hidden="true">
                  {KIND_TAG[kind.key]}
                </span>
                <span className="feed-kind-card-text">
                  <strong>{t(CARD_LABEL[kind.key])}</strong>
                  <span className="muted">
                    {kind.why_not
                      ? serverText("unavailable", { unavailable: kind.why_not })
                      : t("not-available-on-this-instance")}
                  </span>
                </span>
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function filterCount(feed: Feed): number {
  return feed.filter_include.length + feed.filter_exclude.length;
}
/** What a check found: the source's name, its link, and its latest post. */
export function FoundCard({ found }: { found: Found }) {
  const t = useFeedWords();
  const latest = found.latest;
  return (
    <div className="feed-found" data-testid="feed-found">
      <p className="feed-found-head">
        <span className="badge badge-mint">{t("found")}</span>
        <strong className="feed-found-name">
          {found.link ? (
            <a href={found.link} target="_blank" rel="noopener noreferrer">
              {found.account}
              <span className="visually-hidden"> {t("opens-in-a-new-tab")}</span>
            </a>
          ) : (
            found.account
          )}
        </strong>
      </p>
      {latest ? (
        <p className="feed-found-latest">
          <span className="muted">{t("latest")} </span>“{latest.title || t("untitled")}”
          {latest.published && <span className="muted"> · {ago(latest.published, t)}</span>}
        </p>
      ) : (
        <p className="muted">{t("nothing-posted-yet")}</p>
      )}
      <p className="muted feed-found-note">
        {t("what-is-there-now-is-not-posted-new", { minutes: found.interval_minutes })}
      </p>
    </div>
  );
}

export function WordChips({ text, label }: { text: string; label: string }) {
  const t = useFeedWords();
  const list = words(text);
  if (list.length === 0) return null;
  return (
    <ul className="feed-words" aria-label={t("label-count-words", { label, count: list.length })}>
      {list.map((word) => (
        <li key={word} className="feed-word">
          {word}
        </li>
      ))}
    </ul>
  );
}
