import "../styles/events.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { type Overview } from "../lib/api";
import { catalogWords } from "../lib/catalogWords";
import { useDiscord } from "../lib/queries";
import {
  type Attendees,
  type Event,
  type EventBody,
  fromZonedInput,
  useEvents,
  zonedInput,
} from "../lib/events";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

const fresh = (tz: string): EventBody => {
  const start = new Date(Date.now() + 86400000);
  start.setMinutes(0, 0, 0);
  return {
    title: "",
    description: "",
    starts_at: start.toISOString(),
    ends_at: new Date(start.getTime() + 3600000).toISOString(),
    tz,
    location: "",
    channel_id: null,
    required_role_id: null,
    options: [
      { key: "going", label: "Going", capacity: 0 },
      { key: "maybe", label: "Maybe", capacity: 0 },
      { key: "declined", label: "Declined", capacity: 0 },
    ],
    recurrence: null,
    sync: false,
  };
};
const fromEvent = (event: Event): EventBody => ({
  title: event.title,
  description: event.description,
  starts_at: event.starts_at,
  ends_at: event.ends_at,
  tz: event.tz,
  location: event.location,
  channel_id: event.channel_id,
  required_role_id: event.required_role_id,
  options: event.options,
  recurrence: event.recurrence,
  sync: !!event.discord_event_id,
});
const day = (iso: string, tz: string, language: string) =>
  new Intl.DateTimeFormat(language, {
    timeZone: tz,
    weekday: "short",
    day: "numeric",
    month: "short",
  }).format(new Date(iso));
const hour = (iso: string, tz: string, language: string) =>
  new Intl.DateTimeFormat(language, {
    timeZone: tz,
    hour: "numeric",
    minute: "2-digit",
    timeZoneName: "short",
  }).format(new Date(iso));

