import { useState } from "react";
import { useOutletContext } from "react-router";

import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { ApiError, type Case, type Overview } from "../lib/api";
import { type CaseFilters, useCase, useCases, useSaveReason } from "../lib/queries";
import { timeAgo } from "../lib/text";
import { useDebounced } from "../lib/useDebounced";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

/** The tone of a kind of case, like the mod-log's colours. */
function tone(kind: string): string {
  if (kind === "ban" || kind === "kick") return "is-danger";
  if (["unban", "untimeout", "unwarn", "unlock"].includes(kind)) return "is-good";
  if (["timeout", "warn", "purge", "lock"].includes(kind)) return "is-warn";
  return "";
}

export function Cases() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const [typed, setTyped] = useState("");
  const [kind, setKind] = useState("");
  const [active, setActive] = useState(false);
  const q = useDebounced(typed.trim(), 300);
  const filters: CaseFilters = { q, kind, member: "", active };
  const cases = useCases(guild, filters);
  const rows = cases.data?.pages.flatMap((p) => p.items) ?? [];
  const kinds = cases.data?.pages[0]?.kinds ?? [];
  const [open, setOpen] = useState<number | null>(null);
  return (
    <div className="settings-page">
      <PageHead
        title="CASES"
        lead="Every moderation action taken with Pinchora, numbered per server. Open one to see it whole and give it a better reason."
      />
      <Panel title="FIND">
        <div className="filters">
          <Field
            label="Search"
            htmlFor="case-search"
            hint="A name, a reason, a word in the details, or a user id."
          >
            <input
              id="case-search"
              type="search"
              className="pixel-input"
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              placeholder="spam, Mia, 1234…"
            />
          </Field>
          <Field label="Kind" htmlFor="case-kind">
            <select
              id="case-kind"
              className="pixel-input pixel-select"
              value={kind}
              onChange={(event) => setKind(event.target.value)}
            >
              <option value="">Every kind</option>
              {kinds.map((k) => (
                <option key={k.key} value={k.key}>
                  {k.label}
                </option>
              ))}
            </select>
          </Field>
          <div className="filters-switch">
            <span className="field-label">Still standing</span>
            <PixelToggle
              label="Only bans, warns and timeouts that still stand"
              checked={active}
              onChange={setActive}
            />
          </div>
        </div>
      </Panel>
      <Panel title="ALL CASES">
        {cases.isPending && <Loading label="Reading the cases" />}
        {cases.isError && <Problem error={cases.error} />}
        {cases.isSuccess && rows.length === 0 && (
          <p className="empty-row muted">
            {q || kind || active
              ? "No case matches."
              : "No cases yet. /warn, /timeout, /ban and the others open them."}
          </p>
        )}
        {rows.length > 0 && (
          <ol className="audit-table" aria-label="Cases">
            {rows.map((c) => (
              <CaseRow
                key={c.number}
                guild={guild}
                entry={c}
                open={open === c.number}
                onToggle={() => setOpen(open === c.number ? null : c.number)}
              />
            ))}
          </ol>
        )}
        {cases.hasNextPage && (
          <div className="panel-foot">
            <PixelButton
              variant="secondary"
              disabled={cases.isFetchingNextPage}
              onClick={() => void cases.fetchNextPage()}
            >
              {cases.isFetchingNextPage ? "Loading…" : "Older cases"}
            </PixelButton>
          </div>
        )}
      </Panel>
    </div>
  );
}

function who(c: Case): string {
  return c.on_channel ? c.target_name : c.target_name || c.target_id;
}

function CaseRow({
  guild,
  entry,
  open,
  onToggle,
}: {
  guild: string;
  entry: Case;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <li className="audit-entry">
      <button type="button" className="audit-summary" aria-expanded={open} onClick={onToggle}>
        <span className={`case-number pixel-label ${tone(entry.kind)}`}>#{entry.number}</span>
        <span className="audit-text">
          <strong>{entry.label}</strong> · {who(entry)}
          {entry.duration && <span className="muted"> · {entry.duration}</span>}
          <span className="muted"> · by {entry.moderator_name}</span>
          {entry.reason && <span className="case-reason muted"> — {entry.reason}</span>}
        </span>
        <time className="audit-time muted" dateTime={entry.created_at} title={entry.created_at}>
          {timeAgo(entry.created_at)}
        </time>
      </button>
      {open && <CaseDetail guild={guild} number={entry.number} />}
    </li>
  );
}

