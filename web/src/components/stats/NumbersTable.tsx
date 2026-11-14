import { translate as spaTranslate } from "../../i18n";
import { thousands } from "../../features/admin/api/stats";

/** The numbers behind a chart, as a table, folded away until asked for. */
export function NumbersTable({
  caption,
  columns,
  rows,
}: {
  caption: string;
  columns: string[];
  rows: (string | number)[][];
}) {
  return (
    <details className="chart-table">
      <summary>{spaTranslate("Show the numbers")}</summary>
      <div className="chart-table-scroll">
        <table>
          <caption className="visually-hidden">{caption}</caption>
          <thead>
            <tr>
              {columns.map((c) => (
                <th key={c} scope="col">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr key={i}>
                {row.map((cell, j) =>
                  j === 0 ? (
                    <th key={j} scope="row">
                      {cell}
                    </th>
                  ) : (
                    <td key={j}>{typeof cell === "number" ? thousands(cell) : cell}</td>
                  ),
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}
