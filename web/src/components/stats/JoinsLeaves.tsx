import { translate as spaTranslate } from "../../i18n";
import { type DayRow, dayLabel, niceMax, thousands, compact } from "../../features/admin/api/stats";
import { useWidth, useCursor, Readout, Key, PAD, columnPath } from "./ChartPrimitives";

/** Joins above the line and leaves below it, one pair of columns a day. */
export function JoinsLeaves({ days, height = 200 }: { days: DayRow[]; height?: number }) {
  const { ref, width } = useWidth<HTMLDivElement>(480);
  const cursor = useCursor(days.length);
  const most = Math.max(0, ...days.map((d) => Math.max(d.joins, d.leaves)));
  const scale = niceMax(most);
  const top = PAD.top;
  const bottom = height - PAD.bottom;
  const mid = (top + bottom) / 2;
  const reach = mid - top - 2;
  const slot = (width - PAD.left - PAD.right) / Math.max(1, days.length);
  const bar = Math.max(1, Math.min(24, slot - 2));
  const joins = days.reduce((a, d) => a + d.joins, 0);
  const leaves = days.reduce((a, d) => a + d.leaves, 0);
  const active = cursor.active;
  const shown = active !== null ? days[active] : undefined;
  return (
    <div className="chart" ref={ref}>
      <ul className="chart-legend">
        <li>
          <Key series={2} shape="box" />
          {spaTranslate("Joins")} <strong>{thousands(joins)}</strong>
        </li>
        <li>
          <Key series={3} shape="box" />
          {spaTranslate("Leaves")} <strong>{thousands(leaves)}</strong>
        </li>
      </ul>
      <svg
        width={width}
        height={height}
        role="img"
        tabIndex={0}
        aria-label={spaTranslate(
          "Joins and leaves per day: {arg1} joined and {arg2} left. Use the arrow keys to read each day.",
          { arg1: thousands(joins), arg2: thousands(leaves) },
        )}
        onKeyDown={cursor.onKeyDown}
        onBlur={() => cursor.setActive(null)}
        onPointerLeave={() => cursor.setActive(null)}
      >
        <g className="chart-grid">
          <rect
            x={PAD.left}
            y={mid - 1}
            width={width - PAD.left - PAD.right}
            height={2}
            className="chart-base"
          />
          <text x={PAD.left - 8} y={top + 8} textAnchor="end" className="chart-tick">
            +{compact(scale)}
          </text>
          <text x={PAD.left - 8} y={mid + 4} textAnchor="end" className="chart-tick">
            0
          </text>
          <text x={PAD.left - 8} y={bottom} textAnchor="end" className="chart-tick">
            −{compact(scale)}
          </text>
        </g>
        {days.map((d, i) => {
          const x = PAD.left + i * slot + (slot - bar) / 2;
          const up = d.joins > 0 ? Math.max(2, (d.joins / scale) * reach) : 0;
          const down = d.leaves > 0 ? Math.max(2, (d.leaves / scale) * reach) : 0;
          return (
            <g key={d.day} className={active === i ? "is-active" : undefined}>
              {up > 0 && <path d={columnPath(x, mid - 1 - up, bar, up)} className="chart-column series-2" />}
              {down > 0 && (
                <rect x={x} y={mid + 1} width={bar} height={down} className="chart-column series-3" />
              )}
              <rect
                x={PAD.left + i * slot}
                y={top}
                width={slot}
                height={bottom - top}
                className="chart-hit"
                onPointerEnter={() => cursor.setActive(i)}
              />
            </g>
          );
        })}
        {days[0] && (
          <text x={PAD.left} y={height - 8} className="chart-tick">
            {dayLabel(days[0].day)}
          </text>
        )}
        {days.length > 1 && (
          <text x={width - PAD.right} y={height - 8} textAnchor="end" className="chart-tick">
            {dayLabel(days[days.length - 1]!.day)}
          </text>
        )}
      </svg>
      {most === 0 && (
        <p className="chart-empty">
          <span className="muted">{spaTranslate("Nobody joined or left in these days.")}</span>
        </p>
      )}
      {shown && active !== null && (
        <Readout x={PAD.left + active * slot + slot / 2} width={width} spoken={cursor.spoken}>
          <span className="chart-readout-row">
            <Key series={2} /> <strong>{thousands(shown.joins)}</strong>{" "}
            <span className="muted">{spaTranslate("joined")}</span>
          </span>
          <span className="chart-readout-row">
            <Key series={3} /> <strong>{thousands(shown.leaves)}</strong>{" "}
            <span className="muted">{spaTranslate("left")}</span>
          </span>
          <span className="chart-readout-sub">{dayLabel(shown.day, true)}</span>
        </Readout>
      )}
    </div>
  );
}