function CaseDetail({ guild, number }: { guild: string; number: number }) {
  const detail = useCase(guild, number);
  if (detail.isPending) return <Loading label="Opening the case" />;
  if (detail.isError) return <Problem error={detail.error} />;
  const { case: c, history } = detail.data;
  return (
    <div className="case-detail">
      <table className="diff">
        <tbody>
          <tr>
            <th scope="row">{c.on_channel ? "Channel" : "Member"}</th>
            <td>
              {c.target_name} <span className="muted">({c.target_id})</span>
            </td>
          </tr>
          <tr>
            <th scope="row">Moderator</th>
            <td>
              {c.moderator_name} <span className="muted">({c.moderator_id})</span>
            </td>
          </tr>
          {c.details && (
            <tr>
              <th scope="row">What</th>
              <td>{c.details}</td>
            </tr>
          )}
          {c.duration && (
            <tr>
              <th scope="row">Duration</th>
              <td>
                {c.duration}
                {c.expires_at && (
                  <span className="muted"> · ends {new Date(c.expires_at).toLocaleString()}</span>
                )}
              </td>
            </tr>
          )}
          <tr>
            <th scope="row">Status</th>
            <td>{status(c)}</td>
          </tr>
          {c.dm_sent !== null && (
            <tr>
              <th scope="row">Direct message</th>
              <td>{c.dm_sent ? "Delivered" : "Not delivered"}</td>
            </tr>
          )}
          <tr>
            <th scope="row">Opened</th>
            <td>{new Date(c.created_at).toLocaleString()}</td>
          </tr>
        </tbody>
      </table>
      <ReasonEditor guild={guild} entry={c} />
      {history.length > 0 && (
        <div className="case-history">
          <p className="pixel-label muted">Their other cases</p>
          <ul className="list">
            {history.map((h) => (
              <li key={h.number} className="list-row">
                <span className={`case-number pixel-label ${tone(h.kind)}`}>#{h.number}</span>
                <span className="list-main">
                  {h.label}
                  {h.reason && <span className="muted"> — {h.reason}</span>}
                </span>
                <span className="muted list-note">{timeAgo(h.created_at)}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function status(c: Case): string {
  if (!["ban", "warn", "timeout"].includes(c.kind)) return "Done";
  if (!c.active) return c.kind === "warn" ? "Taken back" : "Ended";
  return c.kind === "warn" ? "Counts" : "Standing";
}

function ReasonEditor({ guild, entry }: { guild: string; entry: Case }) {
  const save = useSaveReason(guild);
  const [reason, setReason] = useState(entry.reason ?? "");
  const changed = reason.trim() !== (entry.reason ?? "");
  const error =
    save.error instanceof ApiError ? save.error.message : save.error ? "Could not save the reason." : null;
  return (
    <div className="reason-editor">
      <Field
        label="Reason"
        htmlFor={`reason-${entry.number}`}
        error={error}
        hint="Saving edits the case in the mod-log too."
        aside={<span className={`counter ${reason.length > 512 ? "is-over" : ""}`}>{reason.length}/512</span>}
      >
        <textarea
          id={`reason-${entry.number}`}
          className="pixel-input pixel-textarea"
          rows={2}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </Field>
      <PixelButton
        variant="secondary"
        disabled={!changed || save.isPending || reason.length > 512}
        onClick={() => save.mutate({ number: entry.number, reason: reason.trim() || null })}
      >
        {save.isPending ? "Saving…" : save.isSuccess && !changed ? "Saved" : "Save reason"}
      </PixelButton>
    </div>
  );
}
