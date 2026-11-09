import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { Link, useLocation, useNavigate, useOutletContext, useParams, useSearchParams } from "react-router";
import { Field, SwitchRow } from "../../../components/Field";
import { MessagePreview } from "../../../components/MessagePreview";
import { Panel } from "../../../components/Panel";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { PlaceholderChips } from "../../../components/PlaceholderChips";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { serverText } from "../../../i18n";
import { ApiError, type FieldError, type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useDebounced } from "../../../lib/useDebounced";
import "../../../styles/feeds.css";
import { NotFound } from "../../../components/NotFound";
import { PageHead } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import {
  KIND_TAG,
  SOURCE_HELP,
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
  wordsProblem,
  type Feed,
  type FeedDraft,
  type FeedKind,
  type FeedList,
  type Found,
} from "../api/feeds";
import { FeedWordsContext, useFeedWords } from "../api/feedsWords";
import { DeleteFeedDialog, Empty, FeedRow, FoundCard, WordChips } from "./FeedPresentation";
import { MyChannels } from "./MyFeedChannels";
import { CARD_LABEL, KIND_LINE, channelName, kindLabel } from "./feedLabels";

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
  const t = useFeedWords();
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
          title={t("social-feeds")}
          lead={t("new-videos-posts-and-episodes-from-the-places")}
          action={
            <PixelLink
              href={`/g/${guild}/feeds/new`}
              aria-disabled={list.data ? list.data.items.length >= list.data.max : false}
              className={list.data && list.data.items.length >= list.data.max ? "is-disabled" : ""}
            >
              {t("add-a-feed")}
            </PixelLink>
          }
        />
      ) : (
        <PageHead
          title={page === "new" ? t("add-a-feed-text") : t("edit-feed")}
          lead={
            page === "new"
              ? t("pick-where-the-posts-come-from-check-the")
              : t("change-where-the-posts-go-and-how-they")
          }
          action={
            <PixelLink variant="secondary" href={`/g/${guild}/feeds`}>
              {t("back-to-the-list")}
            </PixelLink>
          }
        />
      )}
      {off && (
        <div className="notice notice-heart" role="note" data-testid="feeds-off">
          {t("the-social-feeds-module-is-switched-off-so")}{" "}
          <Link to={`/g/${guild}/modules`}>{t("modules")}</Link>
        </div>
      )}
      {list.isPending && <Loading label={t("reading-the-feeds")} />}
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
  const t = useFeedWords();
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
          t("posted-account-s-latest-item-in-channel", {
            account: feed.account,
            channel: channelName(info, feed.channel_id, t),
          }),
        ),
      onError: (error) => setFailure(error.message),
      onSettled: () => setTesting(null),
    });
  };

  return (
    <>
      <Panel
        title={t("following")}
        aside={
          <span
            className="muted counter"
            aria-label={t("count-of-max-feeds-used", { count: list.items.length, max: list.max })}
          >
            {list.items.length}/{list.max} {t("used")}
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
          <ul className="tag-list" aria-label={t("feeds")}>
            {list.items.map((feed) => (
              <FeedRow key={feed.id} {...{ feed, guild, list, info, save, testing, runTest, setAsking }} />
            ))}
          </ul>
        )}
        {full && (
          <p className="field-hint muted">{t("a-server-follows-at-most-max-feeds", { max: list.max })}</p>
        )}
      </Panel>
      <MyChannels list={list} />
      {asking && <DeleteFeedDialog {...{ asking, remove, setAsking, setToast }} />}
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}

// /Editor

