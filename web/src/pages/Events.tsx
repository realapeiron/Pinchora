import "../styles/events.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { type Overview } from "../lib/api";
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
const day = (iso: string, tz: string) =>
  new Intl.DateTimeFormat("en", { timeZone: tz, weekday: "short", day: "numeric", month: "short" }).format(
    new Date(iso),
  );
const hour = (iso: string, tz: string) =>
  new Intl.DateTimeFormat("en", {
    timeZone: tz,
    hour: "numeric",
    minute: "2-digit",
    timeZoneName: "short",
  }).format(new Date(iso));

export function Events() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const events = useEvents(guild);
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
      if (editing.id) await events.save.mutateAsync({ id: editing.id, body: editing.body });
      else await events.create.mutateAsync(editing.body);
      setNotice(editing.id ? "Event saved." : "Event created and posted.");
      setEditing(null);
      setPreview(false);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Could not save event.");
    }
  }
  async function showAttendees(id: string) {
    try {
      setLooking({ id, data: await events.attendees(id) });
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Could not read attendees.");
    }
  }
  function changeTime(field: "starts_at" | "ends_at", value: string) {
    if (!editing || !value) return;
    try {
      setEditing({ ...editing, body: { ...editing.body, [field]: fromZonedInput(value, editing.body.tz) } });
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Choose a valid local time.");
    }
  }
  return (
    <div className="settings-page events-page">
      <PageHead
        title="EVENTS"
        lead="Plan a gathering, post one RSVP card, and keep the guest list in sync."
        action={<PixelButton onClick={() => setEditing({ body: fresh(zone) })}>Create event</PixelButton>}
      />
      {module && !module.enabled && (
        <p className="notice">
          <strong>Events are off.</strong> Turn them on in <Link to={`/g/${guild}/modules`}>Modules</Link>{" "}
          before members can RSVP.
        </p>
      )}
      {notice && (
        <p className="notice" role="status">
          {notice}{" "}
          <button type="button" onClick={() => setNotice(null)} aria-label="Dismiss notice">
            ×
          </button>
        </p>
      )}
      {events.query.isPending && <Loading label="Reading events" />}
      {events.query.isError && <Problem error={events.query.error} />}
      {events.query.isSuccess && (
        <>
          <Panel title="SERVER CLOCK" aside={<span className="ev-zone">{zone}</span>}>
            <div className="ev-clock">
              <span>{hour(new Date().toISOString(), zone)}</span>
              <p className="muted">
                Every event form uses this time zone. Discord still shows each member the correct local time.
              </p>
            </div>
            {tzEdit === null ? (
              <PixelButton variant="secondary" onClick={() => setTzEdit(zone)}>
                Change time zone
              </PixelButton>
            ) : (
              <div className="ev-zone-edit">
                <input
                  className="pixel-input"
                  aria-label="IANA time zone"
                  value={tzEdit}
                  onChange={(e) => setTzEdit(e.target.value)}
                  placeholder="Europe/Istanbul"
                />
                <PixelButton
                  onClick={async () => {
                    try {
                      await events.timezone.mutateAsync(tzEdit);
                      setTzEdit(null);
                      setNotice("Server time zone saved.");
                    } catch {
                      setNotice("Use an IANA time zone such as Europe/Istanbul.");
                    }
                  }}
                >
                  Save time zone
                </PixelButton>
                <PixelButton variant="ghost" onClick={() => setTzEdit(null)}>
                  Cancel
                </PixelButton>
              </div>
            )}
          </Panel>
          <Panel
            title={`UPCOMING · ${upcoming.length}`}
            aside={<span className="ev-small">Next 100 events</span>}
          >
            {upcoming.length === 0 ? (
              <div className="ev-empty">
                <strong>No events on the calendar yet.</strong>
                <p>Choose Create event to post the first RSVP card.</p>
              </div>
            ) : (
              <div className="ev-list">
                {upcoming.map((event) => (
                  <article className="ev-item" key={event.id}>
                    <div className="ev-date">
                      <strong>{day(event.starts_at, event.tz)}</strong>
                      <span>{hour(event.starts_at, event.tz)}</span>
                    </div>
                    <div className="ev-detail">
                      <h3>{event.title}</h3>
                      <p>{event.description || "No description"}</p>
                      <div className="ev-meta">
                        <span>#{event.id}</span>
                        <span>
                          {event.options[0]?.capacity
                            ? `${event.options[0].capacity} going places`
                            : "Open capacity"}
                        </span>
                        {event.recurrence && (
                          <span>
                            {event.recurrence.repeat} until {event.recurrence.until}
                          </span>
                        )}
                        {event.discord_event_id && <span>Discord synced</span>}
                      </div>
                    </div>
                    <div className="ev-actions">
                      <PixelButton variant="secondary" onClick={() => showAttendees(event.id)}>
                        Attendees
                      </PixelButton>
                      <PixelButton
                        variant="secondary"
                        onClick={() => setEditing({ id: event.id, body: fromEvent(event) })}
                      >
                        Edit
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
                        Duplicate
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        onClick={async () => {
                          if (!window.confirm(`Cancel ${event.title}?`)) return;
                          try {
                            await events.cancel.mutateAsync(event.id);
                            setNotice("Event cancelled.");
                          } catch (error) {
                            setNotice(error instanceof Error ? error.message : "Could not cancel the event.");
                          }
                        }}
                      >
                        Cancel
                      </PixelButton>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </Panel>
          {past.length > 0 && (
            <Panel title="PAST AND CANCELLED">
              <div className="ev-history">
                {past.map((event) => (
                  <div key={event.id}>
                    <span>{day(event.starts_at, event.tz)}</span>
                    <strong>{event.title}</strong>
                    <span>{event.status}</span>
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
                      Duplicate
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
            aria-label="Event attendees"
            onClick={(e) => e.stopPropagation()}
          >
            <header>
              <h2>Attendees · {looking.data.count}</h2>
              <PixelButton variant="ghost" onClick={() => setLooking(null)}>
                Close
              </PixelButton>
            </header>
            {looking.data.attendees.length === 0 ? (
              <p className="muted">Nobody has answered yet.</p>
            ) : (
              <ul>
                {looking.data.attendees.map((person) => (
                  <li key={person.user_id}>
                    <span>{person.user_id}</span>
                    <strong>{person.option}</strong>
                    {person.waitlisted && <em>Waitlisted</em>}
                  </li>
                ))}
              </ul>
            )}
            {looking.data.count > 25 && (
              <p className="muted">
                Showing the first 25. Use `/event attendees {looking.id}` in Discord for more pages.
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
            aria-label={editing.id ? "Edit event" : "Create event"}
            onClick={(e) => e.stopPropagation()}
          >
            <header>
              <h2>{editing.id ? "Edit event" : "Create event"}</h2>
              <PixelButton variant="ghost" onClick={() => setEditing(null)}>
                Close
              </PixelButton>
            </header>
            <div className="ev-form">
              <Field label="Title" htmlFor="ev-title">
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
              <Field label="Description" htmlFor="ev-description">
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
                <Field label="Starts" htmlFor="ev-start">
                  <input
                    id="ev-start"
                    type="datetime-local"
                    className="pixel-input"
                    value={zonedInput(editing.body.starts_at, editing.body.tz)}
                    onChange={(e) => changeTime("starts_at", e.target.value)}
                  />
                </Field>
                <Field label="Ends" htmlFor="ev-end">
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
                Enter the wall-clock time in {editing.body.tz}. Members see it in their own time zone.
              </p>
              <Field label="Event time zone" htmlFor="ev-tz" hint="IANA name, such as Europe/Istanbul.">
                <input
                  id="ev-tz"
                  className="pixel-input"
                  defaultValue={editing.body.tz}
                  onBlur={(e) => {
                    try {
                      new Intl.DateTimeFormat("en", { timeZone: e.target.value });
                      setEditing({ ...editing, body: { ...editing.body, tz: e.target.value } });
                    } catch {
                      setNotice("Use an IANA time zone such as Europe/Istanbul.");
                    }
                  }}
                />
              </Field>
              <Field label="Post RSVP card in" htmlFor="ev-channel">
                <ChannelPicker
                  id="ev-channel"
                  info={discord.data}
                  value={editing.body.channel_id}
                  onChange={(channel_id) => setEditing({ ...editing, body: { ...editing.body, channel_id } })}
                  none="Choose a channel"
                />
              </Field>
              <div className="ev-form-pair">
                <Field
                  label="Going capacity"
                  htmlFor="ev-capacity"
                  hint="0 means no limit; extra RSVPs wait in order."
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
                <Field label="Required role" htmlFor="ev-role" hint="Leave empty for everyone.">
                  <RolePicker
                    id="ev-role"
                    info={discord.data}
                    value={editing.body.required_role_id}
                    none="Everyone can RSVP"
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
                      label={fixed ? `${option?.label ?? "Choice"} choice` : `Custom choice ${index - 2}`}
                      htmlFor={`ev-option-${index}`}
                      hint={fixed ? "Default RSVP choice" : "Leave blank to omit this choice."}
                    >
                      <input
                        id={`ev-option-${index}`}
                        className="pixel-input"
                        maxLength={40}
                        disabled={fixed}
                        value={option?.label ?? ""}
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
                      <Field label="Capacity" htmlFor={`ev-option-cap-${index}`} hint="0 means unlimited.">
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
              <Field label="Location" htmlFor="ev-location">
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
                <Field label="Repeat" htmlFor="ev-repeat">
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
                    <option value="off">No repeat</option>
                    <option value="daily">Daily</option>
                    <option value="weekly">Weekly</option>
                  </select>
                </Field>
                {editing.body.recurrence && (
                  <Field label="Until" htmlFor="ev-until">
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
                Also create a Discord Scheduled Event <small>Requires Manage Events for the bot.</small>
              </label>
              {preview && (
                <div className="ev-preview">
                  <strong>{editing.body.title || "Event title"}</strong>
                  <p>{editing.body.description || "Description"}</p>
                  <span>{new Date(editing.body.starts_at).toLocaleString()} · Going · Maybe · Declined</span>
                </div>
              )}
              <div className="ev-form-actions">
                <PixelButton variant="secondary" onClick={() => setPreview(!preview)}>
                  {preview ? "Hide preview" : "Preview"}
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
                  {editing.id ? "Save changes" : "Create and post"}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
