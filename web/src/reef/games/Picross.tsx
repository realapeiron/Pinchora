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
            <table className="reef-picross">
              <thead>
                <tr>
                  <th aria-label={t("Row clues")} />
                  {p.cols.map((clues, col) => (
                    <th scope="col" key={col}>
                      {String.fromCharCode(65 + col)}
                      <br />
                      {clues.join(" · ")}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {p.rows.map((clues, row) => (
                  <tr key={row}>
                    <th scope="row">
                      {row + 1}
                      <br />
                      {clues.join(" · ")}
                    </th>
                    {p.cols.map((_, col) => {
                      const cell = row * 10 + col;
                      return (
                        <td key={col}>
                          <button
                            type="button"
                            data-cell={cell}
                            aria-label={`${String.fromCharCode(65 + col)}${row + 1}: ${t(p.grid[cell] === "#" ? "Filled" : "Empty")}`}
                            aria-pressed={p.grid[cell] === "#"}
                            aria-disabled={busy || p.status === "solved"}
                            onClick={() => void mark(cell)}
                            onKeyDown={(e) => {
                              const delta: Record<string, number> = {
                                ArrowLeft: -1,
                                ArrowRight: 1,
                                ArrowUp: -10,
                                ArrowDown: 10,
                              };
                              if (e.key in delta) {
                                e.preventDefault();
                                e.currentTarget
                                  .closest("table")
                                  ?.querySelector<HTMLButtonElement>(
                                    `[data-cell="${Math.max(0, Math.min(99, cell + delta[e.key]!))}"]`,
                                  )
                                  ?.focus();
                              }
                            }}
                          >
                            {p.grid[cell] === "#" ? "■" : "·"}
                          </button>
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}
