import { useCallback, useState } from "react";
import { Link, useLocation, useOutletContext, useParams } from "react-router";
import { PixelLink } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import "../../../styles/feeds.css";
import { NotFound } from "../../../components/NotFound";
import { PageHead } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { useDeleteFeed, useFeeds, useSaveFeed, useTestFeed, type Feed, type FeedList } from "../api/feeds";
import { FeedWordsContext, useFeedWords } from "../api/feedsWords";
import { DeleteFeedDialog, Empty, FeedRow } from "./FeedPresentation";
import { MyChannels } from "./MyFeedChannels";
import { channelName } from "./feedLabels";
import { EditorFor } from "./FeedEditor";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { feedsSummary } from "./feedsSummary";

export { MyFeedsChannels, MyFeedsChannelsRoute } from "./MyFeedChannels";

export function Feeds() {
  const overview = useOutletContext<Overview>();
  const list = useFeeds(overview.guild.id);
  if (list.isPending) return <Loading />;
  if (list.isError) return <Problem error={list.error} />;
  return (
    <FeedWordsContext.Provider value={list.data.strings}>
      <FeedsPage />
    </FeedWordsContext.Provider>
  );
}

/** `/g/:guild/feeds`, `/g/:guild/feeds/new` and `/g/:guild/feeds/:feedId`. */
function FeedsPage() {
  const text = useFeedWords();
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const rest = useParams()["*"] ?? "";
  const list = useFeeds(guild);
  const off = data.modules.find((m) => m.key === "feeds")?.enabled === false;
  const page =
    rest === "" ? "list" : rest === "new" ? "new" : /^\d+$/.test(rest) ? "edit" : ("missing" as const);
  if (page === "missing") return <NotFound />;
  return (
    <div className="settings-page">
      {page === "list" ? (
        <PageHead
          title={text("social-feeds")}
          lead={text("new-videos-posts-and-episodes-from-the-places")}
          action={
            <PixelLink
              href={`/g/${guild}/feeds/new`}
              aria-disabled={list.data ? list.data.items.length >= list.data.max : false}
              className={list.data && list.data.items.length >= list.data.max ? "is-disabled" : ""}
            >
              {text("add-a-feed")}
            </PixelLink>
          }
        />
      ) : (
        <PageHead
          title={page === "new" ? text("add-a-feed-text") : text("edit-feed")}
          lead={
            page === "new"
              ? text("pick-where-the-posts-come-from-check-the")
              : text("change-where-the-posts-go-and-how-they")
          }
          action={
            <PixelLink variant="secondary" href={`/g/${guild}/feeds`}>
              {text("back-to-the-list")}
            </PixelLink>
          }
        />
      )}
      {off && (
        <div className="notice notice-heart" role="note" data-testid="feeds-off">
          {text("the-social-feeds-module-is-switched-off-so")}{" "}
          <Link to={`/g/${guild}/modules`}>{text("modules")}</Link>
        </div>
      )}
      {list.isPending && <Loading label={text("reading-the-feeds")} />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && page === "list" && <FeedsList guild={guild} list={list.data} />}
      {list.isSuccess && page !== "list" && (
        <EditorFor guild={guild} list={list.data} id={page === "edit" ? rest : null} />
      )}
    </div>
  );
}

// /List

function FeedsList({ guild, list }: { guild: string; list: FeedList }) {
  const text = useFeedWords();
  const info = useDiscord(guild).data;
  const location = useLocation();
  const [toast, setToast] = useState<string | null>(
    (location.state as { toast?: string } | null)?.toast ?? null,
  );
  const clearToast = useCallback(() => setToast(null), []);
  const save = useSaveFeed(guild);
  const remove = useDeleteFeed(guild);
  const test = useTestFeed(guild);
  const [asking, setAsking] = useState<Feed | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const full = list.items.length >= list.max;

  const runTest = (feed: Feed) => {
    setTesting(feed.id);
    setFailure(null);
    test.mutate(feed.id, {
      onSuccess: () =>
        setToast(
          text("posted-account-s-latest-item-in-channel", {
            account: feed.account,
            channel: channelName(info, feed.channel_id, text),
          }),
        ),
      onError: (error) => setFailure(error.message),
      onSettled: () => setTesting(null),
    });
  };

  return (
    <>
      <SettingsRow
        title={text("following")}
        description={dt("Review this subscription and its delivery.")}
        summary={feedsSummary(list.items)}
        initialOpen
      >
        <span
          className="muted counter"
          aria-label={text("count-of-max-feeds-used", { count: list.items.length, max: list.max })}
        >
          {list.items.length}/{list.max} {text("used")}
        </span>
        {(failure || save.isError) && (
          <p className="field-error" role="alert">
            {failure ?? save.error?.message}
          </p>
        )}
        {list.items.length === 0 ? (
          <Empty guild={guild} list={list} />
        ) : (
          <ul className="tag-list" aria-label={text("feeds")}>
            {list.items.map((feed) => (
              <FeedRow key={feed.id} {...{ feed, guild, list, info, save, testing, runTest, setAsking }} />
            ))}
          </ul>
        )}
        {full && (
          <p className="field-hint muted">{text("a-server-follows-at-most-max-feeds", { max: list.max })}</p>
        )}
      </SettingsRow>
      <MyChannels list={list} />
      {asking && <DeleteFeedDialog {...{ asking, remove, setAsking, setToast }} />}
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}
