import { useEffect, useRef, useState } from "react";
import { t } from "../../../i18n/dashboard";
import { compact, thousands, type DayRow } from "../api/stats";
import { chartModel } from "./overviewModel";

export function OverviewChart({ rows }: { rows: DayRow[] }) {
  const frame = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 600, height: 230 });
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    if (!frame.current) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      setSize((old) => {
        const width = Math.round(entry.contentRect.width);
        const height = Math.round(entry.contentRect.height);
        return old.width === width && old.height === height ? old : { width, height };
      });
    });
    observer.observe(frame.current);
    return () => observer.disconnect();
  }, []);
  const { width: w, height: h } = size;
  const model = chartModel(rows);
  const left = 42,
    right = 12,
    top = 14,
    bottom = 28;
  const plot = Math.max(1, h - top - bottom);
  const slot = (w - left - right) / Math.max(rows.length, 1);
  const x = (i: number) => left + slot * (i + 0.5);
  const y = (v: number) => top + plot * (1 - v / model.max);
  const barWidth = Math.max(4, Math.floor(slot * (rows.length > 10 ? 0.62 : 0.46)));
  const labelEvery = rows.length > 10 ? (w < 520 ? 7 : 5) : 1;
  const active = hover === null ? null : model.bars[hover];
  const sum = rows.reduce((total, row) => total + row.messages, 0);
  const peak = model.bars.reduce(
    (best, bar) => (bar.value > (best?.value ?? -1) ? bar : best),
    model.bars[0],
  );
  return (
    <>
      <div
        className="overview-chart"
        ref={frame}
        onPointerLeave={() => setHover(null)}
        onPointerMove={(event) => {
          const bounds = frame.current?.getBoundingClientRect();
          if (!bounds) return;
          const index = Math.floor((event.clientX - bounds.left - left) / slot);
          setHover(index >= 0 && index < rows.length ? index : null);
        }}
      >
        <svg viewBox={`0 0 ${w} ${h}`} width={w} height={h} shapeRendering="crispEdges" aria-hidden="true">
          {model.ticks.map((tick) => (
            <g key={tick}>
              <line x1={left} x2={w - right} y1={y(tick)} y2={y(tick)} className="overview-gridline" />
              <text x={left - 8} y={y(tick) + 4} textAnchor="end" className="overview-axis">
                {compact(tick)}
              </text>
            </g>
          ))}
          {model.bars.map((bar, i) => {
            const count = bar.value === 0 ? 0 : Math.max(1, Math.round((y(0) - y(bar.value)) / 8));
            const today = i === model.bars.length - 1;
            return (
              <g
                key={bar.day}
                className={hover !== null && hover !== i ? "overview-bar dim" : "overview-bar"}
              >
                {Array.from({ length: count }, (_, k) => (
                  <rect
                    key={k}
                    x={Math.round(x(i) - barWidth / 2)}
                    y={y(0) - (k + 1) * 8 + 2}
                    width={barWidth}
                    height={6}
                    className={
                      today ? (k === count - 1 ? "today-cap" : "today") : k === count - 1 ? "cap" : "normal"
                    }
                  />
                ))}
                {(i % labelEvery === 0 || today) &&
                  (rows.length <= 10 || today || rows.length - 1 - i >= labelEvery) && (
                    <text x={x(i)} y={h - 7} textAnchor="middle" className="overview-axis">
                      {bar.label}
                    </text>
                  )}
              </g>
            );
          })}
        </svg>
        {active && (
          <div
            className="overview-chart-tip"
            role="status"
            style={{
              left: Math.min(Math.max(x(hover!), 54), w - 54),
              top: Math.max(4, y(active.value) - 58),
            }}
          >
            <strong>{thousands(active.value)}</strong> {t("messages")}
            <span>{active.label}</span>
          </div>
        )}
      </div>
      <table className="visually-hidden">
        <caption>{t("Daily messages")}</caption>
        <thead>
          <tr>
            <th>{t("Day")}</th>
            <th>{t("Messages")}</th>
          </tr>
        </thead>
        <tbody>
          {model.bars.map((bar) => (
            <tr key={bar.day}>
              <th>{bar.label}</th>
              <td>{thousands(bar.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="overview-chart-summary">
        <span>
          {t("Total")} <b>{thousands(sum)}</b>
        </span>
        <span>
          {t("Daily average")} <b>{thousands(Math.round(sum / Math.max(rows.length, 1)))}</b>
        </span>
        {peak && (
          <span>
            {t("Busiest day")} <b>{peak.label}</b>
          </span>
        )}
      </div>
    </>
  );
}
