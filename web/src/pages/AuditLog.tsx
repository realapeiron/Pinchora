import { useState } from "react";
import { useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import type { AuditEntry, Overview } from "../lib/api";
import { useAudit } from "../lib/queries";
import { describeChange, timeAgo } from "../lib/text";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

export function AuditLog() {
  const data = useOutletContext<Overview>();
  const audit = useAudit(data.guild.id);
  const rows = audit.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <div className="settings-page">
      <PageHead
        title="AUDIT LOG"
        lead="Every change made on the dashboard: who, what, and when. Kept 180 days."
      />
      <Panel title="CHANGES">
        {audit.isPending && <Loading label="Reading the log" />}
        {audit.isError && <Problem error={audit.error} />}
        {audit.isSuccess && rows.length === 0 && (
          <p className="empty-row muted">No changes yet. What you save on the dashboard shows up here.</p>
        )}
        {rows.length > 0 && (
          <ol className="audit-table" aria-label="Changes">
            {rows.map((entry) => (
              <AuditRow key={entry.id} entry={entry} />
            ))}
          </ol>
        )}
        {audit.hasNextPage && (
          <div className="panel-foot">
            <PixelButton
              variant="secondary"
              disabled={audit.isFetchingNextPage}
              onClick={() => void audit.fetchNextPage()}
            >
              {audit.isFetchingNextPage ? "Loading…" : "Older changes"}
            </PixelButton>
          </div>
        )}
      </Panel>
    </div>
  );
}

function show(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function AuditRow({ entry }: { entry: AuditEntry }) {
  const [open, setOpen] = useState(false);
  const fields = Array.from(new Set([...Object.keys(entry.before ?? {}), ...Object.keys(entry.after ?? {})]));
  return (
    <li className="audit-entry">
      <button type="button" className="audit-summary" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className="audit-dot" aria-hidden="true" />
        <span className="audit-text">
          <strong>{entry.actor_name}</strong> {describeChange(entry)}
          {entry.target && <span className="muted"> · {entry.target}</span>}
        </span>
        <time className="audit-time muted" dateTime={entry.created_at} title={entry.created_at}>
          {timeAgo(entry.created_at)}
        </time>
      </button>
      {open && fields.length > 0 && (
        <table className="diff">
          <thead>
            <tr>
              <th scope="col">Field</th>
              <th scope="col">Before</th>
              <th scope="col">After</th>
            </tr>
          </thead>
          <tbody>
            {fields.map((field) => (
              <tr key={field}>
                <th scope="row">{field.replaceAll("_", " ")}</th>
                <td className="diff-before">{show(entry.before?.[field])}</td>
                <td className="diff-after">{show(entry.after?.[field])}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </li>
  );
}
