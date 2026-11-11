import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { serverText } from "../../../i18n";
import { KIND_TAG } from "../api/feeds";
import { FoundCard } from "./FeedPresentation";
import { CARD_LABEL, KIND_LINE, kindLabel } from "./feedLabels";
import type { useFeedEditor } from "./useFeedEditor";

export function FeedEditorSource({ model }: { model: ReturnType<typeof useFeedEditor> }) {
  const { t, feed, list, draft, kindInfo, help, sourceError, pickKind, set, runCheck, check, shown } = model;
  return (
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
                  if (event.key === "Enter" && draft.source.trim()) runCheck(draft.kind, draft.source.trim());
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
  );
}
