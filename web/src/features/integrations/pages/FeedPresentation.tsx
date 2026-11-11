import { Link } from "react-router";
import { serverText } from "../../../i18n";
import { useFeedWords } from "../api/feedsWords";
import { CARD_LABEL, KIND_LINE } from "./feedLabels";

import { PixelButton } from "../../../components/PixelButton";
import { KIND_TAG, ago, useDeleteFeed, type Feed, type FeedList } from "../api/feeds";

import { words, type Found } from "../api/feeds";

export { FeedRow } from "./FeedRow";

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
              /* eslint-disable-next-line jsx-a11y/no-autofocus -- Dialog dismissal receives focus on open. */
              autoFocus
              variant="secondary"
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
