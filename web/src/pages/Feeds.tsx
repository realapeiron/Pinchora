import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { Link, useLocation, useNavigate, useOutletContext, useParams, useSearchParams } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { MessagePreview } from "../components/MessagePreview";
import { Panel } from "../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { PlaceholderChips } from "../components/PlaceholderChips";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type FieldError, type GuildInfo, type Overview } from "../lib/api";
import {
  type Feed,
  type FeedDraft,
  type FeedKind,
  type FeedList,
  type Found,
  KIND_TAG,
  SOURCE_HELP,
  ago,
  blankDraft,
  bodyOf,
  changedFields,
  draftOf,
  useCheckFeed,
  useDeleteFeed,
  useFeedPreview,
  useFeeds,
  useSaveFeed,
  useTestFeed,
  words,
  wordsProblem,
} from "../lib/feeds";
import { useDiscord } from "../lib/queries";
import { useDebounced } from "../lib/useDebounced";
import "../styles/feeds.css";
import { NotFound } from "./NotFound";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

/** `/g/:guild/feeds`, `/g/:guild/feeds/new` and `/g/:guild/feeds/:feedId`. */
export function Feeds() {
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
          title="SOCIAL FEEDS"
          lead="New videos, posts and episodes from the places your server follows, posted in a channel as they appear."
          action={
            <PixelLink
              href={`/g/${guild}/feeds/new`}
              aria-disabled={list.data ? list.data.items.length >= list.data.max : false}
              className={list.data && list.data.items.length >= list.data.max ? "is-disabled" : ""}
            >
              Add a feed
            </PixelLink>
          }
        />
      ) : (
        <PageHead
          title={page === "new" ? "ADD A FEED" : "EDIT FEED"}
          lead={
            page === "new"
              ? "Pick where the posts come from, check the source, then choose where they go."
              : "Change where the posts go and how they look. The source stays."
          }
          action={
            <PixelLink variant="secondary" href={`/g/${guild}/feeds`}>
              Back to the list
            </PixelLink>
          }
        />
      )}
      {off && (
        <div className="notice notice-heart" role="note" data-testid="feeds-off">
          <strong>The Social feeds module is switched off,</strong> so nothing is posted. Switch it on on the{" "}
          <Link to={`/g/${guild}/modules`}>Modules</Link> page.
        </div>
      )}
      {list.isPending && <Loading label="Reading the feeds" />}
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
        setToast(`Posted ${feed.account}'s latest item in ${channelName(info, feed.channel_id)}.`),
      onError: (error) => setFailure(error.message),
      onSettled: () => setTesting(null),
    });
  };

  return (
    <>
      <Panel
        title="FOLLOWING"
        aside={
          <span className="muted counter" aria-label={`${list.items.length} of ${list.max} feeds used`}>
            {list.items.length}/{list.max} used
          </span>
        }
      >
        {(failure || save.isError) && (
          <p className="field-error" role="alert">
            {failure ?? save.error?.message}
          </p>
        )}
        {list.items.length === 0 ? (
          <Empty guild={guild} list={list} />
        ) : (
          <ul className="tag-list" aria-label="Feeds">
            {list.items.map((feed) => (
              <li key={feed.id} className="tag-row feed-row" data-feed={feed.account}>
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
                    <span className="visually-hidden"> posting in </span>
                    <ChannelTag info={info} id={feed.channel_id} />
                    {feed.ping_role_id && (
                      <>
                        <span aria-hidden="true"> · </span>
                        <span className="visually-hidden">, pinging </span>
                        <RoleTag info={info} id={feed.ping_role_id} />
                      </>
                    )}
                  </p>
                  <p className={`feed-health is-${feed.health.state}`}>
                    <span className="feed-dot" aria-hidden="true" />
                    {feed.health.text}
                  </p>
                  <p className="tag-facts muted">
                    Checked {ago(feed.health.last_checked_at)} · last post {ago(feed.health.last_sent_at)} ·
                    every {feed.health.interval_minutes} min
                    {filterCount(feed) > 0 &&
                      ` · ${filterCount(feed)} filter word${filterCount(feed) === 1 ? "" : "s"}`}
                  </p>
                </div>
                <div className="tag-actions">
                  <PixelToggle
                    label={`Feed ${feed.account}`}
                    checked={feed.enabled}
                    disabled={save.isPending}
                    onChange={(enabled) => save.mutate({ id: feed.id, body: { enabled } })}
                  />
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={`Send a test post of ${feed.account}`}
                    disabled={testing !== null}
                    onClick={() => runTest(feed)}
                  >
                    {testing === feed.id ? "Posting…" : "Test"}
                  </PixelButton>
                  <PixelLink
                    variant="secondary"
                    className="button-small"
                    href={`/g/${guild}/feeds/${feed.id}`}
                  >
                    Edit
                  </PixelLink>
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={`Delete ${feed.account}`}
                    onClick={() => setAsking(feed)}
                  >
                    Delete
                  </PixelButton>
                </div>
              </li>
            ))}
          </ul>
        )}
        {full && <p className="field-hint muted">A server follows at most {list.max} feeds.</p>}
      </Panel>
      {asking && (
        <div className="dialog-backdrop" role="presentation">
          <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="delete-title">
            <div className="panel-body">
              <h2 id="delete-title" className="pixel-title">
                Stop following “{asking.account}”?
              </h2>
              <p>Nothing more is posted from it. Posts already in the channel stay.</p>
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
                  Keep it
                </PixelButton>
                <PixelButton
                  disabled={remove.isPending}
                  onClick={() =>
                    remove.mutate(asking.id, {
                      onSuccess: () => {
                        setToast(`Stopped following ${asking.account}.`);
                        setAsking(null);
                      },
                    })
                  }
                >
                  {remove.isPending ? "Removing…" : "Stop following"}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}

const KIND_LINE: Record<FeedKind, string> = {
  youtube: "New videos from a channel.",
  rss: "Blogs, news sites and podcasts.",
  reddit: "The newest posts of a subreddit.",
  bluesky: "New posts of an account.",
  bridge: "X, Instagram and TikTok, through an RSS bridge.",
};

const CARD_LABEL: Record<FeedKind, string> = {
  youtube: "YouTube",
  rss: "RSS & podcasts",
  reddit: "Reddit",
  bluesky: "Bluesky",
  bridge: "X · Instagram · TikTok",
};

/** No feed yet: what can be followed, each a way into the form. */
function Empty({ guild, list }: { guild: string; list: FeedList }) {
  return (
    <div className="feed-empty">
      <p className="feed-empty-lead">Nothing followed yet. Pick a place to start:</p>
      <ul className="feed-kinds">
        {list.kinds.map((kind) => (
          <li key={kind.key}>
            {kind.available ? (
              <Link className="feed-kind-card" to={`/g/${guild}/feeds/new?kind=${kind.key}`}>
                <span className={`feed-kind feed-kind-${kind.key}`} aria-hidden="true">
                  {KIND_TAG[kind.key]}
                </span>
                <span className="feed-kind-card-text">
                  <strong>{CARD_LABEL[kind.key]}</strong>
                  <span className="muted">{KIND_LINE[kind.key]}</span>
                </span>
              </Link>
            ) : (
              <span className="feed-kind-card is-locked" title={kind.why_not ?? undefined}>
                <span className={`feed-kind feed-kind-${kind.key}`} aria-hidden="true">
                  {KIND_TAG[kind.key]}
                </span>
                <span className="feed-kind-card-text">
                  <strong>{CARD_LABEL[kind.key]}</strong>
                  <span className="muted">Not available on this instance.</span>
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

function kindLabel(list: FeedList, kind: FeedKind): string {
  const label = list.kinds.find((k) => k.key === kind)?.label ?? kind;
  return kind === "rss" ? "RSS" : label;
}

function channelName(info: GuildInfo | undefined, id: string): string {
  const channel = info?.channels.find((c) => c.id === id);
  return channel ? `#${channel.name}` : "its channel";
}

// /Editor

function EditorFor({ guild, list, id }: { guild: string; list: FeedList; id: string | null }) {
  const [params] = useSearchParams();
  if (id === null) {
    const wanted = params.get("kind") as FeedKind | null;
    const kind =
      wanted && list.kinds.some((k) => k.key === wanted && k.available) ? wanted : ("youtube" as FeedKind);
    if (list.items.length >= list.max) {
      return (
        <Panel title="FULL" tone="gold">
          <p>
            This server already follows {list.max} feeds. Remove one on the{" "}
            <Link to={`/g/${guild}/feeds`}>list</Link> first.
          </p>
        </Panel>
      );
    }
    return <FeedEditor key="new" guild={guild} list={list} feed={null} initial={blankDraft(kind)} />;
  }
  const feed = list.items.find((f) => f.id === id);
  if (!feed) return <Problem error={new ApiError(404, "not_found", "This feed no longer exists.")} />;
  return <FeedEditor key={id} guild={guild} list={list} feed={feed} initial={draftOf(feed)} />;
}

function FeedEditor({
  guild,
  list,
  feed,
  initial,
}: {
  guild: string;
  list: FeedList;
  feed: Feed | null;
  initial: FeedDraft;
}) {
  const navigate = useNavigate();
  const info = useDiscord(guild).data;
  const save = useSaveFeed(guild);
  const check = useCheckFeed(guild);
  const test = useTestFeed(guild);
  const [saved, setSaved] = useState<FeedDraft | null>(feed ? initial : null);
  const [draft, setDraft] = useState(initial);
  const [found, setFound] = useState<Found | null>(null);
  const [errors, setErrors] = useState<FieldError[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [tested, setTested] = useState<{ ok: boolean; text: string } | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  const kindInfo = list.kinds.find((k) => k.key === draft.kind);
  const help = SOURCE_HELP[draft.kind];

  const problem = (field: string) => errors.find((e) => e.field === field)?.message ?? null;
  const set = <K extends keyof FeedDraft>(key: K, value: FeedDraft[K]) => {
    setDraft((d) => ({ ...d, [key]: value }));
    setErrors((list) => list.filter((e) => e.field !== key));
    setFailure(null);
  };

  // A new source is checked once typing stops; the result belongs to what was checked.
  const settledSource = useDebounced(draft.source.trim(), 700);
  const checkedFor = useRef("");
  const checkMutate = check.mutate;
  const runCheck = useCallback(
    (kind: FeedKind, source: string) => {
      checkedFor.current = `${kind}:${source}`;
      checkMutate(
        { kind, source },
        {
          onSuccess: (result) => {
            if (checkedFor.current === `${kind}:${source}`) setFound(result);
          },
        },
      );
    },
    [checkMutate],
  );
  useEffect(() => {
    if (feed) return;
    if (settledSource === "" || !kindInfo?.available) return;
    if (checkedFor.current === `${draft.kind}:${settledSource}`) return;
    runCheck(draft.kind, settledSource);
  }, [feed, settledSource, draft.kind, kindInfo, runCheck]);
  // An existing feed is read once, so the preview shows its real latest post.
  const feedKind = feed?.kind;
  const feedSource = feed?.source;
  useEffect(() => {
    if (feedKind && feedSource) runCheck(feedKind, feedSource);
  }, [feedKind, feedSource, runCheck]);

  const stale = !feed && found !== null && checkedFor.current !== `${draft.kind}:${draft.source.trim()}`;
  const shown = stale ? null : found;
  const includeProblem = wordsProblem(draft.filter_include, list.filter_words_max, list.filter_word_max);
  const excludeProblem = wordsProblem(draft.filter_exclude, list.filter_words_max, list.filter_word_max);
  const messageLength = [...(draft.message ?? "")].length;
  const tooLong = messageLength > list.message_max;
  const changes = saved ? changedFields(saved, draft).length : draft.source.trim() ? 1 : 0;
  const missing = !feed && !shown ? "Check the source first" : draft.channel_id ? null : "Pick a channel";
  const blocked = missing !== null || includeProblem !== null || excludeProblem !== null || tooLong;

  const previewBody = useMemo(
    () => ({
      kind: draft.kind,
      source: shown?.source ?? feed?.source,
      account: shown?.account ?? feed?.account,
      message: draft.message?.trim() ? draft.message : null,
      embed: draft.embed,
      ping_role_id: draft.ping_role_id,
      item: shown?.latest ?? undefined,
    }),
    [draft, shown, feed],
  );
  const key = JSON.stringify(previewBody);
  const settledKey = useDebounced(key, 400);
  const settledBody = useMemo(() => JSON.parse(settledKey) as unknown, [settledKey]);
  const preview = useFeedPreview(guild, settledBody, !tooLong);

  const pickKind = (kind: FeedKind) => {
    setDraft((d) => ({ ...d, kind, source: "", embed: kind !== "youtube" }));
    setFound(null);
    checkedFor.current = "";
    check.reset();
    setErrors([]);
  };

  const insert = (token: string) => {
    const el = area.current;
    const base = draft.message ?? kindInfo?.default_message ?? "";
    const [start, end] =
      el && draft.message !== null ? [el.selectionStart, el.selectionEnd] : [base.length, base.length];
    set("message", base.slice(0, start) + token + base.slice(end));
    requestAnimationFrame(() => {
      if (!el) return;
      el.focus();
      el.setSelectionRange(start + token.length, start + token.length);
    });
  };

  const onSave = () => {
    if (blocked) return;
    setErrors([]);
    setFailure(null);
    const body = bodyOf({ ...draft, source: shown?.source ?? draft.source }, saved);
    save.mutate(
      { id: feed?.id, body },
      {
        onSuccess: () => {
          // Saved before leaving, so the unsaved-changes guard lets the page go.
          flushSync(() => setSaved(draft));
          const name = shown?.account ?? feed?.account ?? "the feed";
          navigate(`/g/${guild}/feeds`, {
            state: {
              toast: feed
                ? `Saved ${name}.`
                : `Following ${name}. New posts go to ${channelName(info, draft.channel_id ?? "")}.`,
            },
          });
        },
        onError: (error) => {
          if (error instanceof ApiError && error.errors.length > 0) {
            setErrors(error.errors);
            setFailure(
              error.errors.length > 1 ? `${error.errors.length} things to fix, marked below` : error.message,
            );
          } else {
            setFailure(error.message);
          }
        },
      },
    );
  };

  const runTest = () => {
    if (!feed) return;
    setTested(null);
    test.mutate(feed.id, {
      onSuccess: () =>
        setTested({ ok: true, text: `Posted the latest item in ${channelName(info, feed.channel_id)}.` }),
      onError: (error) => setTested({ ok: false, text: error.message }),
    });
  };

  const sourceError = problem("source") ?? (check.isError && !stale ? check.error.message : null);
  return (
    <>
      <div className="editor">
        <div className="editor-form">
          <Panel title="SOURCE">
            {feed ? (
              <div className="feed-source-read">
                <span className={`feed-kind feed-kind-${feed.kind}`} aria-hidden="true">
                  {KIND_TAG[feed.kind]}
                </span>
                <div>
                  <p className="feed-source-name">
                    {feed.link ? (
                      <a href={feed.link} target="_blank" rel="noopener noreferrer">
                        {feed.account}
                        <span className="visually-hidden"> (opens in a new tab)</span>
                      </a>
                    ) : (
                      feed.account
                    )}
                  </p>
                  <p className="muted">
                    {kindLabel(list, feed.kind)} · <code>{feed.source}</code>
                  </p>
                </div>
              </div>
            ) : (
              <>
                <div
                  className="choice feed-kind-choice"
                  role="radiogroup"
                  aria-label="Where the posts come from"
                >
                  {list.kinds.map((kind) => (
                    <label
                      key={kind.key}
                      className={`choice-card ${draft.kind === kind.key ? "is-checked" : ""} ${kind.available ? "" : "is-locked"}`}
                    >
                      <input
                        type="radio"
                        name="kind"
                        value={kind.key}
                        checked={draft.kind === kind.key}
                        disabled={!kind.available}
                        onChange={() => pickKind(kind.key)}
                      />
                      <span className="choice-title feed-choice-title">
                        <span className={`feed-kind feed-kind-${kind.key}`} aria-hidden="true">
                          {KIND_TAG[kind.key]}
                        </span>
                        <span>{CARD_LABEL[kind.key]}</span>
                      </span>
                      <span className="choice-detail muted">
                        {kind.available ? KIND_LINE[kind.key] : "Not available on this instance."}
                      </span>
                    </label>
                  ))}
                </div>
                {!kindInfo?.available && kindInfo?.why_not && (
                  <p className="notice" role="note">
                    {kindInfo.why_not}
                  </p>
                )}
                <Field label={help.label} htmlFor="feed-source" error={sourceError} hint={help.hint}>
                  <div className="feed-source-row">
                    <input
                      id="feed-source"
                      className="pixel-input"
                      value={draft.source}
                      placeholder={help.placeholder}
                      maxLength={500}
                      autoComplete="off"
                      spellCheck={false}
                      aria-invalid={sourceError ? true : undefined}
                      aria-describedby="feed-source-hint"
                      onChange={(event) => set("source", event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" && draft.source.trim())
                          runCheck(draft.kind, draft.source.trim());
                      }}
                    />
                    <PixelButton
                      variant="secondary"
                      disabled={!draft.source.trim() || check.isPending || !kindInfo?.available}
                      onClick={() => runCheck(draft.kind, draft.source.trim())}
                    >
                      {check.isPending ? "Checking…" : "Check"}
                    </PixelButton>
                  </div>
                </Field>
                <div className="feed-found-slot" aria-live="polite">
                  {check.isPending && <p className="muted feed-checking">Looking it up…</p>}
                  {shown && !check.isPending && <FoundCard found={shown} />}
                </div>
              </>
            )}
          </Panel>

          <Panel title="WHERE IT POSTS">
            <div className="form-grid form-grid-2">
              <Field label="Discord channel" htmlFor="feed-channel" error={problem("channel_id")}>
                <ChannelPicker
                  id="feed-channel"
                  info={info}
                  value={draft.channel_id}
                  onChange={(next) => set("channel_id", next)}
                  invalid={problem("channel_id") !== null}
                />
              </Field>
              <Field
                label="Ping a role"
                htmlFor="feed-role"
                error={problem("ping_role_id")}
                hint="Pinged with each new post; it must be mentionable."
              >
                <RolePicker
                  id="feed-role"
                  info={info}
                  value={draft.ping_role_id}
                  onChange={(next) => set("ping_role_id", next)}
                  none="Nobody"
                />
              </Field>
            </div>
          </Panel>

          <Panel title="MESSAGE">
            <Field
              label="Text"
              htmlFor="feed-message"
              error={problem("message") ?? (tooLong ? `At most ${list.message_max} characters.` : null)}
              aside={
                <span className={`counter ${tooLong ? "is-over" : ""}`}>
                  {messageLength}/{list.message_max}
                </span>
              }
              hint="Empty uses the default shown in grey."
            >
              <textarea
                id="feed-message"
                ref={area}
                className="pixel-input pixel-textarea"
                rows={3}
                value={draft.message ?? ""}
                placeholder={kindInfo?.default_message}
                aria-invalid={tooLong || problem("message") ? true : undefined}
                onChange={(event) => set("message", event.target.value === "" ? null : event.target.value)}
              />
              <div className="feed-chips">
                <PlaceholderChips placeholders={list.placeholders} onPick={insert} />
                {draft.message !== null && (
                  <PixelButton variant="ghost" className="chip-reset" onClick={() => set("message", null)}>
                    Use default
                  </PixelButton>
                )}
              </div>
            </Field>
            <SwitchRow
              title="Pinchora's card"
              control={
                <PixelToggle
                  label="Pinchora's card"
                  checked={draft.embed}
                  onChange={(next) => set("embed", next)}
                />
              }
            >
              Title, picture and summary under the text. Off, Discord shows its own preview of the link (for
              YouTube, its video player).
            </SwitchRow>
          </Panel>

          <Panel title="FILTERS">
            <div className="form-grid form-grid-2">
              <Field
                label="Only titles with"
                htmlFor="feed-include"
                error={problem("filter_include") ?? includeProblem}
              >
                <input
                  id="feed-include"
                  className="pixel-input"
                  value={draft.filter_include}
                  placeholder="live, trailer"
                  autoComplete="off"
                  aria-invalid={includeProblem ? true : undefined}
                  onChange={(event) => set("filter_include", event.target.value)}
                />
                <WordChips text={draft.filter_include} label="Only titles with" />
              </Field>
              <Field
                label="Never titles with"
                htmlFor="feed-exclude"
                error={problem("filter_exclude") ?? excludeProblem}
              >
                <input
                  id="feed-exclude"
                  className="pixel-input"
                  value={draft.filter_exclude}
                  placeholder="sponsored, #shorts"
                  autoComplete="off"
                  aria-invalid={excludeProblem ? true : undefined}
                  onChange={(event) => set("filter_exclude", event.target.value)}
                />
                <WordChips text={draft.filter_exclude} label="Never titles with" />
              </Field>
            </div>
            <p className="field-hint muted feed-filter-hint">
              Words separated by commas, up to {list.filter_words_max} in each list; capital letters don't
              matter. With no “only” words, every post goes out.
            </p>
            <div className="feed-switch">
              <SwitchRow
                title="Switched on"
                control={
                  <PixelToggle
                    label="Switched on"
                    checked={draft.enabled}
                    onChange={(next) => set("enabled", next)}
                  />
                }
              >
                Off, the feed stays set up but nothing is posted.
              </SwitchRow>
            </div>
          </Panel>
        </div>

        <aside className="editor-rail">
          <Panel title="PREVIEW">
            <div className={`preview-live ${preview.isFetching ? "is-busy" : ""}`} aria-live="polite">
              {preview.data ? (
                <MessagePreview preview={preview.data} label="Feed post preview" />
              ) : preview.isError ? (
                <p className="muted">{preview.error.message}</p>
              ) : (
                <p className="muted">…</p>
              )}
            </div>
            {preview.data && preview.data.notes.length > 0 && (
              <ul className="preview-notes muted">
                {preview.data.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            )}
            {feed && (
              <div className="feed-test">
                <PixelButton
                  variant="secondary"
                  onClick={runTest}
                  disabled={test.isPending || changes > 0}
                  title={changes > 0 ? "Save first: the test uses the saved feed." : undefined}
                >
                  {test.isPending ? "Posting…" : "Send a test post"}
                </PixelButton>
                <p className="field-hint muted">
                  {changes > 0
                    ? "Save first: the test uses the saved feed."
                    : "Posts the latest item in the channel now. Nobody is pinged."}
                </p>
                {tested && changes === 0 && (
                  <p
                    className={tested.ok ? "feed-test-ok" : "field-error"}
                    role={tested.ok ? "status" : "alert"}
                  >
                    {tested.text}
                  </p>
                )}
              </div>
            )}
          </Panel>
        </aside>
      </div>
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={blocked}
        error={failure ?? (changes > 0 ? missing : null)}
        onSave={onSave}
        onReset={() => {
          setDraft(saved ?? initial);
          setErrors([]);
          setFailure(null);
        }}
      />
    </>
  );
}

/** What a check found: the source's name, its link, and its latest post. */
function FoundCard({ found }: { found: Found }) {
  const latest = found.latest;
  return (
    <div className="feed-found" data-testid="feed-found">
      <p className="feed-found-head">
        <span className="badge badge-mint">FOUND</span>
        <strong className="feed-found-name">
          {found.link ? (
            <a href={found.link} target="_blank" rel="noopener noreferrer">
              {found.account}
              <span className="visually-hidden"> (opens in a new tab)</span>
            </a>
          ) : (
            found.account
          )}
        </strong>
      </p>
      {latest ? (
        <p className="feed-found-latest">
          <span className="muted">Latest: </span>“{latest.title || "untitled"}”
          {latest.published && <span className="muted"> · {ago(latest.published)}</span>}
        </p>
      ) : (
        <p className="muted">Nothing posted yet.</p>
      )}
      <p className="muted feed-found-note">
        What is there now is not posted; new posts are, checked about every {found.interval_minutes} min.
      </p>
    </div>
  );
}

function WordChips({ text, label }: { text: string; label: string }) {
  const list = words(text);
  if (list.length === 0) return null;
  return (
    <ul className="feed-words" aria-label={`${label}: ${list.length} words`}>
      {list.map((word) => (
        <li key={word} className="feed-word">
          {word}
        </li>
      ))}
    </ul>
  );
}