function EditorFor({ guild, list, id }: { guild: string; list: FeedList; id: string | null }) {
  const t = useFeedWords();
  const [params] = useSearchParams();
  if (id === null) {
    const wanted = params.get("kind") as FeedKind | null;
    const kind =
      wanted && list.kinds.some((k) => k.key === wanted && k.available) ? wanted : ("youtube" as FeedKind);
    if (list.items.length >= list.max) {
      return (
        <Panel title={t("full")} tone="gold">
          <p>
            {t("this-server-already-follows-max-feeds-remove-one", { max: list.max })}{" "}
            <Link to={`/g/${guild}/feeds`}>{t("list")}</Link>
          </p>
        </Panel>
      );
    }
    return <FeedEditor key="new" guild={guild} list={list} feed={null} initial={blankDraft(kind)} />;
  }
  const feed = list.items.find((f) => f.id === id);
  if (!feed) return <Problem error={new ApiError(404, "not_found", t("this-feed-no-longer-exists"))} />;
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
  const t = useFeedWords();
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
  const includeProblem = wordsProblem(draft.filter_include, list.filter_words_max, list.filter_word_max, t);
  const excludeProblem = wordsProblem(draft.filter_exclude, list.filter_words_max, list.filter_word_max, t);
  const messageLength = [...(draft.message ?? "")].length;
  const tooLong = messageLength > list.message_max;
  const changes = saved ? changedFields(saved, draft).length : draft.source.trim() ? 1 : 0;
  const missing =
    !feed && !shown ? t("check-the-source-first") : draft.channel_id ? null : t("pick-a-channel");
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
    // Commit before restoring selection so a deferred focus cannot steal input from another field.
    flushSync(() => set("message", base.slice(0, start) + token + base.slice(end)));
    el?.focus();
    el?.setSelectionRange(start + token.length, start + token.length);
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
          const name = shown?.account ?? feed?.account ?? t("the-feed");
          navigate(`/g/${guild}/feeds`, {
            state: {
              toast: feed
                ? t("saved-name", { name })
                : t("following-name-new-posts-go-to-channel", {
                    name,
                    channel: channelName(info, draft.channel_id ?? "", t),
                  }),
            },
          });
        },
        onError: (error) => {
          if (error instanceof ApiError && error.errors.length > 0) {
            setErrors(error.errors);
            setFailure(
              error.errors.length > 1
                ? t("count-things-to-fix-marked-below", { count: error.errors.length })
                : error.message,
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
        setTested({
          ok: true,
          text: t("posted-the-latest-item-in-channel", { channel: channelName(info, feed.channel_id, t) }),
        }),
      onError: (error) => setTested({ ok: false, text: error.message }),
    });
  };

  const sourceError = problem("source") ?? (check.isError && !stale ? check.error.message : null);
  return (
    <>
      <div className="editor">
        <div className="editor-form">
          <Panel title={t("source")}>
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
                        <span className="visually-hidden"> {t("opens-in-a-new-tab")}</span>
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
                  aria-label={t("where-the-posts-come-from")}
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
                        <span>{t(CARD_LABEL[kind.key])}</span>
                      </span>
                      <span className="choice-detail muted">
                        {kind.available
                          ? t(KIND_LINE[kind.key])
                          : kind.why_not
                            ? serverText("unavailable", { unavailable: kind.why_not })
                            : t("not-available-on-this-instance")}
                      </span>
                    </label>
                  ))}
                </div>
                {!kindInfo?.available && kindInfo?.why_not && (
                  <p className="notice" role="note">
                    {kindInfo.why_not}
                  </p>
                )}
                <Field label={t(help.label)} htmlFor="feed-source" error={sourceError} hint={t(help.hint)}>
                  <div className="feed-source-row">
                    <input
                      id="feed-source"
                      className="pixel-input"
                      value={draft.source}
                      placeholder={t(help.placeholder)}
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
                      {check.isPending ? t("checking") : t("check")}
                    </PixelButton>
                  </div>
                </Field>
                <div className="feed-found-slot" aria-live="polite">
                  {check.isPending && <p className="muted feed-checking">{t("looking-it-up")}</p>}
                  {shown && !check.isPending && <FoundCard found={shown} />}
                </div>
              </>
            )}
          </Panel>

          <Panel title={t("where-it-posts")}>
            <div className="form-grid form-grid-2">
              <Field label={t("discord-channel")} htmlFor="feed-channel" error={problem("channel_id")}>
                <ChannelPicker
                  id="feed-channel"
                  info={info}
                  value={draft.channel_id}
                  onChange={(next) => set("channel_id", next)}
                  invalid={problem("channel_id") !== null}
                />
              </Field>
              <Field
                label={t("ping-a-role")}
                htmlFor="feed-role"
                error={problem("ping_role_id")}
                hint={t("pinged-with-each-new-post-it-must-be")}
              >
                <RolePicker
                  id="feed-role"
                  info={info}
                  value={draft.ping_role_id}
                  onChange={(next) => set("ping_role_id", next)}
                  none={t("nobody")}
                />
              </Field>
            </div>
          </Panel>

          {(draft.kind === "twitch" || draft.kind === "kick") && (
            <Panel title={t("live-now-role")}>
              <Field
                label={t("live-now-role")}
                htmlFor="feed-live-role"
                error={problem("live_role_id")}
                hint={t("live-now-role-hint")}
              >
                <RolePicker
                  id="feed-live-role"
                  info={info}
                  value={draft.live_role_id}
                  onChange={(next) => set("live_role_id", next)}
                  none={t("nobody")}
                />
              </Field>
            </Panel>
          )}

          <Panel title={t("message")}>
            <Field
              label={t("text")}
              htmlFor="feed-message"
              error={
                problem("message") ??
                (tooLong ? t("at-most-max-characters", { max: list.message_max }) : null)
              }
              aside={
                <span className={`counter ${tooLong ? "is-over" : ""}`}>
                  {messageLength}/{list.message_max}
                </span>
              }
              hint={t("empty-uses-the-default-shown-in-grey")}
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
                    {t("use-default")}
                  </PixelButton>
                )}
              </div>
            </Field>
            <SwitchRow
              title={t("pinchora-s-card")}
              control={
                <PixelToggle
                  label={t("pinchora-s-card")}
                  checked={draft.embed}
                  onChange={(next) => set("embed", next)}
                />
              }
            >
              {t("title-picture-and-summary-under-the-text-off")}
            </SwitchRow>
          </Panel>

          <Panel title={t("filters")}>
            <div className="form-grid form-grid-2">
              <Field
                label={t("only-titles-with")}
                htmlFor="feed-include"
                error={problem("filter_include") ?? includeProblem}
              >
                <input
                  id="feed-include"
                  className="pixel-input"
                  value={draft.filter_include}
                  placeholder={t("live-trailer")}
                  autoComplete="off"
                  aria-invalid={includeProblem ? true : undefined}
                  onChange={(event) => set("filter_include", event.target.value)}
                />
                <WordChips text={draft.filter_include} label={t("only-titles-with")} />
              </Field>
              <Field
                label={t("never-titles-with")}
                htmlFor="feed-exclude"
                error={problem("filter_exclude") ?? excludeProblem}
              >
                <input
                  id="feed-exclude"
                  className="pixel-input"
                  value={draft.filter_exclude}
                  placeholder={t("sponsored-shorts")}
                  autoComplete="off"
                  aria-invalid={excludeProblem ? true : undefined}
                  onChange={(event) => set("filter_exclude", event.target.value)}
                />
                <WordChips text={draft.filter_exclude} label={t("never-titles-with")} />
              </Field>
            </div>
            <p className="field-hint muted feed-filter-hint">
              {t("words-separated-by-commas-up-to-max-in", { max: list.filter_words_max })}
            </p>
            <div className="feed-switch">
              <SwitchRow
                title={t("switched-on")}
                control={
                  <PixelToggle
                    label={t("switched-on")}
                    checked={draft.enabled}
                    onChange={(next) => set("enabled", next)}
                  />
                }
              >
                {t("off-the-feed-stays-set-up-but-nothing")}
              </SwitchRow>
            </div>
          </Panel>
        </div>

        <aside className="editor-rail">
          <Panel title={t("preview")}>
            <div className={`preview-live ${preview.isFetching ? "is-busy" : ""}`} aria-live="polite">
              {preview.data ? (
                <MessagePreview preview={preview.data} label={t("feed-post-preview")} />
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
                  title={changes > 0 ? t("save-first-the-test-uses-the-saved-feed") : undefined}
                >
                  {test.isPending ? t("posting") : t("send-a-test-post")}
                </PixelButton>
                <p className="field-hint muted">
                  {changes > 0
                    ? t("save-first-the-test-uses-the-saved-feed")
                    : t("posts-the-latest-item-in-the-channel-now")}
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
