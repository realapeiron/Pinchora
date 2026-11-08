import PuzzleGrid from "./PuzzleGrid";
import { refreshEconomy } from "./economy";
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { t } from "../../i18n/reef-games";
export type Puzzle = {
  mode: string;
  day: number;
  revision: string;
  grid: string;
  checks: number;
  status: string;
  paid: number;
  rows: number[][];
  cols: number[][];
};
export default function Picross({ room }: { room: string }) {
  const {
    mode,
    setMode,
    busy,
    setBusy,
    error,
    setError,
    filled,
    setFilled,
    client,
    path,
    key,
    query,
    p,
    mark,
  } = usePuzzle(room);
  return (
    <section aria-label={t("Pixel Picross")}>
      <label>
        {t("Puzzle mode")}
        <select disabled={busy} value={mode} onChange={(e) => setMode(e.target.value)}>
          <option value="daily">{t("Daily")}</option>
          <option value="practice">{t("Practice")}</option>
        </select>
      </label>
      <p>{t("Progress and rewards are shared with Discord.")}</p>
      {query.isPending && <p role="status">{t("Loading…")}</p>}
      {(error || query.isError) && <p role="alert">{t(error || "This request could not be completed.")}</p>}
      {p && (
        <>
          {mode === "practice" && p.status === "solved" && (
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void api<Puzzle>(path, "POST", { mode, next_revision: p.revision })
                  .then((saved) => client.setQueryData(key, saved))
                  .catch(() => setError("The board changed. Refresh and try again."))
                  .finally(() => setBusy(false));
              }}
            >
              {t("Next practice puzzle")}
            </button>
          )}
          <div role="group" aria-label={t("Mark cells")}>
            <button type="button" aria-pressed={filled} onClick={() => setFilled(true)}>
              {t("Fill")}
            </button>
            <button type="button" aria-pressed={!filled} onClick={() => setFilled(false)}>
              {t("Clear")}
            </button>
          </div>
          <p role="status">
            {p.status === "solved"
              ? t("Puzzle solved! {count} shells", { count: p.paid })
              : t("Match every row and column clue. Arrow keys move between cells; Space marks a cell.")}
          </p>
          <div
            className="reef-board-scroll"
            role="region"
            tabIndex={0}
            aria-label={t("Picross clues and cells")}
          >
            <PuzzleGrid p={p} busy={busy} mark={mark} />
          </div>
        </>
      )}
    </section>
  );
}

function usePuzzle(room: string) {
  const [mode, setMode] = useState("daily");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [filled, setFilled] = useState(true);
  const client = useQueryClient();
  const path = `/api/reef/rooms/${encodeURIComponent(room)}/picross`;
  const key = ["reef-picross", room, mode];
  const query = useQuery({
    queryKey: key,
    queryFn: () => api<Puzzle>(path, "POST", { mode }),
    refetchInterval: 5000,
  });
  const p = query.data;
  useEffect(() => {
    if (p?.status === "solved") void refreshEconomy(client);
  }, [client, p?.status, p?.revision, p?.day, mode]);
  const mark = async (cell: number) => {
    if (!p || busy || p.status === "solved") return;
    setBusy(true);
    setError("");
    try {
      const saved = await api<Puzzle>(`${path}/marks`, "POST", {
        mode,
        day: p.day,
        revision: p.revision,
        grid: p.grid,
        cell,
        filled,
      });
      client.setQueryData(key, saved);
    } catch {
      setError("The board changed. Refresh and try again.");
      await query.refetch();
    } finally {
      setBusy(false);
    }
  };
  return {
    mode,
    setMode,
    busy,
    setBusy,
    error,
    setError,
    filled,
    setFilled,
    client,
    path,
    key,
    query,
    p,
    mark,
  };
}
