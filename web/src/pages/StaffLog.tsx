import { translate as spaTranslate } from "../i18n";
import { useState } from "react";
import { useOutletContext } from "react-router";

import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import type { Overview, StaffEntry } from "../lib/api";
import { SOURCES, staffLine } from "../lib/moderation";
import { useStaffLog } from "../lib/queries";
import { timeAgo } from "../lib/text";
import { useDebounced } from "../lib/useDebounced";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";

export function StaffLog() {
  const data = useOutletContext<Overview>();
  const [typed, setTyped] = useState("");
  const [source, setSource] = useState("");
  const q = useDebounced(typed.trim(), 300);
  const log = useStaffLog(data.guild.id, { q, source });
  const rows = log.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("STAFF LOG")}
      lead={spaTranslate(
        "What staff did: moderation commands, changes made on this dashboard, and what Discord's audit log says was done by hand. Kept 180 days.",
      )}
    >
      <Panel title={spaTranslate("FIND")}>
        <div className="filters">
          <Field
            label={spaTranslate("Search")}
            htmlFor="staff-search"
            hint={spaTranslate("A name, an action, a reason, or a user id.")}
          >
            <input
              id="staff-search"
              type="search"
              className="pixel-input"
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              placeholder={spaTranslate("ban, Mia, 1234…")}
            />
          </Field>
          <Field label={spaTranslate("Where")} htmlFor="staff-source">
            <select
              id="staff-source"
              className="pixel-input pixel-select"
              value={source}
              onChange={(event) => setSource(event.target.value)}
            >
              <option value="">{spaTranslate("Everywhere")}</option>
              <option value="command">{spaTranslate("With Pinchora's commands")}</option>
              <option value="web">{spaTranslate("On the dashboard")}</option>
              <option value="discord">{spaTranslate("By hand in Discord")}</option>
            </select>
          </Field>
        </div>
      </Panel>
      <Panel title={spaTranslate("ACTIONS")}>
        {log.isPending && <Loading label={spaTranslate("Reading the log")} />}
        {log.isError && <Problem error={log.error} />}
        {log.isSuccess && rows.length === 0 && (
          <p className="empty-row muted">
            {q || source ? spaTranslate("Nothing matches.") : spaTranslate("Nothing yet.")}
          </p>
        )}
        {rows.length > 0 && (
          <ol className="audit-table" aria-label={spaTranslate("Staff actions")}>
            {rows.map((entry) => (
              <StaffRow key={entry.key} entry={entry} />
            ))}
          </ol>
        )}
        {log.hasNextPage && (
          <div className="panel-foot">
            <PixelButton
              variant="secondary"
              disabled={log.isFetchingNextPage}
              onClick={() => void log.fetchNextPage()}
            >
              {log.isFetchingNextPage ? spaTranslate("Loading…") : spaTranslate("Older actions")}
            </PixelButton>
          </div>
        )}
      </Panel>
    </SettingsPage>
  );
}

function StaffRow({ entry }: { entry: StaffEntry }) {
  const [open, setOpen] = useState(false);
  const more = Boolean(entry.reason || entry.details);
  return (
    <li className="audit-entry">
      <button
        type="button"
        className="audit-summary"
        aria-expanded={more ? open : undefined}
        onClick={() => setOpen(!open)}
      >
        <span className={`audit-dot staff-${entry.source}`} aria-hidden="true" />
        <span className="audit-text">
          <strong>{entry.actor_name}</strong> {staffLine(entry)}
          {entry.case_number !== null && (
            <span className="muted">
              {" "}
              {spaTranslate("· case #")}
              {entry.case_number}
            </span>
          )}
          <span className="muted"> {spaTranslate(SOURCES[entry.source])}</span>
        </span>
        <time className="audit-time muted" dateTime={entry.created_at} title={entry.created_at}>
          {timeAgo(entry.created_at)}
        </time>
      </button>
      {open && more && (
        <table className="diff">
          <tbody>
            {entry.reason && (
              <tr>
                <th scope="row">{spaTranslate("Reason")}</th>
                <td>{entry.reason}</td>
              </tr>
            )}
            {entry.details && (
              <tr>
                <th scope="row">{spaTranslate("Details")}</th>
                <td>{entry.details}</td>
              </tr>
            )}
          </tbody>
        </table>
      )}
    </li>
  );
}
