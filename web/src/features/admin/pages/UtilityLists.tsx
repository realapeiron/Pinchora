import { useState, type Dispatch, type SetStateAction } from "react";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { type EmbedSpec, type GuildInfo } from "../../../lib/api";
import { channelLabel } from "../../../components/Pickers";
import { catalogWords } from "../api/catalogWords";
import {
  type ScheduledDraft,
  type Sticky,
  type UtilityList,
  useUtility,
  TIMEZONE_NOTICE,
  pauseReason,
} from "../api/utility";
import { ScheduleCard } from "./UtilityCards";
import { utilitySummary } from "./utilitySummary";
import { UtilityInlinePreview } from "./UtilityInlinePreview";

function name(info: GuildInfo | undefined, id: string) {
  const channel = info?.channels.find((item) => item.id === id);
  return channel ? channelLabel(channel) : `#${id}`;
}
const newSticky = (): Sticky => ({
  channel_id: "",
  content: "",
  embed: null,
  min_gap_secs: 30,
  min_messages: 5,
  message_count: 0,
  enabled: true,
});
const newScheduled = (tz: string): ScheduledDraft => ({
  channel_id: "",
  content: "",
  embed: null,
  schedule: { kind: "once" },
  tz,
  next_run_at: new Date(Date.now() + 3_600_000).toISOString(),
});

export function UtilityLists({
  data,
  info,
  actions,
  setSticky,
  setScheduled,
  setEditingId,
  setNotice,
  t,
}: {
  data: UtilityList;
  info: GuildInfo | undefined;
  actions: ReturnType<typeof useUtility>;
  setSticky: Dispatch<SetStateAction<Sticky | null>>;
  setScheduled: Dispatch<SetStateAction<ScheduledDraft | null>>;
  setEditingId: Dispatch<SetStateAction<string | null>>;
  setNotice: Dispatch<SetStateAction<string | null>>;
  t: ReturnType<typeof catalogWords>;
}) {
  const { removeSticky, preview, enableScheduled, removeScheduled } = actions;
  const summary = utilitySummary(data);
  const [previewItem, setPreviewItem] = useState<{ content: string; embed: EmbedSpec | null } | null>(null);
  const [previewKind, setPreviewKind] = useState<"sticky" | "scheduled" | null>(null);
  const [runs, setRuns] = useState<string[]>([]);
  return (
    <div className="settings-detail-list">
      <SettingsRow
        rowId="utility-lists-sticky-messages"
        title={dashboardT("Sticky messages", {}, "STICKY MESSAGES")}
        description={dashboardT("Manage notes that stay visible in channels.")}
        summary={summary.sticky}
      >
        <div className="settings-detail-fields">
          <p className="panel-lead muted">
            {t(
              "After both the chosen number of member messages and the minimum time interval, Pinchora posts a fresh copy and removes the old one. Counts are batched to prevent bursts. It needs Manage Messages.",
            )}
          </p>
          <div className="utility-list">
            {data.sticky.map((item) => (
              <article className="utility-item" key={item.channel_id}>
                <div className="utility-item-main">
                  <strong>{name(info, item.channel_id)}</strong>
                  <span className="badge">{item.enabled ? t("ON") : t("OFF")}</span>
                  <p>{item.content || t("Embed only")}</p>
                  {pauseReason(item.pause_reason) && (
                    <p role="status">{item.pause_reason_text ?? t(pauseReason(item.pause_reason) ?? "")}</p>
                  )}
                  <small>
                    {t("Every {messages} messages and at least {seconds} seconds · {count} counted", {
                      messages: item.min_messages,
                      seconds: item.min_gap_secs,
                      count: item.message_count,
                    })}
                  </small>
                </div>
                <div className="utility-actions">
                  <PixelButton variant="secondary" onClick={() => setSticky({ ...item })}>
                    {t("Edit")}
                  </PixelButton>
                  <PixelButton
                    variant="ghost"
                    onClick={() => {
                      setPreviewItem(item);
                      setPreviewKind("sticky");
                      setRuns([]);
                    }}
                  >
                    {t("Preview")}
                  </PixelButton>
                  <PixelButton
                    variant="ghost"
                    onClick={() =>
                      removeSticky.mutate(item.channel_id, {
                        onSuccess: () => setNotice(t("Sticky message removed.")),
                        onError: (e) => setNotice(e.message),
                      })
                    }
                  >
                    {t("Remove")}
                  </PixelButton>
                </div>
              </article>
            ))}
            {data.sticky.length === 0 && <p className="empty-row muted">{t("No sticky messages yet.")}</p>}
          </div>
          <PixelButton onClick={() => setSticky(newSticky())}>{t("＋ New sticky")}</PixelButton>
        </div>
        {previewKind === "sticky" && previewItem && (
          <UtilityInlinePreview item={previewItem} runs={runs} t={t} onClose={() => setPreviewItem(null)} />
        )}
      </SettingsRow>
      <SettingsRow
        rowId="utility-lists-scheduled-messages"
        title={dashboardT(
          "Scheduled messages · {count}/{max} used",
          {
            count: data.scheduled.length,
            max: data.max_scheduled,
          },
          "SCHEDULED MESSAGES · {count}/{max} USED",
        )}
        description={dashboardT("Manage messages sent at chosen times.")}
        summary={summary.scheduled}
      >
        <div className="settings-detail-fields">
          <p className="panel-lead muted">
            {t(
              "Send once, every few minutes, daily, weekly, or with a five-field cron pattern. Choose an IANA time zone for the server's local time.",
            )}{" "}
            {data.timezone_notice ?? t(TIMEZONE_NOTICE)}
          </p>
          <div className="utility-list">
            {data.scheduled.map((item) => (
              <ScheduleCard
                key={item.id}
                item={item}
                info={info}
                onPreview={() =>
                  preview.mutate(item.id, {
                    onSuccess: (result) => {
                      setPreviewItem(item);
                      setPreviewKind("scheduled");
                      setRuns(result.runs);
                    },
                    onError: (e) => setNotice(e.message),
                  })
                }
                onEdit={() => {
                  setEditingId(item.id);
                  setScheduled({
                    channel_id: item.channel_id,
                    content: item.content,
                    embed: item.embed,
                    schedule: item.schedule,
                    tz: item.tz,
                    next_run_at: item.next_run_at,
                  });
                }}
                onToggle={() =>
                  enableScheduled.mutate(
                    { id: item.id, enabled: !item.enabled },
                    { onError: (e) => setNotice(e.message) },
                  )
                }
                onRemove={() =>
                  removeScheduled.mutate(item.id, {
                    onSuccess: () => setNotice(t("Scheduled message removed.")),
                    onError: (e) => setNotice(e.message),
                  })
                }
              />
            ))}
            {data.scheduled.length === 0 && <p className="empty-row muted">{t("Nothing scheduled yet.")}</p>}
          </div>
          <PixelButton
            disabled={data.scheduled.length >= data.max_scheduled}
            onClick={() => {
              setEditingId(null);
              setScheduled(newScheduled(data.timezone));
            }}
          >
            {t("＋ Schedule a message")}
          </PixelButton>
        </div>
        {previewKind === "scheduled" && previewItem && (
          <UtilityInlinePreview item={previewItem} runs={runs} t={t} onClose={() => setPreviewItem(null)} />
        )}
      </SettingsRow>
    </div>
  );
}
