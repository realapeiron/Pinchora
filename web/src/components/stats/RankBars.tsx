import { thousands } from "../../features/admin/api/stats";

export interface Ranked {
  id: string;
  name: string;
  value: number;
  /** What the row says on the right; the value when not given. */
  shown?: string;
  avatar?: string | null;
}

/** A ranked list with a bar for each row, the longest the busiest. */
export function RankBars({ rows, empty, label }: { rows: Ranked[]; empty: string; label: string }) {
  if (rows.length === 0) return <p className="empty-row muted">{empty}</p>;
  const most = Math.max(1, ...rows.map((r) => r.value));
  return (
    <ol className="rank" aria-label={label}>
      {rows.map((row, i) => (
        <li key={row.id} className="rank-row">
          <span className="rank-place pixel-label muted">{i + 1}</span>
          {row.avatar !== undefined &&
            (row.avatar ? (
              <img className="rank-avatar" src={row.avatar} alt="" width={24} height={24} />
            ) : (
              <span className="rank-avatar" aria-hidden="true" />
            ))}
          <span className="rank-name">{row.name}</span>
          <span className="rank-value">{row.shown ?? thousands(row.value)}</span>
          <span className="rank-track" aria-hidden="true">
            <span className="rank-fill series-1" style={{ width: `${(row.value / most) * 100}%` }} />
          </span>
        </li>
      ))}
    </ol>
  );
}
