import { useState } from "react";
import { useOutletContext } from "react-router";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { translate as t } from "../../../i18n";
import type { AuditEntry, Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { EMPTY_FILTERS, auditDiff, auditParams, diffText, useFilteredAudit } from "../api/server-tools";
import { type Names, describeChange, targetLabel, timeAgo, plainNames } from "../../../lib/text";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/server-tools.css";

export function AuditLog() {
  const data = useOutletContext<Overview>();
  const [draft, setDraft] = useState(EMPTY_FILTERS);
  const [filters, setFilters] = useState(EMPTY_FILTERS);
  const audit = useFilteredAudit(data.guild.id, filters);
  const discord = useDiscord(data.guild.id).data;
  const names: Names = {
    channel: (id) => {
      const channel = discord?.channels.find((c) => c.id === id);
      return channel ? `#${channel.name}` : plainNames.channel(id);
    },
    role: (id) => {
      const role = discord?.roles.find((r) => r.id === id);
      return role ? `@${role.name}` : plainNames.role(id);
    },
    module: (key) => data.modules?.find((m) => m.key === key)?.label ?? plainNames.module(key),
  };
  const rows = audit.data?.pages.flatMap((p) => p.items) ?? [];
  const invalid = !!draft.from && !!draft.until && draft.from > draft.until;
  return (
    <SettingsPage
      title={t("AUDIT LOG")}
      lead={t("Every change made on the dashboard: who, what, and when. Kept 180 days.")}
    >
      <Panel title={t("FILTER CHANGES")}>
        <form
          className="audit-filters"
          onSubmit={(e) => {
            e.preventDefault();
            if (!invalid) setFilters({ ...draft });
          }}
        >
          <Field label={t("Actor ID")} htmlFor="audit-actor">
            <input
              className="pixel-input"
              id="audit-actor"
              inputMode="numeric"
              pattern="[0-9]*"
              value={draft.actor}
              onChange={(e) => setDraft({ ...draft, actor: e.target.value })}
            />
          </Field>
          <Field
            label={t("Area")}
            htmlFor="audit-area"
            hint={t("Action prefix, for example roles, settings or access.")}
          >
            <input
              className="pixel-input"
              id="audit-area"
              maxLength={64}
              pattern="[a-zA-Z0-9_-]*"
              value={draft.area}
              onChange={(e) => setDraft({ ...draft, area: e.target.value })}
            />
          </Field>
          <Field label={t("From date (UTC)")} htmlFor="audit-from">
            <input
              className="pixel-input"
              type="date"
              max="9999-12-31"
              id="audit-from"
              value={draft.from}
              onChange={(e) => setDraft({ ...draft, from: e.target.value })}
            />
          </Field>
          <Field label={t("Through date (UTC)")} htmlFor="audit-until">
            <input
              className="pixel-input"
              type="date"
              max="9999-12-31"
              id="audit-until"
              min={draft.from}
              value={draft.until}
              onChange={(e) => setDraft({ ...draft, until: e.target.value })}
            />
          </Field>
          <Field
            label={t("Search changes")}
            htmlFor="audit-text"
            hint={t("Search actors, actions, targets and before/after values.")}
          >
            <input
              className="pixel-input"
              type="search"
              id="audit-text"
              maxLength={200}
              value={draft.text}
              onChange={(e) => setDraft({ ...draft, text: e.target.value })}
            />
          </Field>
          <div className="audit-filter-actions">
            <PixelButton type="submit" disabled={invalid}>
              {t("Apply filters")}
            </PixelButton>
            <PixelButton
              variant="ghost"
              onClick={() => {
                setDraft(EMPTY_FILTERS);
                setFilters(EMPTY_FILTERS);
              }}
            >
              {t("Clear filters")}
            </PixelButton>
            <a
              className="pixel-button pixel-button-secondary"
              href={`/api/guilds/${data.guild.id}/audit-tools/export?${auditParams(filters)}`}
              download
            >
              {t("Export CSV")}
            </a>
          </div>
        </form>
        <p className="muted fine">
          {t(
            "CSV uses applied filters. Limited to 10,000 changes, 100,000 inspected rows, 8 MiB or 10 seconds; a final row marks a reached limit.",
          )}
        </p>
      </Panel>
      <Panel title={t("CHANGES")}>
        {audit.isPending && <Loading label={t("Reading the log")} />}
        {audit.isError && <Problem error={audit.error} />}
        {audit.isSuccess && rows.length === 0 && (
          <p className="empty-row muted">
            {audit.hasNextPage
              ? t("No matches in this batch. Load older changes to continue searching.")
              : Object.values(filters).some(Boolean)
                ? t("No changes match these filters.")
                : t("No changes yet. What you save on the dashboard shows up here.")}
          </p>
        )}
        {rows.length > 0 && (
          <ol className="audit-table" aria-label={t("Changes")}>
            {rows.map((entry) => (
              <AuditRow key={entry.id} entry={entry} names={names} />
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
              {audit.isFetchingNextPage ? t("Loading…") : t("Older changes")}
            </PixelButton>
          </div>
        )}
      </Panel>
    </SettingsPage>
  );
}
function AuditRow({ entry, names }: { entry: AuditEntry; names: Names }) {
  const [open, setOpen] = useState(false);
  const target = targetLabel(entry, names);
  const diff = auditDiff(entry.before, entry.after);
  return (
    <li className="audit-entry">
      <button
        type="button"
        className="audit-summary"
        aria-expanded={open}
        aria-controls={`diff-${entry.id}`}
        onClick={() => setOpen(!open)}
      >
        <span className="audit-dot" aria-hidden="true" />
        <span className="audit-text">
          <strong>{entry.actor_name}</strong> {describeChange(entry, names)}
          {target && <span className="muted"> · {target}</span>}
          <span className="muted"> · {entry.action}</span>
          {entry.source === "discord" && <span className="muted"> {t("in Discord")}</span>}
        </span>
        <time className="audit-time muted" dateTime={entry.created_at} title={entry.created_at}>
          {timeAgo(entry.created_at)}
        </time>
      </button>
      {open && (
        <dl id={`diff-${entry.id}`} className="audit-diff">
          {diff.map((line) => (
            <div className="audit-diff-row" key={line.path}>
              <dt>{line.path}</dt>
              <dd>
                <div>
                  <span className="pixel-label muted">{t("Before")}</span>
                  <pre className="diff-before">{diffText(line.before, line.path, names)}</pre>
                </div>
                <div>
                  <span className="pixel-label muted">{t("After")}</span>
                  <pre className="diff-after">{diffText(line.after, line.path, names)}</pre>
                </div>
              </dd>
            </div>
          ))}
        </dl>
      )}
    </li>
  );
}
