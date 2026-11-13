import { t as deferredText } from "../../../i18n/dashboard";
import { locale, translate as spaTranslate } from "../../../i18n";
import "../../../styles/events.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { eventsSummary } from "./eventsSummary";
import { PixelButton } from "../../../components/PixelButton";
import { type Overview } from "../../../lib/api";
import { catalogWords } from "../../admin";
import { useDiscord } from "../../../lib/queries";
import { type Attendees, type EventBody, fromZonedInput, useEvents } from "../api/events";
import { SettingsPage } from "../../../components/PageHead";
import { EventEditor, type EventDraft } from "./EventEditor";
import { EventCalendar } from "./EventCalendar";
import { eventHour } from "./eventTime";
import { Loading, Problem } from "../../../components/States";

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
      { key: "going", label: spaTranslate("Going"), capacity: 0 },
      { key: "maybe", label: deferredText("Maybe"), capacity: 0 },
      { key: "declined", label: spaTranslate("Declined"), capacity: 0 },
    ],
    recurrence: null,
    sync: false,
  };
};
export function Events() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const events = useEvents(guild);
  const language = locale() === "tr" ? "tr-TR" : "en-US";
  const t = catalogWords(events.query.data?.strings, language);
  const discord = useDiscord(guild);
  const [editing, setEditing] = useState<EventDraft | null>(null);
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
  const summary = eventsSummary(zone, upcoming, past);
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
    <SettingsPage
      className="settings-page events-page"
      title={t("EVENTS")}
      lead={t("Plan a gathering, post one RSVP card, and keep the guest list in sync.")}
      action={
        <PixelButton onClick={() => setEditing({ body: fresh(zone) })}>{t("Create event")}</PixelButton>
      }
    >
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
        <div className="settings-detail-list">
          <SettingsRow
            rowId="events-server-clock"
            title={dt("Server clock", {}, "SERVER CLOCK")}
            description={dt("Set the time zone used by event forms.")}
            summary={summary.clock}
            initialOpen
          >
            <span className="ev-zone">{zone}</span>
            <div className="ev-clock">
              <span>{eventHour(new Date().toISOString(), zone, language)}</span>
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
          </SettingsRow>
          <EventCalendar
            {...{ upcoming, past, language, t, summary, showAttendees, setEditing, setNotice, events }}
          />
        </div>
      )}
      {looking && (
        <div
          className="ev-dialog-backdrop"
          role="presentation"
          onClick={(e) => e.target === e.currentTarget && setLooking(null)}
        >
          <div className="ev-dialog" role="dialog" aria-modal="true" aria-label={t("Event attendees")}>
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
        <EventEditor
          {...{
            editing,
            setEditing,
            info: discord.data,
            language,
            t,
            preview,
            setPreview,
            changeTime,
            setNotice,
            events,
            save,
          }}
        />
      )}
    </SettingsPage>
  );
}
