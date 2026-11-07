import { translate as spaTranslate } from "../i18n";
// The stats charts, drawn as SVG and CSS in the cards' pixel style: one column a day, joins against
// leaves, the busiest hours and ranked bars. Each has a hover and keyboard readout and a table with
// the same numbers, so no value hides behind a pointer.

import { type KeyboardEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  type DayRow,
  WEEKDAYS,
  compact,
  dayLabel,
  heatStep,
  hourLabel,
  niceMax,
  peak,
  thousands,
} from "../lib/stats";

/** The width an element has now, kept up to date as the page resizes. */
function useWidth<T extends HTMLElement>(fallback: number) {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(fallback);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    setWidth(element.clientWidth || fallback);
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(Math.round(entry.contentRect.width) || fallback);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [fallback]);
  return { ref, width };
}

/** Moves a highlighted index with the arrow keys, Home and End; Escape lets go. */
function useCursor(count: number) {
  const [active, setActiveIndex] = useState<number | null>(null);
  // Only a move made with the keyboard is read out by a screen reader: a pointer sweeping over the
  // columns would read out every one of them.
  const [spoken, setSpoken] = useState(false);
  const setActive = (index: number | null) => {
    setSpoken(false);
    setActiveIndex(index);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    const keys: Record<string, (i: number) => number> = {
      ArrowRight: (i) => Math.min(count - 1, i + 1),
      ArrowLeft: (i) => Math.max(0, i - 1),
      Home: () => 0,
      End: () => count - 1,
    };
    if (event.key === "Escape") return setActive(null);
    const move = keys[event.key];
    if (!move || count === 0) return;
    event.preventDefault();
    setSpoken(true);
    setActiveIndex((i) => move(i ?? count - 1));
  };
  return { active, setActive, onKeyDown, spoken };
}

/** The floating readout beside the crosshair (left of it near the right edge): the value leads, the
 *  label follows. It sits inside the chart, so the panel around it never cuts it off. */
