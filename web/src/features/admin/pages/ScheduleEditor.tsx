import { t as deferredText } from "../../../i18n/dashboard";
import { useContext } from "react";
import { EmbedEditor } from "../components/EmbedEditor";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { PixelButton } from "../../../components/PixelButton";
import type { GuildInfo } from "../../../lib/api";
import {
  TIMEZONE_NOTICE,
  firstRunUtc,
  newRecurrence,
  type Recurrence,
  type ScheduledDraft,
} from "../api/utility";
import { UtilityWords } from "./utilityWords";
import { TimeFields } from "./TimeFields";

export function ScheduleEditor({
  draft,
  editing,
  info,
  busy,
  error,
  onChange,
  onPreview,
  onCancel,
  onSave,
}: {
  draft: ScheduledDraft;
  editing: boolean;
  info: GuildInfo | undefined;
  busy: boolean;
  error: Error | null;
  onChange: (s: ScheduledDraft) => void;
  onPreview: () => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  const { t } = useContext(UtilityWords);
  const set = <K extends keyof ScheduledDraft>(key: K, value: ScheduledDraft[K]) =>
    onChange({ ...draft, [key]: value });
  const kind = draft.schedule.kind;
  const local = draft.next_run_at.slice(0, 16);
  const validTime = Number.isFinite(Date.parse(draft.next_run_at));
  const changeKind = (value: Recurrence["kind"]) =>
    set("schedule", newRecurrence(value, draft.next_run_at, draft.tz));
  return (
    <div className="utility-overlay" role="presentation" onClick={onCancel}>
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- Dialog containment. */}
      <section
        className="utility-dialog utility-editor"
        role="dialog"
        aria-modal="true"
        aria-label={deferredText("Schedule editor")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>{editing ? deferredText("EDIT SCHEDULE") : deferredText("SCHEDULE A MESSAGE")}</strong>
          <button onClick={onCancel} aria-label={deferredText("Close editor")}>
            ×
          </button>
        </div>
        <Field label={t("Channel")} htmlFor="schedule-channel">
          <ChannelPicker
            posting
            id="schedule-channel"
            info={info}
            value={draft.channel_id}
            onChange={(v) => set("channel_id", v ?? "")}
            none={t("Choose a channel")}
          />
        </Field>
        <Field
          label={t("Message")}
          htmlFor="schedule-content"
          aside={
            <span className="counter">{deferredText("{count}/2,000", { count: draft.content.length })}</span>
          }
        >
          <textarea
            id="schedule-content"
            className="pixel-input"
            rows={5}
            maxLength={2000}
            value={draft.content}
            onChange={(e) => set("content", e.target.value)}
          />
        </Field>
        <div className="utility-two">
          <Field label={deferredText("First run (UTC)")} htmlFor="schedule-when">
            <input
              id="schedule-when"
              className="pixel-input"
              type="datetime-local"
              value={local}
              onChange={(e) => set("next_run_at", firstRunUtc(e.target.value))}
            />
          </Field>
          <Field label={t("IANA time zone")} htmlFor="schedule-tz" hint={t(TIMEZONE_NOTICE)}>
            <input
              id="schedule-tz"
              className="pixel-input"
              value={draft.tz}
              onChange={(e) => set("tz", e.target.value)}
            />
          </Field>
        </div>
        <Field label={t("Repeat")} htmlFor="schedule-kind">
          <select
            id="schedule-kind"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(e) => changeKind(e.target.value as Recurrence["kind"])}
          >
            <option value="once">{t("Once")}</option>
            <option value="interval">{deferredText("Every interval")}</option>
            <option value="daily">{t("Daily")}</option>
            <option value="weekly">{t("Weekly")}</option>
            <option value="cron">{deferredText("Cron pattern")}</option>
          </select>
        </Field>
        {kind === "interval" && (
          <Field label={deferredText("Every seconds")} htmlFor="schedule-interval">
            <input
              id="schedule-interval"
              className="pixel-input"
              type="number"
              min={60}
              max={31536000}
              value={draft.schedule.seconds}
              onChange={(e) => set("schedule", { kind, seconds: Number(e.target.value) })}
            />
          </Field>
        )}
        {kind === "daily" && <TimeFields schedule={draft.schedule} onChange={(s) => set("schedule", s)} />}
        {kind === "weekly" && (
          <>
            <Field label={deferredText("Day of week")} htmlFor="schedule-weekday">
              <select
                id="schedule-weekday"
                className="pixel-input pixel-select"
                value={draft.schedule.weekday}
                onChange={(e) =>
                  set("schedule", {
                    kind: "weekly",
                    weekday: Number(e.target.value),
                    hour: (draft.schedule as Extract<Recurrence, { kind: "weekly" }>).hour,
                    minute: (draft.schedule as Extract<Recurrence, { kind: "weekly" }>).minute,
                  })
                }
              >
                {[
                  t("Sunday"),
                  t("Monday"),
                  t("Tuesday"),
                  t("Wednesday"),
                  t("Thursday"),
                  t("Friday"),
                  t("Saturday"),
                ].map((day, i) => (
                  <option key={day} value={i}>
                    {day}
                  </option>
                ))}
              </select>
            </Field>
            <TimeFields schedule={draft.schedule} onChange={(s) => set("schedule", s)} />
          </>
        )}
        {kind === "cron" && (
          <Field
            label={deferredText("Cron: minute hour day month weekday")}
            htmlFor="schedule-cron"
            hint={deferredText("Use * or one number in each field. Example: 0 9 * * 1")}
          >
            <input
              id="schedule-cron"
              className="pixel-input"
              value={[
                draft.schedule.minute,
                draft.schedule.hour,
                draft.schedule.day,
                draft.schedule.month,
                draft.schedule.weekday,
              ].join(" ")}
              onChange={(e) => {
                const parts = e.target.value.split(" ");
                set("schedule", {
                  kind,
                  minute: parts[0] ?? "",
                  hour: parts[1] ?? "",
                  day: parts[2] ?? "",
                  month: parts[3] ?? "",
                  weekday: parts[4] ?? "",
                });
              }}
            />
          </Field>
        )}
        <EmbedEditor embed={draft.embed} onChange={(v) => set("embed", v)} />
        {error && (
          <p className="field-error" role="alert">
            {error.message}
          </p>
        )}
        <div className="utility-editor-actions">
          <PixelButton variant="ghost" onClick={onCancel}>
            {t("Cancel")}
          </PixelButton>
          <PixelButton variant="secondary" disabled={!validTime} onClick={onPreview}>
            {t("Preview")}
          </PixelButton>
          <PixelButton
            disabled={busy || !validTime || !draft.channel_id || (!draft.content.trim() && !draft.embed)}
            onClick={onSave}
          >
            {busy ? t("Saving…") : editing ? deferredText("Save schedule") : t("Schedule")}
          </PixelButton>
        </div>
      </section>
    </div>
  );
}
