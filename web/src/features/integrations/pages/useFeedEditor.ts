import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useNavigate } from "react-router";
import { useFeedWords } from "../api/feedsWords";
import { ApiError, type FieldError } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useDebounced } from "../../../lib/useDebounced";
import { channelName } from "./feedLabels";
import {
  SOURCE_HELP,
  bodyOf,
  changedFields,
  wordsProblem,
  useCheckFeed,
  useFeedPreview,
  useSaveFeed,
  useTestFeed,
  type Feed,
  type FeedDraft,
  type FeedKind,
  type FeedList,
  type Found,
} from "../api/feeds";

export function useFeedEditor({
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
  return {
    guild,
    list,
    feed,
    initial,
    t,
    info,
    save,
    check,
    test,
    saved,
    draft,
    area,
    kindInfo,
    help,
    problem,
    set,
    runCheck,
    shown,
    includeProblem,
    excludeProblem,
    messageLength,
    tooLong,
    changes,
    missing,
    blocked,
    preview,
    pickKind,
    insert,
    onSave,
    runTest,
    sourceError,
    tested,
    failure,
    setDraft,
    setErrors,
    setFailure,
  };
}
