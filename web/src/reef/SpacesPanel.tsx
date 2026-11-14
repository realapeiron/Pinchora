import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { FriendsAndDoorbells, type Person, type Bell } from "./spaces/FriendsAndDoorbells";
import { Guestbook, type Note } from "./spaces/Guestbook";
import { GuildPlazas, type Gathering } from "./spaces/GuildPlazas";
import { VisitPreferences, type Preferences } from "./spaces/VisitPreferences";
import { t } from "./i18n";

export type RunAction = (action: () => Promise<unknown>, success?: string) => Promise<void>;
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
  return (
    <aside className="reef-space-sheet" aria-label={t("Social spaces")}>
      <div className="reef-drawer-heading">
        <h2>{t("Social spaces")}</h2>
        <button type="button" onClick={onClose}>
          {t("Close")}
        </button>
      </div>
      <FriendsAndDoorbells
        people={social.data?.people}
        socialPending={social.isPending}
        bellItems={bells.data?.items}
        user={user}
        busy={busy}
        run={run}
      />
      <Guestbook plaza={plaza} owner={owner} user={user} busy={busy} run={run} notes={notes.data?.notes} />
      <GuildPlazas
        plaza={plaza}
        owner={owner}
        canManage={canManage}
        busy={busy}
        run={run}
        events={events.data?.events}
      />
      <VisitPreferences
        busy={busy}
        preferencesData={preferences.data}
        refetchPreferences={() => preferences.refetch()}
        run={run}
      />
      <p role="status">{t(message)}</p>
      {[social, bells, preferences, ...(plaza ? [events] : [notes])].some((q) => q.isError) && (
        <p role="alert">{t("This request could not be completed.")}</p>
      )}
    </aside>
  );
}
