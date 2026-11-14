import { type ReactNode } from "react";
import { translate as spaTranslate } from "../../i18n";

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
