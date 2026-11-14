import { translate as spaTranslate } from "../../i18n";
import { type DayRow, dayLabel, niceMax, thousands } from "../../features/admin/api/stats";
import { useWidth, useCursor, Readout, PAD, Grid, columnPath } from "./ChartPrimitives";

/** One column a day for one measure (messages, voice minutes), with the busiest day labelled. */
export function DailyColumns({
  days,
  value,
  label,
  format = thousands,
  height = 220,
}: {
  days: DayRow[];
  value: (d: DayRow) => number;
  /** What the columns count, e.g. "messages". */
  label: string;
  format?: (n: number) => string;
  height?: number;
}) {
  const { ref, width } = useWidth<HTMLDivElement>(640);
  const cursor = useCursor(days.length);
  const values = days.map(value);
  const most = Math.max(0, ...values);
  const top = niceMax(most);
  const plotH = height - PAD.top - PAD.bottom;
  const slot = (width - PAD.left - PAD.right) / Math.max(1, days.length);
  const bar = Math.max(1, Math.min(24, slot - 2));
  const peakIndex = most > 0 ? values.indexOf(most) : -1;
  const active = cursor.active;
  const shown = active !== null ? days[active] : undefined;
  const total = values.reduce((a, b) => a + b, 0);
  const first = days[0];
  const last = days[days.length - 1];
  return (
    <div className="chart" ref={ref}>
      <svg
        width={width}
        height={height}
        role="img"
        tabIndex={0}
        aria-label={`${label} per day: ${format(total)} in all${
          peakIndex >= 0
            ? spaTranslate(", the most on {arg1} with {arg2}", {
                arg1: dayLabel(days[peakIndex]!.day),
                arg2: format(most),
              })
            : ""
        }. Use the arrow keys to read each day.`}
        onKeyDown={cursor.onKeyDown}
        onBlur={() => cursor.setActive(null)}
        onPointerLeave={() => cursor.setActive(null)}
      >
        <Grid top={top} width={width} height={height} ticks={most > 0} />
        {values.map((v, i) => {
          const h = v > 0 ? Math.max(2, (v / top) * plotH) : 0;
          const x = PAD.left + i * slot + (slot - bar) / 2;
          return (
            <g key={days[i]!.day}>
              {h > 0 && (
                <path
                  d={columnPath(x, height - PAD.bottom - h, bar, h)}
                  className={`chart-column series-1 ${active === i ? "is-active" : ""}`}
                />
              )}
              {i === peakIndex && (
                <text
                  x={Math.min(Math.max(x + bar / 2, PAD.left + 16), width - PAD.right - 16)}
                  y={height - PAD.bottom - h - 8}
                  textAnchor="middle"
                  className="chart-peak"
                >
                  {format(v)}
                </text>
              )}
              <rect
                x={PAD.left + i * slot}
                y={PAD.top - 20}
                width={slot}
                height={plotH + 20}
                className="chart-hit"
                onPointerEnter={() => cursor.setActive(i)}
              />
            </g>
          );
        })}
        {active !== null && (
          <rect
            x={PAD.left + active * slot + slot / 2 - 0.5}
            y={PAD.top}
            width={1}
            height={plotH}
            className="chart-crosshair"
          />
        )}
        {first && (
          <text x={PAD.left} y={height - 8} className="chart-tick">
            {dayLabel(first.day)}
          </text>
        )}
        {last && days.length > 1 && (
          <text x={width - PAD.right} y={height - 8} textAnchor="end" className="chart-tick">
            {dayLabel(last.day)}
          </text>
        )}
      </svg>
      {shown && active !== null && (
        <Readout x={PAD.left + active * slot + slot / 2} width={width} spoken={cursor.spoken}>
          <strong>{format(value(shown))}</strong> <span className="muted">{label}</span>
          <span className="chart-readout-sub">{dayLabel(shown.day, true)}</span>
        </Readout>
      )}
      {most === 0 && (
        <p className="chart-empty">
          <span className="muted">{spaTranslate("Nothing counted in these days yet.")}</span>
        </p>
      )}
    </div>
  );
}
