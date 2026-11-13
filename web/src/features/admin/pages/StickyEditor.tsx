import { t as deferredText } from "../../../i18n/dashboard";
import { useContext } from "react";
import { EmbedEditor } from "../components/EmbedEditor";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import type { GuildInfo } from "../../../lib/api";
import type { Sticky } from "../api/utility";
import { UtilityWords } from "./utilityWords";

export function StickyEditor({
  draft,
  info,
  busy,
  error,
  onChange,
  onPreview,
  onCancel,
  onSave,
}: {
  draft: Sticky;
  info: GuildInfo | undefined;
  busy: boolean;
  error: Error | null;
  onChange: (s: Sticky) => void;
  onPreview: () => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  const { t } = useContext(UtilityWords);
  const set = <K extends keyof Sticky>(key: K, value: Sticky[K]) => onChange({ ...draft, [key]: value });
  return (
    <div className="utility-overlay" role="presentation" onClick={onCancel}>
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- Dialog containment. */}
      <section
        className="utility-dialog utility-editor"
        role="dialog"
        aria-modal="true"
        aria-label={deferredText("Sticky message editor")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>{t("STICKY MESSAGE")}</strong>
          <button onClick={onCancel} aria-label={deferredText("Close editor")}>
            ×
          </button>
        </div>
        <Field label={t("Channel")} htmlFor="sticky-channel">
          <ChannelPicker
            posting
            id="sticky-channel"
            info={info}
            value={draft.channel_id}
            onChange={(v) => set("channel_id", v ?? "")}
            none={t("Choose a channel")}
          />
        </Field>
        <Field
          label={t("Message")}
          htmlFor="sticky-content"
          aside={
            <span className="counter">{deferredText("{count}/1,000", { count: draft.content.length })}</span>
          }
        >
          <textarea
            id="sticky-content"
            className="pixel-input"
            rows={5}
            maxLength={1000}
            value={draft.content}
            onChange={(e) => set("content", e.target.value)}
          />
        </Field>
        <div className="utility-two">
          <Field label={deferredText("After messages")} htmlFor="sticky-messages">
            <input
              id="sticky-messages"
              className="pixel-input"
              type="number"
              min={1}
              max={1000}
              value={draft.min_messages}
              onChange={(e) => set("min_messages", Number(e.target.value))}
            />
          </Field>
          <Field label={deferredText("Time interval (seconds)")} htmlFor="sticky-gap">
            <input
              id="sticky-gap"
              className="pixel-input"
              type="number"
              min={5}
              max={86400}
              value={draft.min_gap_secs}
              onChange={(e) => set("min_gap_secs", Number(e.target.value))}
            />
          </Field>
        </div>
        <div className="utility-toggle">
          <span>{t("Enabled")}</span>
          <PixelToggle label={t("Enabled")} checked={draft.enabled} onChange={(v) => set("enabled", v)} />
        </div>
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
          <PixelButton variant="secondary" onClick={onPreview}>
            {t("Preview")}
          </PixelButton>
          <PixelButton
            disabled={busy || !draft.channel_id || (!draft.content.trim() && !draft.embed)}
            onClick={onSave}
          >
            {busy ? t("Saving…") : deferredText("Save sticky")}
          </PixelButton>
        </div>
      </section>
    </div>
  );
}
