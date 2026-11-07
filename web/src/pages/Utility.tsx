import "../styles/utility.css";

import { createContext, useContext, useState } from "react";
import { dateTime } from "../i18n";
import { Link, useOutletContext } from "react-router";

import { EmbedEditor } from "../components/EmbedEditor";
import { Field } from "../components/Field";
import { MessagePreview } from "../components/MessagePreview";
import { Panel } from "../components/Panel";
import { ChannelPicker, channelLabel } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { type EmbedSpec, type GuildInfo, type Overview } from "../lib/api";
import { catalogWords } from "../lib/catalogWords";
import { useDiscord } from "../lib/queries";
import {
  type Recurrence,
  type Scheduled,
  type ScheduledDraft,
  type Sticky,
  useUtility,
  TIMEZONE_NOTICE,
  validTimezone,
  pauseReason,
  firstRunUtc,
  newRecurrence,
} from "../lib/utility";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

const UtilityWords = createContext({ t: catalogWords(), language: "en" });

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

export function Utility() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const {
    query,
    saveSticky,
    removeSticky,
    addScheduled,
    saveScheduled,
    removeScheduled,
    enableScheduled,
    preview,
    previewDraft,
    saveTimezone,
  } = useUtility(guild);
  const language = query.data?.language ?? "en";
  const t = catalogWords(query.data?.strings, language);
  const discord = useDiscord(guild);
  const [sticky, setSticky] = useState<Sticky | null>(null);
  const [scheduled, setScheduled] = useState<ScheduledDraft | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [previewItem, setPreviewItem] = useState<{ content: string; embed: EmbedSpec | null } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [runs, setRuns] = useState<string[]>([]);
  const module = data.modules.find((m) => m.key === "utility");
  const showError = (error: Error | null) =>
    error && (
      <p className="field-error" role="alert">
        {error.message}
      </p>
    );
  return (
    <UtilityWords.Provider value={{ t, language }}>
      <div className="settings-page utility-page">
        <PageHead
          title={t("MESSAGES")}
          lead={t(
            "Keep an important note visible, or send a message at the right time. AFK notices are set by each member with /afk set.",
          )}
        />
        {module && !module.enabled && (
          <p className="notice">
            <strong>{t("Messages are off.")}</strong>{" "}
            {t("Enable Messages to use sticky posts and AFK notices.")}{" "}
            <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>
          </p>
        )}
        {query.isPending && <Loading label={t("Reading messages")} />}
        {query.isError && <Problem error={query.error} />}
        {query.isSuccess && (
          <>
            <div className="utility-summary">
              <div>
                <span className="pixel-label">{t("STICKY")}</span>
                <strong>{query.data.sticky.length}</strong>
                <small>{t("channel notes")}</small>
              </div>
              <div>
                <span className="pixel-label">{t("SCHEDULED")}</span>
                <strong>
                  {query.data.scheduled.length}/{query.data.max_scheduled}
                </strong>
                <small>{t("used")}</small>
              </div>
              <div>
                <span className="pixel-label">{t("AFK")}</span>
                <strong>{t("5 min")}</strong>
                <small>{t("mention cooldown per channel")}</small>
              </div>
            </div>
            <TimezoneEditor
              initial={query.data.timezone}
              busy={saveTimezone.isPending}
              error={saveTimezone.error}
              onSave={(value) =>
                saveTimezone.mutate(value, {
                  onSuccess: () => setNotice(t("Server time zone saved.")),
                  onError: (e) => setNotice(e.message),
                })
              }
            />
            <Panel title={t("STICKY MESSAGES")}>
              <p className="panel-lead muted">
                {t(
                  "After both the chosen number of member messages and the minimum time interval, Pinchora posts a fresh copy and removes the old one. Counts are batched to prevent bursts. It needs Manage Messages.",
                )}
              </p>
              <div className="utility-list">
                {query.data.sticky.map((item) => (
                  <article className="utility-item" key={item.channel_id}>
                    <div className="utility-item-main">
                      <strong>{name(discord.data, item.channel_id)}</strong>
                      <span className="badge">{item.enabled ? t("ON") : t("OFF")}</span>
                      <p>{item.content || t("Embed only")}</p>
                      {pauseReason(item.pause_reason) && (
                        <p role="status">
                          {item.pause_reason_text ?? t(pauseReason(item.pause_reason) ?? "")}
                        </p>
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
                {query.data.sticky.length === 0 && (
                  <p className="empty-row muted">{t("No sticky messages yet.")}</p>
                )}
              </div>
              <PixelButton onClick={() => setSticky(newSticky())}>{t("＋ New sticky")}</PixelButton>
            </Panel>
            <Panel
              title={t("SCHEDULED MESSAGES · {count}/{max} USED", {
                count: query.data.scheduled.length,
                max: query.data.max_scheduled,
              })}
            >
              <p className="panel-lead muted">
                {t(
                  "Send once, every few minutes, daily, weekly, or with a five-field cron pattern. Choose an IANA time zone for the server's local time.",
                )}{" "}
                {query.data.timezone_notice ?? t(TIMEZONE_NOTICE)}
              </p>
              <div className="utility-list">
                {query.data.scheduled.map((item) => (
                  <ScheduleCard
                    key={item.id}
                    item={item}
                    info={discord.data}
                    onPreview={() =>
                      preview.mutate(item.id, {
                        onSuccess: (result) => {
                          setPreviewItem(item);
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
                {query.data.scheduled.length === 0 && (
                  <p className="empty-row muted">{t("Nothing scheduled yet.")}</p>
                )}
              </div>
              <PixelButton
                disabled={query.data.scheduled.length >= query.data.max_scheduled}
                onClick={() => {
                  setEditingId(null);
                  setScheduled(newScheduled(query.data.timezone));
                }}
              >
                {t("＋ Schedule a message")}
              </PixelButton>
            </Panel>
          </>
        )}
        {sticky && (
          <StickyEditor
            draft={sticky}
            info={discord.data}
            busy={saveSticky.isPending}
            error={saveSticky.error}
            onChange={setSticky}
            onPreview={() => {
              setPreviewItem(sticky);
              setRuns([]);
            }}
            onCancel={() => setSticky(null)}
            onSave={() =>
              saveSticky.mutate(sticky, {
                onSuccess: () => {
                  setSticky(null);
                  setNotice(t("Sticky message saved."));
                },
              })
            }
          />
        )}
        {scheduled && (
          <ScheduleEditor
            draft={scheduled}
            editing={editingId !== null}
            info={discord.data}
            busy={editingId === null ? addScheduled.isPending : saveScheduled.isPending}
            error={editingId === null ? addScheduled.error : saveScheduled.error}
            onChange={setScheduled}
            onPreview={() => {
              previewDraft.mutate(scheduled, {
                onSuccess: (result) => {
                  setPreviewItem(scheduled);
                  setRuns(result.runs);
                },
                onError: (e) => setNotice(e.message),
              });
            }}
            onCancel={() => {
              setScheduled(null);
              setEditingId(null);
            }}
            onSave={() => {
              const onSuccess = () => {
                setScheduled(null);
                setEditingId(null);
                setNotice(t("Scheduled message saved."));
              };
              if (editingId === null) addScheduled.mutate(scheduled, { onSuccess });
              else saveScheduled.mutate({ id: editingId, draft: scheduled }, { onSuccess });
            }}
          />
        )}
        {previewItem && (
          <div className="utility-overlay" role="presentation" onClick={() => setPreviewItem(null)}>
            <section
              className="utility-dialog"
              role="dialog"
              aria-modal="true"
              aria-label={t("Message preview")}
              onClick={(e) => e.stopPropagation()}
            >
              <div className="utility-dialog-head">
                <strong>{t("PREVIEW")}</strong>
                <button onClick={() => setPreviewItem(null)} aria-label={t("Close preview")}>
                  ×
                </button>
              </div>
              <MessagePreview
                preview={{ text: previewItem.content, embed: previewItem.embed, ephemeral: false, notes: [] }}
              />
              {runs.length > 0 && (
                <div className="utility-runs">
                  <strong>{t("NEXT RUNS")}</strong>
                  <ol>
                    {runs.map((run) => (
                      <li key={run}>{dateTime(run)}</li>
                    ))}
                  </ol>
                </div>
              )}
            </section>
          </div>
        )}
        {notice && (
          <div className="notice" role="status">
            {notice}
            <button onClick={() => setNotice(null)} aria-label={t("Dismiss")}>
              ×
            </button>
          </div>
        )}
        {showError(removeSticky.error || removeScheduled.error || enableScheduled.error)}
      </div>
    </UtilityWords.Provider>
  );
}

function TimezoneEditor({
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
    <Panel title={t("SERVER TIME")}>
      <div className="utility-timezone">
        <Field label={t("Default IANA time zone")} htmlFor="utility-offset" hint={t(TIMEZONE_NOTICE)}>
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
          {busy ? t("Saving…") : t("Save time zone")}
        </PixelButton>
      </div>
      {error && (
        <p className="field-error" role="alert">
          {error.message}
        </p>
      )}
    </Panel>
  );
}

function ScheduleCard({
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
          {item.enabled ? t("ACTIVE") : t("PAUSED")}
        </span>
        <p>{item.content || t("Embed only")}</p>
        {pauseReason(item.pause_reason) && (
          <p role="status">{item.pause_reason_text ?? t(pauseReason(item.pause_reason) ?? "")}</p>
        )}
        <small>
          {t("{kind} · next {when} · {count} sent", {
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
          {t("Preview 5 runs")}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onToggle}>
          {item.enabled ? t("Pause") : t("Resume")}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onRemove}>
          {t("Remove")}
        </PixelButton>
      </div>
    </article>
  );
}

function StickyEditor({
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
      <section
        className="utility-dialog utility-editor"
        role="dialog"
        aria-modal="true"
        aria-label={t("Sticky message editor")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>{t("STICKY MESSAGE")}</strong>
          <button onClick={onCancel} aria-label={t("Close editor")}>
            ×
          </button>
        </div>
        <Field label={t("Channel")} htmlFor="sticky-channel">
          <ChannelPicker
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
          aside={<span className="counter">{t("{count}/1,000", { count: draft.content.length })}</span>}
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
          <Field label={t("After messages")} htmlFor="sticky-messages">
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
          <Field label={t("Time interval (seconds)")} htmlFor="sticky-gap">
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
            {busy ? t("Saving…") : t("Save sticky")}
          </PixelButton>
        </div>
      </section>
    </div>
  );
}

function ScheduleEditor({
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
      <section
        className="utility-dialog utility-editor"
        role="dialog"
        aria-modal="true"
        aria-label={t("Schedule editor")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>{editing ? t("EDIT SCHEDULE") : t("SCHEDULE A MESSAGE")}</strong>
          <button onClick={onCancel} aria-label={t("Close editor")}>
            ×
          </button>
        </div>
        <Field label={t("Channel")} htmlFor="schedule-channel">
          <ChannelPicker
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
          aside={<span className="counter">{t("{count}/2,000", { count: draft.content.length })}</span>}
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
          <Field label={t("First run (UTC)")} htmlFor="schedule-when">
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
            <option value="interval">{t("Every interval")}</option>
            <option value="daily">{t("Daily")}</option>
            <option value="weekly">{t("Weekly")}</option>
            <option value="cron">{t("Cron pattern")}</option>
          </select>
        </Field>
        {kind === "interval" && (
          <Field label={t("Every seconds")} htmlFor="schedule-interval">
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
            <Field label={t("Day of week")} htmlFor="schedule-weekday">
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
            label={t("Cron: minute hour day month weekday")}
            htmlFor="schedule-cron"
            hint={t("Use * or one number in each field. Example: 0 9 * * 1")}
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
            {busy ? t("Saving…") : editing ? t("Save schedule") : t("Schedule")}
          </PixelButton>
        </div>
      </section>
    </div>
  );
}

function TimeFields({
  schedule,
  onChange,
}: {
  schedule: Extract<Recurrence, { kind: "daily" | "weekly" }>;
  onChange: (s: Recurrence) => void;
}) {
  const { t } = useContext(UtilityWords);
  return (
    <div className="utility-two">
      <Field label={t("Hour")} htmlFor="schedule-hour">
        <input
          id="schedule-hour"
          className="pixel-input"
          type="number"
          min={0}
          max={23}
          value={schedule.hour}
          onChange={(e) => onChange({ ...schedule, hour: Number(e.target.value) })}
        />
      </Field>
      <Field label={t("Minute")} htmlFor="schedule-minute">
        <input
          id="schedule-minute"
          className="pixel-input"
          type="number"
          min={0}
          max={59}
          value={schedule.minute}
          onChange={(e) => onChange({ ...schedule, minute: Number(e.target.value) })}
        />
      </Field>
    </div>
  );
}
