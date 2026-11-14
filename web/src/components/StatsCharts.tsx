import { type KeyboardEvent, useState } from "react";
import { translate as spaTranslate } from "../i18n";
import { WEEKDAYS, heatStep, hourLabel, peak, thousands } from "../features/admin/api/stats";

export { Key } from "./stats/ChartPrimitives";
export { DailyColumns } from "./stats/DailyColumns";
export { JoinsLeaves } from "./stats/JoinsLeaves";
export { RankBars, type Ranked } from "./stats/RankBars";
export { NumbersTable } from "./stats/NumbersTable";
export { StatTile } from "./stats/StatTile";

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
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Arrow keys explore the labelled chart. */}
      <div
        className="heat-grid"
        role="img"
        /* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- The chart needs a keyboard focus target. */
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
