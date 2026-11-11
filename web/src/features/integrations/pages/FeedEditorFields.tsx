import { Field, SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { PlaceholderChips } from "../../../components/PlaceholderChips";
import { WordChips } from "./FeedPresentation";
import type { useFeedEditor } from "./useFeedEditor";

export function FeedEditorFields({ model }: { model: ReturnType<typeof useFeedEditor> }) {
  const {
    t,
    list,
    draft,
    info,
    problem,
    set,
    tooLong,
    messageLength,
    kindInfo,
    area,
    insert,
    includeProblem,
    excludeProblem,
  } = model;
  return (
    <>
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
            problem("message") ?? (tooLong ? t("at-most-max-characters", { max: list.message_max }) : null)
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
    </>
  );
}