function Readout({
  x,
  width,
  spoken,
  children,
}: {
  x: number;
  width: number;
  spoken: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(x + 12);
  useEffect(() => {
    const box = ref.current?.offsetWidth ?? 120;
    setLeft(x + 12 + box <= width ? x + 12 : Math.max(0, x - 12 - box));
  }, [x, width, children]);
  return (
    <div ref={ref} className="chart-readout" style={{ left }} aria-live={spoken ? "polite" : "off"}>
      {children}
    </div>
  );
}

/** A line key in the series' colour, for readouts and legends. */
export function Key({ series, shape = "line" }: { series: 1 | 2 | 3; shape?: "line" | "box" }) {
  return <span className={`chart-key chart-key-${shape} series-${series}`} aria-hidden="true" />;
}

const PAD = { left: 44, right: 8, top: 26, bottom: 30 };

/** Gridlines at the top, halfway and the base, with what they stand for. */
function Grid({ top, width, height, ticks }: { top: number; width: number; height: number; ticks: boolean }) {
  const lines = [
    { y: PAD.top, value: top },
    { y: PAD.top + (height - PAD.top - PAD.bottom) / 2, value: top / 2 },
    { y: height - PAD.bottom, value: 0 },
  ];
  return (
    <g className="chart-grid">
      {lines.map((line, i) => (
        <g key={i}>
          <rect
            x={PAD.left}
            y={line.y - (i === 2 ? 1 : 0.5)}
            width={width - PAD.left - PAD.right}
            height={i === 2 ? 2 : 1}
            className={i === 2 ? "chart-base" : "chart-line"}
          />
          {ticks && (i !== 1 || top % 2 === 0) && (
            <text x={PAD.left - 8} y={line.y + 4} textAnchor="end" className="chart-tick">
              {compact(line.value)}
            </text>
          )}
        </g>
      ))}
    </g>
  );
}

/** A column with its top corners cut by one step: the cards' pixel "rounded" end. */
function columnPath(x: number, y: number, w: number, h: number) {
  const c = w >= 6 && h >= 4 ? 2 : 0;
  return `M${x + c},${y}H${x + w - c}V${y + c}H${x + w}V${y + h}H${x}V${y + c}H${x + c}Z`;
}

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

const HOURS = Array.from({ length: 24 }, (_, h) => h);

/** Messages by weekday and hour (UTC), one cell an hour, quiet to busy in one hue. */
export function Heatmap({ grid }: { grid: number[][] }) {
  const max = Math.max(0, ...grid.flat());
  const [active, setActive] = useState<{ d: number; h: number } | null>(null);
  const [spoken, setSpoken] = useState(false);
  const best = peak(grid);
  const onKeyDown = (event: KeyboardEvent) => {
    const moves: Record<string, [number, number]> = {
      ArrowRight: [0, 1],
      ArrowLeft: [0, -1],
      ArrowDown: [1, 0],
      ArrowUp: [-1, 0],
    };
    if (event.key === "Escape") return setActive(null);
    const move = moves[event.key];
    if (!move) return;
    event.preventDefault();
    setSpoken(true);
    setActive((at) => {
      const from = at ?? { d: best?.weekday ?? 0, h: best?.hour ?? 12 };
      return {
        d: Math.min(6, Math.max(0, from.d + move[0])),
        h: Math.min(23, Math.max(0, from.h + move[1])),
      };
    });
  };
  const shown = active ? (grid[active.d]?.[active.h] ?? 0) : null;
  return (
    <div className="heat">
      <div
        className="heat-grid"
        role="img"
        tabIndex={0}
        aria-label={
          best
            ? spaTranslate(
                "Busiest hour: {arg1} {arg2}, {arg3} messages. Use the arrow keys to read each hour.",
                {
                  arg1: spaTranslate(WEEKDAYS[best.weekday]!),
                  arg2: hourLabel(best.hour),
                  arg3: thousands(best.messages),
                },
              )
            : spaTranslate("No messages counted yet.")
        }
        onKeyDown={onKeyDown}
        onBlur={() => setActive(null)}
        onPointerLeave={() => setActive(null)}
      >
        {grid.map((hours, d) => (
          <div className="heat-row" key={d}>
            <span className="heat-day pixel-label" aria-hidden="true">
              {spaTranslate(WEEKDAYS[d]!).slice(0, 3).toUpperCase()}
            </span>
            {hours.map((n, h) => {
              const step = heatStep(n, max);
              const on = active?.d === d && active.h === h;
              return (
                <span
                  key={h}
                  className={`heat-cell ${step === null ? "heat-empty" : `heat-${step}`} ${on ? "is-active" : ""}`}
                  onPointerEnter={() => {
                    setSpoken(false);
                    setActive({ d, h });
                  }}
                />
              );
            })}
          </div>
        ))}
        <div className="heat-hours" aria-hidden="true">
          <span className="heat-day" />
          {HOURS.map((h) => (
            <span key={h} className="heat-hour pixel-label">
              {h % 6 === 0 ? String(h).padStart(2, "0") : ""}
            </span>
          ))}
        </div>
      </div>
      <div className="heat-foot">
        <p className="heat-readout" aria-live={spoken ? "polite" : "off"}>
          {active && shown !== null ? (
            <>
              <strong>{thousands(shown)}</strong> <span className="muted">{spaTranslate("messages ·")}</span>{" "}
              {spaTranslate(WEEKDAYS[active.d]!)} {hourLabel(active.h)}
            </>
          ) : best ? (
            <>
              <span className="muted">{spaTranslate("Busiest:")}</span>{" "}
              {spaTranslate(WEEKDAYS[best.weekday]!)} {hourLabel(best.hour)}
            </>
          ) : (
            <span className="muted">{spaTranslate("Nothing counted yet.")}</span>
          )}
        </p>
        <span className="heat-scale" aria-hidden="true">
          <span className="pixel-label muted">{spaTranslate("QUIET")}</span>
          {[0, 1, 2, 3, 4].map((s) => (
            <span key={s} className={`heat-cell heat-${s}`} />
          ))}
          <span className="pixel-label muted">{spaTranslate("BUSY")}</span>
        </span>
      </div>
    </div>
  );
}

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

/** One headline number, with the change from the period before when there was one. */
export function StatTile({
  label,
  value,
  note,
  delta,
  upIsGood = true,
  period,
}: {
  label: string;
  value: string;
  note?: ReactNode;
  delta?: number | null;
  upIsGood?: boolean;
  period?: number;
}) {
  const direction =
    delta === null || delta === undefined ? null : delta > 0 ? "up" : delta < 0 ? "down" : "flat";
  const good = direction === "flat" ? null : (direction === "up") === upIsGood;
  return (
    <div className="stat-tile">
      <span className="pixel-label muted">{label}</span>
      <strong className="stat-tile-value">{value}</strong>
      {direction && (
        <span className="stat-tile-delta">
          <span
            className={`delta-mark ${good === null ? "" : good ? "is-good" : "is-bad"}`}
            aria-hidden="true"
          >
            {direction === "up" ? "▲" : direction === "down" ? "▼" : "■"}
          </span>
          <span>
            {direction === "flat" ? spaTranslate("No change") : `${delta! > 0 ? "+" : ""}${delta}%`}
            <span className="muted delta-long">
              {" "}
              {spaTranslate("vs the")} {period} {spaTranslate("days before")}
            </span>
            <span className="muted delta-short">
              {" "}
              {spaTranslate("vs previous")} {period}d
            </span>
          </span>
        </span>
      )}
      {note && <span className="stat-tile-note muted">{note}</span>}
    </div>
  );
}
