import { t } from "../../i18n/reef-games";
import type { Puzzle } from "./Picross";
export default function PuzzleGrid({
  p,
  busy,
  mark,
}: {
  p: Puzzle;
  busy: boolean;
  mark: (cell: number) => Promise<void>;
}) {
  return (
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
  );
}
