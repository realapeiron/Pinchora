import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { plural as spaPlural, translate as spaTranslate } from "../../../i18n";
import { Panel } from "../../../components/Panel";
import { type Summary, type TicketsOverview, duration } from "../api/tickets";

export function SummaryPanel({ summary }: { summary: TicketsOverview["summary"] }) {
  const [span, setSpan] = useState<"week" | "month">("week");
  const numbers: Summary = summary[span];
  return (
    <Panel
      title={deferredText("AT A GLANCE")}
      aside={
        <div className="tk-segment is-tight" role="group" aria-label={spaTranslate("Period")}>
          {(
            [
              ["week", spaTranslate("7 days")],
              ["month", spaTranslate("30 days")],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              type="button"

              aria-pressed={span === key}
              onClick={() => setSpan(key)}
            >
              {label}
            </button>
          ))}
        </div>
      }
    >
      <ul className="tk-stats">
        <li className="tk-stat">
          <span className="tk-stat-name">{deferredText("Open now")}</span>
          <span className="tk-stat-value">{numbers.open_now}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Opened")}</span>
          <span className="tk-stat-value">{numbers.opened}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Closed")}</span>
          <span className="tk-stat-value">{numbers.closed}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{deferredText("First reply")}</span>
          <span className="tk-stat-value">{duration(numbers.median_first_reply_secs)}</span>
          <span className="tk-stat-detail">{spaTranslate("median")}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Rating")}</span>
          <span className="tk-stat-value">
            {numbers.rating_avg === null ? "—" : numbers.rating_avg.toFixed(1)}
            {numbers.rating_avg !== null && (
              <span className="tk-stars" aria-hidden="true">
                {" "}
                ★
              </span>
            )}
          </span>
          <span className="tk-stat-detail">
            {spaPlural(numbers.ratings, { one: "{count} rating", other: "{count} ratings" })}
          </span>
        </li>
      </ul>
    </Panel>
  );
}
