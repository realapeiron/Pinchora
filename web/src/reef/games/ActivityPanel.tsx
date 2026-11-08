import { createPortal } from "react-dom";
import { refreshEconomy } from "./economy";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { locale } from "../../i18n";
import { t } from "../../i18n/reef-games";
import type { Game, Intent } from "./types";
import "./games.css";
const Picross = lazy(() => import("./Picross"));
import Match from "./Match";
import { names } from "./types";
export default function ActivityPanel({
  room,
  user,
  placement,
  onClose,
  bubbleTarget,
}: {
  room: string;
  user: string;
  placement: string | null;
  onClose: () => void;
  bubbleTarget: HTMLElement | null;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus();
  }, []);
  const { selected, select, picross, setPicross, busy, error, now, query, game, send, create } =
    useActivities(room, placement);
  return (
    <aside className="reef-space-sheet reef-activities" aria-label={t("Room activities")}>
      {bubbleTarget &&
        game?.kind === "trivia" &&
        game.board.phase !== "waiting" &&
        createPortal(
          <div className="reef-activity-bubble" role="status">
            {t(game.board.phase === "done" ? "Round finished" : game.answered ? "Answered" : "Thinking…")}
          </div>,
          bubbleTarget,
        )}
      <div className="reef-drawer-heading">
        <h2 ref={heading} tabIndex={-1}>
          {t("Room activities")}
        </h2>
        <button type="button" onClick={onClose}>
          {t("Close")}
        </button>
      </div>
      <p>
        {t(
          room.startsWith("house:")
            ? "House matches are practice: no shell rewards."
            : "Plaza wins share the daily reward limits with Discord.",
        )}
      </p>
      <div className="reef-activity-actions">
        {Object.entries(names).map(([kind, name]) => (
          <button type="button" disabled={busy} key={kind} onClick={() => create(kind)}>
            {t(name)}
          </button>
        ))}
        <button
          type="button"
          onClick={() => {
            setPicross(true);
            select(null);
          }}
        >
          {t("Pixel Picross")}
        </button>
      </div>
      {(error || query.isError) && <p role="alert">{t(error || "This request could not be completed.")}</p>}
      {query.isPending && <p role="status">{t("Loading…")}</p>}
      <ul className="reef-objects">
        {query.data?.games.map((g) => (
          <li key={g.id}>
            <button
              type="button"
              aria-pressed={selected === g.id}
              onClick={() => {
                select(g.id);
                setPicross(false);
              }}
            >
              {t(names[g.kind])} · {g.id} ·{" "}
              {t(
                g.board.phase === "done"
                  ? "Finished"
                  : g.board.phase === "waiting"
                    ? "Waiting for players"
                    : "Playing",
              )}
            </button>
          </li>
        ))}
      </ul>
      <Suspense fallback={<p role="status">{t("Loading…")}</p>}>
        {picross && <Picross room={room} />}
        {game && <Match game={game} busy={busy} send={send} user={user} now={now} />}
      </Suspense>
    </aside>
  );
}

function useActivities(room: string, placement: string | null) {
  const [selected, select] = useState<string | null>(null);
  const [picross, setPicross] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [now, setNow] = useState(Date.now);
  const client = useQueryClient();
  const path = `/api/reef/rooms/${encodeURIComponent(room)}/games`;
  const query = useQuery({
    queryKey: ["reef-games", room],
    queryFn: () => api<{ games: Game[]; layout_revision: string }>(path),
    refetchInterval: 2000,
  });
  const game = query.data?.games.find((g) => g.id === selected);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (game?.board.phase === "done") void refreshEconomy(client);
  }, [client, game?.id, game?.board.phase]);
  const run = async (f: () => Promise<Game>) => {
    setBusy(true);
    setError("");
    try {
      const g = await f();
      select(g.id);
      setPicross(false);
      await query.refetch();
    } catch {
      setError("The board changed. Refresh and try again.");
      await query.refetch();
    } finally {
      setBusy(false);
    }
  };
  const send = (intent: Intent) => {
    if (game)
      void run(() =>
        api<Game>(`/api/reef/games/${game.id}/actions`, "POST", {
          request_id: crypto.randomUUID(),
          revision: game.revision,
          ...intent,
        }),
      );
  };
  const create = (kind: string) =>
    void run(() =>
      api<Game>(path, "POST", {
        kind,
        request_id: crypto.randomUUID(),
        lang: locale(),
        placement_id: placement,
        layout_revision: placement ? query.data?.layout_revision : null,
      }),
    );
  return { selected, select, picross, setPicross, busy, error, now, query, game, send, create };
}
