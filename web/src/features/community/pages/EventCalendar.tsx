import type { Dispatch, SetStateAction } from "react";
import { SettingsRow } from "../../../components/SettingsRow";
import { PixelButton } from "../../../components/PixelButton";
import { t as dt } from "../../../i18n/dashboard";
import { catalogWords } from "../../admin";
import { type Event, type EventBody, useEvents } from "../api/events";
import { type EventDraft } from "./EventEditor";
import { type eventsSummary } from "./eventsSummary";
import { eventHour } from "./eventTime";

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
export function EventCalendar({
  upcoming,
  past,
  language,
  t,
  summary,
  showAttendees,
  setEditing,
  setNotice,
  events,
}: {
  upcoming: Event[];
  past: Event[];
  language: string;
  t: ReturnType<typeof catalogWords>;
  summary: ReturnType<typeof eventsSummary>;
  showAttendees: (id: string) => Promise<void>;
  setEditing: Dispatch<SetStateAction<EventDraft | null>>;
  setNotice: (value: string) => void;
  events: ReturnType<typeof useEvents>;
}) {
  return (
    <>
      <SettingsRow
        title={t("UPCOMING · {count}", { count: upcoming.length })}
        description={dt("Review upcoming events and guest lists.")}
        summary={summary.upcoming}
      >
        <span className="ev-small">{t("Next 100 events")}</span>
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
                  <span>{eventHour(event.starts_at, event.tz, language)}</span>
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
      </SettingsRow>
      {past.length > 0 && (
        <SettingsRow
          title={t("PAST AND CANCELLED")}
          description={dt("Review past and cancelled events.")}
          summary={summary.history}
        >
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
        </SettingsRow>
      )}
    </>
  );
}
