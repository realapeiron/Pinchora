import { useState } from "react";
import { Link } from "react-router";
import { api } from "../../lib/api";
import { t } from "../i18n";
import type { RunAction } from "../SpacesPanel";

export type Gathering = { id: string; title: string; starts_at: string; room: string };

export function GuildPlazas({
  plaza,
  owner,
  canManage,
  busy,
  run,
  events,
}: {
  plaza: boolean;
  owner: string;
  canManage: boolean;
  busy: boolean;
  run: RunAction;
  events: Gathering[] | undefined;
}) {
  const [guild, setGuild] = useState(plaza ? owner : "");
  const [title, setTitle] = useState("");
  const [start, setStart] = useState("");
  const plazaUrl = (room: string) => {
    const [, g, r, i] = room.split(":");
    return `/me/house?play=1&scope=plaza&guild=${g}&room=${r}&instance=${i}`;
  };
  return (
    <>
      <h3>{t("Guild plazas")}</h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            const result = await api<{ room: string }>(`/api/reef/guilds/${guild}/plazas`);
            window.location.assign(plazaUrl(result.room));
          });
        }}
      >
        <label>
          {t("Server ID")}
          <input
            value={guild}
            inputMode="numeric"
            pattern="[0-9]+"
            onChange={(e) => setGuild(e.target.value)}
            required
          />
        </label>
        <button disabled={busy}>{t("Enter plaza")}</button>
      </form>
      {plaza && (
        <>
          <h3>{t("Scheduled gatherings")}</h3>
          <ul className="reef-objects">
            {events?.map((e) => (
              <li key={e.id}>
                <span>
                  {e.title}
                  <time dateTime={e.starts_at}>{new Date(e.starts_at).toLocaleString()}</time>
                </span>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const r = await api<{ status: string }>(
                        `/api/reef/guilds/${owner}/gatherings/${e.id}/rsvp`,
                        "POST",
                        { option: "going" },
                      );
                      return r.status === "waitlisted" ? "Waitlisted" : "RSVP confirmed";
                    }, "RSVP confirmed")
                  }
                >
                  {t("RSVP")}
                </button>
                <Link to={plazaUrl(e.room)}>{t("Join gathering")}</Link>
              </li>
            ))}
          </ul>
          {canManage && (
            <details>
              <summary>{t("Schedule gathering")}</summary>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  const date = new Date(start);
                  void run(() =>
                    api(`/api/reef/guilds/${owner}/gatherings`, "POST", {
                      title,
                      starts_at: date.toISOString(),
                      ends_at: new Date(date.getTime() + 3600000).toISOString(),
                    }),
                  );
                }}
              >
                <label>
                  {t("Title")}
                  <input value={title} maxLength={100} required onChange={(e) => setTitle(e.target.value)} />
                </label>
                <label>
                  {t("Starts at")}
                  <input
                    type="datetime-local"
                    value={start}
                    required
                    onChange={(e) => setStart(e.target.value)}
                  />
                </label>
                <button disabled={busy}>{t("Schedule")}</button>
              </form>
            </details>
          )}
        </>
      )}
    </>
  );
}
