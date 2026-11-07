import "../styles/utility.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";

import { EmbedEditor } from "../components/EmbedEditor";
import { Field } from "../components/Field";
import { MessagePreview } from "../components/MessagePreview";
import { Panel } from "../components/Panel";
import { ChannelPicker, channelLabel } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { type EmbedSpec, type GuildInfo, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type Recurrence,
  type Scheduled,
  type ScheduledDraft,
  type Sticky,
  useUtility,
  FIXED_OFFSET_NOTICE,
  pauseReason,
  firstRunUtc,
  newRecurrence,
} from "../lib/utility";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

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
    <div className="settings-page utility-page">
      <PageHead
        title="MESSAGES"
        lead="Keep an important note visible, or send a message at the right time. AFK notices are set by each member with /afk set."
      />
      {module && !module.enabled && (
        <p className="notice">
          <strong>Messages are off.</strong> Turn the module on in{" "}
          <Link to={`/g/${guild}/modules`}>Modules</Link> to use sticky posts and AFK notices.
        </p>
      )}
      {query.isPending && <Loading label="Reading messages" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <>
          <div className="utility-summary">
            <div>
              <span className="pixel-label">STICKY</span>
              <strong>{query.data.sticky.length}</strong>
              <small>channel notes</small>
            </div>
            <div>
              <span className="pixel-label">SCHEDULED</span>
              <strong>
                {query.data.scheduled.length}/{query.data.max_scheduled}
              </strong>
              <small>used</small>
            </div>
            <div>
              <span className="pixel-label">AFK</span>
              <strong>5 min</strong>
              <small>mention cooldown per channel</small>
            </div>
          </div>
          <TimezoneEditor
            initial={query.data.utc_offset}
            busy={saveTimezone.isPending}
            error={saveTimezone.error}
            onSave={(value) =>
              saveTimezone.mutate(value, {
                onSuccess: () => setNotice("Server time offset saved."),
                onError: (e) => setNotice(e.message),
              })
            }
          />
          <Panel title="STICKY MESSAGES">
            <p className="panel-lead muted">
              After both the chosen number of member messages and the minimum time interval, Pinchora posts a
              fresh copy and removes the old one. Counts are batched to prevent bursts. It needs Manage
              Messages.
            </p>
            <div className="utility-list">
              {query.data.sticky.map((item) => (
                <article className="utility-item" key={item.channel_id}>
                  <div className="utility-item-main">
                    <strong>{name(discord.data, item.channel_id)}</strong>
                    <span className="badge">{item.enabled ? "ON" : "OFF"}</span>
                    <p>{item.content || "Embed only"}</p>
                    {pauseReason(item.pause_reason) && (
                      <p role="status">{item.pause_reason_text ?? pauseReason(item.pause_reason)}</p>
                    )}
                    <small>
                      Every {item.min_messages} messages and at least {item.min_gap_secs} seconds ·{" "}
                      {item.message_count} counted
                    </small>
                  </div>
                  <div className="utility-actions">
                    <PixelButton variant="secondary" onClick={() => setSticky({ ...item })}>
                      Edit
                    </PixelButton>
                    <PixelButton
                      variant="ghost"
                      onClick={() => {
                        setPreviewItem(item);
                        setRuns([]);
                      }}
                    >
                      Preview
                    </PixelButton>
                    <PixelButton
                      variant="ghost"
                      onClick={() =>
                        removeSticky.mutate(item.channel_id, {
                          onSuccess: () => setNotice("Sticky message removed."),
                          onError: (e) => setNotice(e.message),
                        })
                      }
                    >
                      Remove
                    </PixelButton>
                  </div>
                </article>
              ))}
              {query.data.sticky.length === 0 && <p className="empty-row muted">No sticky messages yet.</p>}
            </div>
            <PixelButton onClick={() => setSticky(newSticky())}>＋ New sticky</PixelButton>
          </Panel>
          <Panel
            title={`SCHEDULED MESSAGES · ${query.data.scheduled.length}/${query.data.max_scheduled} USED`}
          >
            <p className="panel-lead muted">
              Send once, every few minutes, daily, weekly, or with a five-field cron pattern. Choose a UTC
              offset for the server's local time. {query.data.fixed_offset_notice ?? FIXED_OFFSET_NOTICE}
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
                      onSuccess: () => setNotice("Scheduled message removed."),
                      onError: (e) => setNotice(e.message),
                    })
                  }
                />
              ))}
              {query.data.scheduled.length === 0 && <p className="empty-row muted">Nothing scheduled yet.</p>}
            </div>
            <PixelButton
              disabled={query.data.scheduled.length >= query.data.max_scheduled}
              onClick={() => {
                setEditingId(null);
                setScheduled(newScheduled(query.data.utc_offset));
              }}
            >
              ＋ Schedule a message
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
                setNotice("Sticky message saved.");
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
              setNotice("Scheduled message saved.");
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
            aria-label="Message preview"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="utility-dialog-head">
              <strong>PREVIEW</strong>
              <button onClick={() => setPreviewItem(null)} aria-label="Close preview">
                ×
              </button>
            </div>
            <MessagePreview
              preview={{ text: previewItem.content, embed: previewItem.embed, ephemeral: false, notes: [] }}
            />
            {runs.length > 0 && (
              <div className="utility-runs">
                <strong>NEXT RUNS</strong>
                <ol>
                  {runs.map((run) => (
                    <li key={run}>{new Date(run).toLocaleString()}</li>
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
          <button onClick={() => setNotice(null)} aria-label="Dismiss">
            ×
          </button>
        </div>
      )}
      {showError(removeSticky.error || removeScheduled.error || enableScheduled.error)}
    </div>
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
  const [value, setValue] = useState(initial);
  return (
    <Panel title="SERVER TIME">
      <div className="utility-timezone">
        <Field label="Default UTC offset" htmlFor="utility-offset" hint={FIXED_OFFSET_NOTICE}>
          <input
            id="utility-offset"
            className="pixel-input"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            maxLength={6}
          />
        </Field>
        <PixelButton
          disabled={busy || value === initial || !/^[+-](?:(?:0\d|1[0-3]):[0-5]\d|14:00)$/.test(value)}
          onClick={() => onSave(value)}
        >
          {busy ? "Saving…" : "Save offset"}
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
  return (
    <article className="utility-item">
      <div className="utility-item-main">
        <strong>
          #{item.id} · {name(info, item.channel_id)}
        </strong>
        <span className={`badge ${item.enabled ? "badge-gold" : ""}`}>
          {item.enabled ? "ACTIVE" : "PAUSED"}
        </span>
        <p>{item.content || "Embed only"}</p>
        {pauseReason(item.pause_reason) && (
          <p role="status">{item.pause_reason_text ?? pauseReason(item.pause_reason)}</p>
        )}
        <small>
          {item.schedule.kind.replace("_", " ")} · next {new Date(item.next_run_at).toLocaleString()} ·{" "}
          {item.runs} sent
        </small>
      </div>
      <div className="utility-actions">
        <PixelButton variant="secondary" onClick={onEdit}>
          Edit
        </PixelButton>
        <PixelButton variant="secondary" onClick={onPreview}>
          Preview 5 runs
        </PixelButton>
        <PixelButton variant="ghost" onClick={onToggle}>
          {item.enabled ? "Pause" : "Resume"}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onRemove}>
          Remove
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
  const set = <K extends keyof Sticky>(key: K, value: Sticky[K]) => onChange({ ...draft, [key]: value });
  return (
    <div className="utility-overlay" role="presentation" onClick={onCancel}>
      <section
        className="utility-dialog utility-editor"
        role="dialog"
        aria-modal="true"
        aria-label="Sticky message editor"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>STICKY MESSAGE</strong>
          <button onClick={onCancel} aria-label="Close editor">
            ×
          </button>
        </div>
        <Field label="Channel" htmlFor="sticky-channel">
          <ChannelPicker
            id="sticky-channel"
            info={info}
            value={draft.channel_id}
            onChange={(v) => set("channel_id", v ?? "")}
            none="Choose a channel"
          />
        </Field>
        <Field
          label="Message"
          htmlFor="sticky-content"
          aside={<span className="counter">{draft.content.length}/1000</span>}
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
          <Field label="After messages" htmlFor="sticky-messages">
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
          <Field label="Time interval (seconds)" htmlFor="sticky-gap">
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
          <span>Enabled</span>
          <PixelToggle label="Enabled" checked={draft.enabled} onChange={(v) => set("enabled", v)} />
        </div>
        <EmbedEditor embed={draft.embed} onChange={(v) => set("embed", v)} />
        {error && (
          <p className="field-error" role="alert">
            {error.message}
          </p>
        )}
        <div className="utility-editor-actions">
          <PixelButton variant="ghost" onClick={onCancel}>
            Cancel
          </PixelButton>
          <PixelButton variant="secondary" onClick={onPreview}>
            Preview
          </PixelButton>
          <PixelButton
            disabled={busy || !draft.channel_id || (!draft.content.trim() && !draft.embed)}
            onClick={onSave}
          >
            {busy ? "Saving…" : "Save sticky"}
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
        aria-label="Schedule editor"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="utility-dialog-head">
          <strong>{editing ? "EDIT SCHEDULE" : "SCHEDULE A MESSAGE"}</strong>
          <button onClick={onCancel} aria-label="Close editor">
            ×
          </button>
        </div>
        <Field label="Channel" htmlFor="schedule-channel">
          <ChannelPicker
            id="schedule-channel"
            info={info}
            value={draft.channel_id}
            onChange={(v) => set("channel_id", v ?? "")}
            none="Choose a channel"
          />
        </Field>
        <Field
          label="Message"
          htmlFor="schedule-content"
          aside={<span className="counter">{draft.content.length}/2000</span>}
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
          <Field label="First run (UTC)" htmlFor="schedule-when">
            <input
              id="schedule-when"
              className="pixel-input"
              type="datetime-local"
              value={local}
              onChange={(e) => set("next_run_at", firstRunUtc(e.target.value))}
            />
          </Field>
          <Field label="UTC offset" htmlFor="schedule-tz" hint={FIXED_OFFSET_NOTICE}>
            <input
              id="schedule-tz"
              className="pixel-input"
              value={draft.tz}
              onChange={(e) => set("tz", e.target.value)}
            />
          </Field>
        </div>
        <Field label="Repeat" htmlFor="schedule-kind">
          <select
            id="schedule-kind"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(e) => changeKind(e.target.value as Recurrence["kind"])}
          >
            <option value="once">Once</option>
            <option value="interval">Every interval</option>
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
            <option value="cron">Cron pattern</option>
          </select>
        </Field>
        {kind === "interval" && (
          <Field label="Every seconds" htmlFor="schedule-interval">
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
            <Field label="Day of week" htmlFor="schedule-weekday">
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
                {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].map(
                  (day, i) => (
                    <option key={day} value={i}>
                      {day}
                    </option>
                  ),
                )}
              </select>
            </Field>
            <TimeFields schedule={draft.schedule} onChange={(s) => set("schedule", s)} />
          </>
        )}
        {kind === "cron" && (
          <Field
            label="Cron: minute hour day month weekday"
            htmlFor="schedule-cron"
            hint="Use * or one number in each field. Example: 0 9 * * 1"
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
            Cancel
          </PixelButton>
          <PixelButton variant="secondary" disabled={!validTime} onClick={onPreview}>
            Preview
          </PixelButton>
          <PixelButton
            disabled={busy || !validTime || !draft.channel_id || (!draft.content.trim() && !draft.embed)}
            onClick={onSave}
          >
            {busy ? "Saving…" : editing ? "Save schedule" : "Schedule"}
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
  return (
    <div className="utility-two">
      <Field label="Hour" htmlFor="schedule-hour">
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
      <Field label="Minute" htmlFor="schedule-minute">
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
