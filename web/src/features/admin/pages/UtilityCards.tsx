import { t as deferredText } from "../../../i18n/dashboard";
import { useContext, useState } from "react";
import { dateTime } from "../../../i18n";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { TIMEZONE_NOTICE, validTimezone, pauseReason, type Scheduled } from "../api/utility";
import { UtilityWords } from "./utilityWords";
import { channelLabel } from "../../../components/Pickers";

function name(info: GuildInfo | undefined, id: string) {
  const channel = info?.channels.find((item) => item.id === id);
  return channel ? channelLabel(channel) : `#${id}`;
}

export function TimezoneEditor({
  initial,
  busy,
  error,
  onSave,
}: {
  initial: string;
  busy: boolean;
  error: Error | null;
  onSave: (value: string) => void;
}) {
  const { t } = useContext(UtilityWords);
  const [value, setValue] = useState(initial);
  return (
    <div className="settings-detail-fields">
      <div className="utility-timezone">
        <Field
          label={deferredText("Default IANA time zone")}
          htmlFor="utility-offset"
          hint={t(TIMEZONE_NOTICE)}
        >
          <input
            id="utility-offset"
            className="pixel-input"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            maxLength={64}
          />
        </Field>
        <PixelButton
          disabled={busy || value === initial || !validTimezone(value)}
          onClick={() => onSave(value)}
        >
          {busy ? t("Saving…") : deferredText("Save time zone")}
        </PixelButton>
      </div>
      {error && (
        <p className="field-error" role="alert">
          {error.message}
        </p>
      )}
    </div>
  );
}
export function ScheduleCard({
  item,
  info,
  onEdit,
  onPreview,
  onToggle,
  onRemove,
}: {
  item: Scheduled;
  info: GuildInfo | undefined;
  onEdit: () => void;
  onPreview: () => void;
  onToggle: () => void;
  onRemove: () => void;
}) {
  const { t } = useContext(UtilityWords);
  return (
    <article className="utility-item">
      <div className="utility-item-main">
        <strong>
          #{item.id} · {name(info, item.channel_id)}
        </strong>
        <span className={`badge ${item.enabled ? "badge-gold" : ""}`}>
          {item.enabled ? deferredText("ACTIVE") : t("PAUSED")}
        </span>
        <p>{item.content || deferredText("Embed only")}</p>
        {pauseReason(item.pause_reason) && (
          <p role="status">{item.pause_reason_text ?? t(pauseReason(item.pause_reason) ?? "")}</p>
        )}
        <small>
          {deferredText("{kind} · next {when} · {count} sent", {
            kind: t(item.schedule.kind),
            when: dateTime(item.next_run_at),
            count: item.runs,
          })}
        </small>
      </div>
      <div className="utility-actions">
        <PixelButton variant="secondary" onClick={onEdit}>
          {t("Edit")}
        </PixelButton>
        <PixelButton variant="secondary" onClick={onPreview}>
          {deferredText("Preview 5 runs")}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onToggle}>
          {item.enabled ? t("Pause") : deferredText("Resume")}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onRemove}>
          {t("Remove")}
        </PixelButton>
      </div>
    </article>
  );
}
