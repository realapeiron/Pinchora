import { t as deferredText } from "../../../i18n/dashboard";
import type { Case } from "../../../lib/api";
import { dateTime } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { timeAgo } from "../../../lib/text";
import { caseTone } from "./caseView";

export function CasePills({ entry }: { entry: Case }) {
  return (
    <>
      <span className={`cases-kind ${caseTone(entry.kind)}`}>{t(entry.label)}</span>
      <span className={`cases-state ${entry.active ? "is-active" : ""}`}>
        {entry.active ? t("Active") : entry.kind === "warn" ? deferredText("Taken back") : t("Ended")}
      </span>
    </>
  );
}

function target(entry: Case) {
  return entry.target_name || entry.target_id;
}

function openOnKey(event: React.KeyboardEvent, number: number, onOpen: (number: number) => void) {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    onOpen(number);
  }
}

export function CaseList({ rows, onOpen }: { rows: Case[]; onOpen: (number: number) => void }) {
  return (
    <>
      <div className="cases-table-wrap">
        <table className="cases-table">
          <caption className="visually-hidden">{t("Cases newest first")}</caption>
          <thead>
            <tr>
              <th scope="col">#</th>
              <th scope="col">{t("Kind")}</th>
              <th scope="col">{t("Member or channel")}</th>
              <th scope="col">{deferredText("Moderator")}</th>
              <th scope="col">{t("Reason")}</th>
              <th scope="col">{t("Duration")}</th>
              <th scope="col">{t("State")}</th>
              <th scope="col">{t("When")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((entry) => (
              <tr
                key={entry.number}
                tabIndex={0}
                aria-label={t("Open case {number}", { number: entry.number })}
                onClick={() => onOpen(entry.number)}
                onKeyDown={(event) => openOnKey(event, entry.number, onOpen)}
              >
                <td className="cases-number">#{entry.number}</td>
                <td>
                  <span className={`cases-kind ${caseTone(entry.kind)}`}>{t(entry.label)}</span>
                </td>
                <td>{target(entry)}</td>
                <td>{entry.moderator_name}</td>
                <td>
                  <span className="cases-reason" title={entry.reason ?? undefined}>
                    {entry.reason || "—"}
                  </span>
                </td>
                <td>{entry.duration || "—"}</td>
                <td>
                  <span className={`cases-state ${entry.active ? "is-active" : ""}`}>
                    {entry.active
                      ? t("Active")
                      : entry.kind === "warn"
                        ? deferredText("Taken back")
                        : t("Ended")}
                  </span>
                </td>
                <td>
                  <time dateTime={entry.created_at} title={dateTime(entry.created_at)}>
                    {timeAgo(entry.created_at)}
                  </time>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="cases-cards">
        {rows.map((entry) => (
          <button
            key={entry.number}
            type="button"
            className="cases-card"
            aria-label={t("Open case {number}", { number: entry.number })}
            onClick={() => onOpen(entry.number)}
          >
            <span className="cases-card-top">
              <strong>#{entry.number}</strong>
              <CasePills entry={entry} />
              <time dateTime={entry.created_at} title={dateTime(entry.created_at)}>
                {timeAgo(entry.created_at)}
              </time>
            </span>
            <strong className="cases-card-target">{target(entry)}</strong>
            <span className="cases-card-reason">{entry.reason || "—"}</span>
            <span className="cases-card-meta">
              {deferredText("Moderator")}: {entry.moderator_name} · {t("Duration")}: {entry.duration || "—"}
            </span>
          </button>
        ))}
      </div>
    </>
  );
}