export function Events() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const events = useEvents(guild);
  const language = events.query.data?.language ?? "en";
  const t = catalogWords(events.query.data?.strings, language);
  const discord = useDiscord(guild);
  const [editing, setEditing] = useState<{ id?: string; body: EventBody } | null>(null);
  const [looking, setLooking] = useState<{ id: string; data: Attendees } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [tzEdit, setTzEdit] = useState<string | null>(null);
  const [preview, setPreview] = useState(false);
  const module = data.modules.find((m) => m.key === "events");
  const upcoming =
    events.query.data?.events
      .filter((e) => e.status === "upcoming" && new Date(e.ends_at).getTime() > Date.now())
      .sort((a, b) => a.starts_at.localeCompare(b.starts_at)) ?? [];
  const past = events.query.data?.events.filter((e) => !upcoming.includes(e)).slice(0, 10) ?? [];
  const zone = events.query.data?.timezone ?? "UTC";
  async function save() {
    if (!editing) return;
    try {
      const saved = editing.id
        ? await events.save.mutateAsync({ id: editing.id, body: editing.body })
        : await events.create.mutateAsync(editing.body);
      setNotice(saved.warning ?? (editing.id ? t("Event saved.") : t("Event created and posted.")));
      setEditing(null);
      setPreview(false);
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not save event."));
    }
  }
  async function showAttendees(id: string) {
    try {
      setLooking({ id, data: await events.attendees(id) });
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not read attendees."));
    }
  }
  function changeTime(field: "starts_at" | "ends_at", value: string) {
    if (!editing || !value) return;
    try {
      setEditing({ ...editing, body: { ...editing.body, [field]: fromZonedInput(value, editing.body.tz) } });
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Choose a valid local time."));
    }
  }
  return (
    <div className="settings-page events-page">
      <PageHead
        title={t("EVENTS")}
        lead={t("Plan a gathering, post one RSVP card, and keep the guest list in sync.")}
        action={
          <PixelButton onClick={() => setEditing({ body: fresh(zone) })}>{t("Create event")}</PixelButton>
        }
      />
      {module && !module.enabled && (
        <p className="notice">
          <strong>{t("Events are off.")}</strong> {t("Enable Events so members can RSVP.")}{" "}
          <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>
        </p>
      )}
      {notice && (
        <p className="notice" role="status">
          {notice}{" "}
          <button type="button" onClick={() => setNotice(null)} aria-label={t("Dismiss notice")}>
            ×
          </button>
        </p>
      )}
      {events.query.isPending && <Loading label={t("Reading events")} />}
      {events.query.isError && <Problem error={events.query.error} />}
      {events.query.isSuccess && (
        <>
          <Panel title={t("SERVER CLOCK")} aside={<span className="ev-zone">{zone}</span>}>
            <div className="ev-clock">
              <span>{hour(new Date().toISOString(), zone, language)}</span>
              <p className="muted">
                {t(
                  "Every event form uses this time zone. Discord still shows each member the correct local time.",
                )}
              </p>
            </div>
            {tzEdit === null ? (
              <PixelButton variant="secondary" onClick={() => setTzEdit(zone)}>
                {t("Change time zone")}
              </PixelButton>
            ) : (
              <div className="ev-zone-edit">
                <input
                  className="pixel-input"
                  aria-label={t("IANA time zone")}
                  value={tzEdit}
                  onChange={(e) => setTzEdit(e.target.value)}
                  placeholder={t("Europe/Istanbul")}
                />
                <PixelButton
                  onClick={async () => {
                    try {
                      await events.timezone.mutateAsync(tzEdit);
                      setTzEdit(null);
                      setNotice(t("Server time zone saved."));
                    } catch {
                      setNotice(t("Use an IANA time zone such as Europe/Istanbul."));
                    }
                  }}
                >
                  {t("Save time zone")}
                </PixelButton>
                <PixelButton variant="ghost" onClick={() => setTzEdit(null)}>
                  {t("Cancel")}
                </PixelButton>
              </div>
            )}
          </Panel>
          <Panel
            title={t("UPCOMING · {count}", { count: upcoming.length })}
            aside={<span className="ev-small">{t("Next 100 events")}</span>}
          >
            {upcoming.length === 0 ? (
              <div className="ev-empty">
                <strong>{t("No events on the calendar yet.")}</strong>
                <p>{t("Choose Create event to post the first RSVP card.")}</p>
              </div>
            ) : (
              <div className="ev-list">
                {upcoming.map((event) => (
                  <article className="ev-item" key={event.id}>
                    <div className="ev-date">
                      <strong>{day(event.starts_at, event.tz, language)}</strong>
                      <span>{hour(event.starts_at, event.tz, language)}</span>
                    </div>
                    <div className="ev-detail">
                      <h3>{event.title}</h3>
                      <p>{event.description || t("No description")}</p>
                      <div className="ev-meta">
                        <span>#{event.id}</span>
                        <span>
                          {event.options[0]?.capacity
                            ? t("{count} going places", { count: event.options[0].capacity })
                            : t("Open capacity")}
                        </span>
                        {event.recurrence && (
                          <span>
                            {t("{repeat} until {date}", {
                              repeat: t(event.recurrence.repeat),
                              date: event.recurrence.until,
                            })}
                          </span>
                        )}
                        {event.discord_event_id && <span>{t("Discord synced")}</span>}
                      </div>
                    </div>
                    <div className="ev-actions">
                      <PixelButton variant="secondary" onClick={() => showAttendees(event.id)}>
                        {t("Attendees")}
                      </PixelButton>
                      <PixelButton
                        variant="secondary"
                        onClick={() => setEditing({ id: event.id, body: fromEvent(event) })}
                      >
                        {t("Edit")}
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        onClick={() => {
                          const start = Date.now() + 86400000;
                          const duration = Date.parse(event.ends_at) - Date.parse(event.starts_at);
                          setEditing({
                            body: {
                              ...fromEvent(event),
                              starts_at: new Date(start).toISOString(),
                              ends_at: new Date(start + duration).toISOString(),
                              sync: false,
                            },
                          });
                        }}
                      >
                        {t("Duplicate")}
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        onClick={async () => {
                          if (!window.confirm(t("Cancel {title}?", { title: event.title }))) return;
                          try {
                            const saved = await events.cancel.mutateAsync(event.id);
                            setNotice(saved.warning ?? t("Event cancelled."));
                          } catch (error) {
                            setNotice(
                              error instanceof Error ? t(error.message) : t("Could not cancel the event."),
                            );
                          }
                        }}
                      >
                        {t("Cancel")}
                      </PixelButton>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </Panel>
          {past.length > 0 && (
            <Panel title={t("PAST AND CANCELLED")}>
              <div className="ev-history">
                {past.map((event) => (
                  <div key={event.id}>
                    <span>{day(event.starts_at, event.tz, language)}</span>
                    <strong>{event.title}</strong>
                    <span>{t(event.status)}</span>
                    <PixelButton
                      variant="ghost"
                      onClick={() =>
                        setEditing({
                          body: {
                            ...fromEvent(event),
                            starts_at: new Date(Date.now() + 86400000).toISOString(),
                            ends_at: new Date(Date.now() + 90000000).toISOString(),
                            sync: false,
                          },
                        })
                      }
                    >
                      {t("Duplicate")}
                    </PixelButton>
                  </div>
                ))}
              </div>
            </Panel>
          )}
        </>
      )}
      {looking && (
        <div className="ev-dialog-backdrop" onClick={() => setLooking(null)}>
          <div
            className="ev-dialog"
            role="dialog"
            aria-modal="true"
            aria-label={t("Event attendees")}
            onClick={(e) => e.stopPropagation()}
          >
            <header>
              <h2>{t("Attendees · {count}", { count: looking.data.count })}</h2>
              <PixelButton variant="ghost" onClick={() => setLooking(null)}>
                {t("Close")}
              </PixelButton>
            </header>
            {looking.data.attendees.length === 0 ? (
              <p className="muted">{t("Nobody has answered yet.")}</p>
            ) : (
              <ul>
                {looking.data.attendees.map((person) => (
                  <li key={person.user_id}>
                    <span>{person.user_id}</span>
                    <strong>{t(person.option)}</strong>
                    {person.waitlisted && <em>{t("Waitlisted")}</em>}
                  </li>
                ))}
              </ul>
            )}
            {looking.data.count > 25 && (
              <p className="muted">
                {t("Showing the first 25. Use /event attendees {id} in Discord for more pages.", {
                  id: looking.id,
                })}
              </p>
            )}
          </div>
        </div>
      )}
      {editing && (
        <div className="ev-dialog-backdrop" onClick={() => setEditing(null)}>
          <div
            className="ev-dialog ev-editor"
            role="dialog"
            aria-modal="true"
            aria-label={editing.id ? t("Edit event") : t("Create event")}
            onClick={(e) => e.stopPropagation()}
          >
            <header>
              <h2>{editing.id ? t("Edit event") : t("Create event")}</h2>
              <PixelButton variant="ghost" onClick={() => setEditing(null)}>
                {t("Close")}
              </PixelButton>
            </header>
            <div className="ev-form">
              <Field label={t("Title")} htmlFor="ev-title">
                <input
                  id="ev-title"
                  className="pixel-input"
                  maxLength={100}
                  value={editing.body.title}
                  onChange={(e) =>
                    setEditing({ ...editing, body: { ...editing.body, title: e.target.value } })
                  }
                />
              </Field>
              <Field label={t("Description")} htmlFor="ev-description">
                <textarea
                  id="ev-description"
                  className="pixel-input"
                  maxLength={1000}
                  rows={3}
                  value={editing.body.description}
                  onChange={(e) =>
                    setEditing({ ...editing, body: { ...editing.body, description: e.target.value } })
                  }
                />
              </Field>
              <div className="ev-form-pair">
                <Field label={t("Starts")} htmlFor="ev-start">
                  <input
                    id="ev-start"
                    type="datetime-local"
                    className="pixel-input"
                    value={zonedInput(editing.body.starts_at, editing.body.tz)}
                    onChange={(e) => changeTime("starts_at", e.target.value)}
                  />
                </Field>
                <Field label={t("Ends")} htmlFor="ev-end">
                  <input
                    id="ev-end"
                    type="datetime-local"
                    className="pixel-input"
                    value={zonedInput(editing.body.ends_at, editing.body.tz)}
                    onChange={(e) => changeTime("ends_at", e.target.value)}
                  />
                </Field>
              </div>
              <p className="muted ev-time-note">
                {t("Enter the wall-clock time in {zone}. Members see it in their own time zone.", {
                  zone: editing.body.tz,
                })}
              </p>
              <Field
                label={t("Event time zone")}
                htmlFor="ev-tz"
                hint={t("IANA name, such as Europe/Istanbul.")}
              >
                <input
                  id="ev-tz"
                  className="pixel-input"
                  defaultValue={editing.body.tz}
                  onBlur={(e) => {
                    try {
                      new Intl.DateTimeFormat("en", { timeZone: e.target.value });
                      setEditing({ ...editing, body: { ...editing.body, tz: e.target.value } });
                    } catch {
                      setNotice(t("Use an IANA time zone such as Europe/Istanbul."));
                    }
                  }}
                />
              </Field>
              <Field label={t("Post RSVP card in")} htmlFor="ev-channel">
                <ChannelPicker
                  id="ev-channel"
                  info={discord.data}
                  value={editing.body.channel_id}
                  onChange={(channel_id) => setEditing({ ...editing, body: { ...editing.body, channel_id } })}
                  none={t("Choose a channel")}
                />
              </Field>
              <div className="ev-form-pair">
                <Field
                  label={t("Going capacity")}
                  htmlFor="ev-capacity"
                  hint={t("0 means no limit; extra RSVPs wait in order.")}
                >
                  <input
                    id="ev-capacity"
                    type="number"
                    min={0}
                    max={100000}
                    className="pixel-input"
                    value={editing.body.options[0]?.capacity ?? 0}
                    onChange={(e) =>
                      setEditing({
                        ...editing,
                        body: {
                          ...editing.body,
                          options: editing.body.options.map((o, i) =>
                            i === 0 ? { ...o, capacity: Number(e.target.value) } : o,
                          ),
                        },
                      })
                    }
                  />
                </Field>
                <Field label={t("Required role")} htmlFor="ev-role" hint={t("Leave empty for everyone.")}>
                  <RolePicker
                    id="ev-role"
                    info={discord.data}
                    value={editing.body.required_role_id}
                    none={t("Everyone can RSVP")}
                    onChange={(required_role_id) =>
                      setEditing({
                        ...editing,
                        body: { ...editing.body, required_role_id },
                      })
                    }
                  />
                </Field>
              </div>
              {[1, 2, 3, ...(editing.body.options.length > 3 ? [4] : [])].map((index) => {
                const option = editing.body.options[index];
                const fixed = index < 3;
                return (
                  <div className="ev-form-pair" key={index}>
                    <Field
                      label={
                        fixed
                          ? t("{name} choice", { name: t(option?.label ?? t("Choice")) })
                          : t("Custom choice {number}", { number: index - 2 })
                      }
                      htmlFor={`ev-option-${index}`}
                      hint={fixed ? t("Default RSVP choice") : t("Leave blank to omit this choice.")}
                    >
                      <input
                        id={`ev-option-${index}`}
                        className="pixel-input"
                        maxLength={40}
                        disabled={fixed}
                        value={fixed ? t(option?.label ?? "") : (option?.label ?? "")}
                        onChange={(e) => {
                          const options = [...editing.body.options];
                          const label = e.target.value;
                          if (label)
                            options[index] = {
                              key: `extra${index - 2}`,
                              label,
                              capacity: option?.capacity ?? 0,
                            };
                          else options.splice(index);
                          setEditing({ ...editing, body: { ...editing.body, options } });
                        }}
                      />
                    </Field>
                    {option && (
                      <Field
                        label={t("Capacity")}
                        htmlFor={`ev-option-cap-${index}`}
                        hint={t("0 means unlimited.")}
                      >
                        <input
                          id={`ev-option-cap-${index}`}
                          type="number"
                          min={0}
                          max={100000}
                          className="pixel-input"
                          value={option.capacity}
                          onChange={(e) =>
                            setEditing({
                              ...editing,
                              body: {
                                ...editing.body,
                                options: editing.body.options.map((o, i) =>
                                  i === index ? { ...o, capacity: Number(e.target.value) } : o,
                                ),
                              },
                            })
                          }
                        />
                      </Field>
                    )}
                  </div>
                );
              })}
              <Field label={t("Location")} htmlFor="ev-location">
                <input
                  id="ev-location"
                  className="pixel-input"
                  value={editing.body.location}
                  onChange={(e) =>
                    setEditing({ ...editing, body: { ...editing.body, location: e.target.value } })
                  }
                />
              </Field>
              <div className="ev-form-pair">
                <Field label={t("Repeat")} htmlFor="ev-repeat">
                  <select
                    id="ev-repeat"
                    className="pixel-input pixel-select"
                    value={editing.body.recurrence?.repeat ?? "off"}
                    onChange={(e) =>
                      setEditing({
                        ...editing,
                        body: {
                          ...editing.body,
                          recurrence:
                            e.target.value === "off"
                              ? null
                              : {
                                  repeat: e.target.value as "daily" | "weekly",
                                  until:
                                    editing.body.recurrence?.until ?? editing.body.starts_at.slice(0, 10),
                                },
                        },
                      })
                    }
                  >
                    <option value="off">{t("No repeat")}</option>
                    <option value="daily">{t("Daily")}</option>
                    <option value="weekly">{t("Weekly")}</option>
                  </select>
                </Field>
                {editing.body.recurrence && (
                  <Field label={t("Until")} htmlFor="ev-until">
                    <input
                      id="ev-until"
                      type="date"
                      className="pixel-input"
                      value={editing.body.recurrence.until}
                      onChange={(e) =>
                        setEditing({
                          ...editing,
                          body: {
                            ...editing.body,
                            recurrence: editing.body.recurrence && {
                              ...editing.body.recurrence,
                              until: e.target.value,
                            },
                          },
                        })
                      }
                    />
                  </Field>
                )}
              </div>
              <label className="ev-check">
                <input
                  type="checkbox"
                  checked={editing.body.sync}
                  onChange={(e) =>
                    setEditing({ ...editing, body: { ...editing.body, sync: e.target.checked } })
                  }
                />{" "}
                {t("Also create a Discord Scheduled Event")}
                <small>{t("Requires Manage Events for the bot.")}</small>
              </label>
              {preview && (
                <div className="ev-preview">
                  <strong>{editing.body.title || t("Event title")}</strong>
                  <p>{editing.body.description || t("Description")}</p>
                  <span>
                    {new Date(editing.body.starts_at).toLocaleString(language)} · {t("Going")} · {t("Maybe")}{" "}
                    · {t("Declined")}
                  </span>
                </div>
              )}
              <div className="ev-form-actions">
                <PixelButton variant="secondary" onClick={() => setPreview(!preview)}>
                  {preview ? t("Hide preview") : t("Preview")}
                </PixelButton>
                <PixelButton
                  disabled={
                    !editing.body.title.trim() ||
                    !editing.body.channel_id ||
                    events.create.isPending ||
                    events.save.isPending
                  }
                  onClick={save}
                >
                  {editing.id ? t("Save changes") : t("Create and post")}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
