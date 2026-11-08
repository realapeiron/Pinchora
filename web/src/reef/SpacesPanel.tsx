import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { api } from "../lib/api";
import { t } from "./i18n";

type Preferences = { presence: boolean; visits: string; doorbell: boolean };
type Person = {
  id: string;
  name: string;
  partner: boolean;
  online: boolean;
  visitable: boolean;
  doorbell: boolean;
};
type Bell = { id: string; host_id: string; guest_id: string; status: string };
type Note = { id: string; name: string; body: string; author_id: string };
type Gathering = { id: string; title: string; starts_at: string; room: string };
export function SpacesPanel({
  owner,
  user,
  plaza,
  canManage,
  onClose,
}: {
  owner: string;
  user: string;
  plaza: boolean;
  canManage: boolean;
  onClose: () => void;
}) {
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const [guild, setGuild] = useState(plaza ? owner : "");
  const [draft, setDraft] = useState<Preferences | null>(null);
  const [title, setTitle] = useState("");
  const [start, setStart] = useState("");
  const preferences = useQuery({
    queryKey: ["reef-preferences"],
    queryFn: () => api<Preferences>("/api/reef/preferences"),
  });
  const social = useQuery({
    queryKey: ["reef-social"],
    queryFn: () => api<{ people: Person[] }>("/api/reef/social"),
    refetchInterval: 15000,
  });
  const bells = useQuery({
    queryKey: ["reef-doorbells"],
    queryFn: () => api<{ items: Bell[] }>("/api/reef/doorbells"),
    refetchInterval: 5000,
  });
  const notes = useQuery({
    queryKey: ["reef-notes", owner],
    queryFn: () => api<{ notes: Note[] }>(`/api/reef/houses/${owner}/guestbook`),
    enabled: !plaza,
  });
  const events = useQuery({
    queryKey: ["reef-gatherings", owner],
    queryFn: () => api<{ events: Gathering[] }>(`/api/reef/guilds/${owner}/gatherings`),
    enabled: plaza,
  });
  const p = draft ?? preferences.data;
  const run = async (action: () => Promise<unknown>, success = "Saved") => {
    setBusy(true);
    setMessage("");
    try {
      const outcome = await action();
      setMessage(typeof outcome === "string" ? outcome : success);
      await Promise.all([
        bells.refetch(),
        social.refetch(),
        ...(!plaza ? [notes.refetch()] : [events.refetch()]),
      ]);
    } catch {
      setMessage("This request could not be completed.");
    } finally {
      setBusy(false);
    }
  };
  const visit = (host: string) => `/house/${host}?play=1`;
  const plazaUrl = (room: string) => {
    const [, g, r, i] = room.split(":");
    return `/me/house?play=1&scope=plaza&guild=${g}&room=${r}&instance=${i}`;
  };
  return (
    <aside className="reef-space-sheet" aria-label={t("Social spaces")}>
      <div className="reef-drawer-heading">
        <h2>{t("Social spaces")}</h2>
        <button type="button" onClick={onClose}>
          {t("Close")}
        </button>
      </div>
      <h3>{t("Friends and partner")}</h3>
      {social.isPending && <p>{t("Loading…")}</p>}
      <ul className="reef-objects">
        {social.data?.people.map((person) => (
          <li key={person.id}>
            <span>
              {person.name || person.id}
              {person.partner ? ` · ${t("Partner")}` : ""}
              {person.online ? ` · ${t("Online")}` : ""}
            </span>
            {person.visitable ? (
              <Link to={visit(person.id)}>{t("Visit")}</Link>
            ) : person.doorbell ? (
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(() => api(`/api/reef/houses/${person.id}/doorbells`, "POST", {}), "Doorbell sent")
                }
              >
                {t("Ring doorbell")}
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      <h3>{t("Doorbells")}</h3>
      <ul className="reef-objects">
        {bells.data?.items.map((b) => (
          <li key={b.id}>
            <span>{b.host_id === user ? b.guest_id : b.host_id}</span>
            {b.host_id === user && b.status === "pending" ? (
              <>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(() => api(`/api/reef/doorbells/${b.id}/decision`, "POST", { accept: true }))
                  }
                >
                  {t("Accept")}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(() => api(`/api/reef/doorbells/${b.id}/decision`, "POST", { accept: false }))
                  }
                >
                  {t("Decline")}
                </button>
              </>
            ) : b.guest_id === user && b.status === "accepted" ? (
              <Link to={visit(b.host_id)}>{t("Visit")}</Link>
            ) : (
              <span>
                {t(b.status === "pending" ? "Pending" : b.status === "accepted" ? "Accepted" : "Declined")}
              </span>
            )}
          </li>
        ))}
      </ul>
      {!plaza && (
        <>
          <h3>{t("Guestbook")}</h3>
          {owner !== user && (
            <>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await api<{ appreciated: boolean }>(
                      `/api/reef/houses/${owner}/appreciate`,
                      "POST",
                      {},
                    );
                    if (!result.appreciated) throw Error();
                  }, "Appreciated")
                }
              >
                {t("Appreciate")}
              </button>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void run(async () => {
                    const result = await api<{ signed: boolean }>(
                      `/api/reef/houses/${owner}/guestbook`,
                      "POST",
                      { text },
                    );
                    if (!result.signed) throw Error();
                    setText("");
                  });
                }}
              >
                <label>
                  {t("Leave a note")}
                  <textarea value={text} maxLength={200} onChange={(e) => setText(e.target.value)} required />
                </label>
                <button disabled={busy || !text.trim()}>{t("Sign guestbook")}</button>
              </form>
            </>
          )}
          <ul className="reef-notes">
            {notes.data?.notes.map((n) => (
              <li key={n.id}>
                <strong>{n.name}</strong>
                <p>{n.body}</p>
                <div className="reef-space-actions">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () =>
                          api(`/api/reef/houses/${owner}/guestbook/${n.id}/report`, "POST", {
                            reason: "other",
                          }),
                        "Report received",
                      )
                    }
                  >
                    {t("Report")}
                  </button>
                  {n.author_id !== user && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void run(() => api(`/api/reef/blocks/${n.author_id}`, "PUT", {}))}
                    >
                      {t("Block")}
                    </button>
                  )}
                  {owner === user && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void run(() => api(`/api/me/guestbook/${n.id}`, "DELETE"))}
                    >
                      {t("Delete")}
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
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
            {events.data?.events.map((e) => (
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
      <details>
        <summary>{t("Visit preferences")}</summary>
        {p && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await api("/api/reef/preferences", "PUT", p);
                setDraft(null);
                await preferences.refetch();
              });
            }}
          >
            <label>
              <input
                type="checkbox"
                checked={p.presence}
                onChange={(e) => setDraft({ ...p, presence: e.target.checked })}
              />
              {t("Show online status to friends and partner")}
            </label>
            <label>
              <input
                type="checkbox"
                checked={p.doorbell}
                onChange={(e) => setDraft({ ...p, doorbell: e.target.checked })}
              />
              {t("Allow doorbells")}
            </label>
            <label>
              {t("Who can visit")}
              <select value={p.visits} onChange={(e) => setDraft({ ...p, visits: e.target.value })}>
                <option value="open">{t("Everyone")}</option>
                <option value="friends">{t("Friends and partner")}</option>
                <option value="doorbell">{t("Accepted doorbells")}</option>
                <option value="closed">{t("Nobody")}</option>
              </select>
            </label>
            <button disabled={busy || !draft || JSON.stringify(draft) === JSON.stringify(preferences.data)}>
              {t("Save")}
            </button>
          </form>
        )}
      </details>
      <p role="status">{t(message)}</p>
      {[social, bells, preferences, ...(plaza ? [events] : [notes])].some((q) => q.isError) && (
        <p role="alert">{t("This request could not be completed.")}</p>
      )}
    </aside>
  );
}
