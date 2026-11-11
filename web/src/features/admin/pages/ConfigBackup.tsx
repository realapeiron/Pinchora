import { dateTime } from "../../../i18n";
import { useState } from "react";
import { useOutletContext } from "react-router";
import type { Overview } from "../../../lib/api";
import { words } from "../../moderation";
import { useBackup, type BackupData, type BackupDocument, type ImportRequest } from "../api/commandControl";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { backupSummary } from "./backupSummary";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/command-control.css";
export function ConfigBackup() {
  const { guild } = useOutletContext<Overview>();
  const { query } = useBackup(guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <BackupEditor key={guild.id} guild={guild.id} data={query.data} />;
}
function BackupEditor({ guild, data }: { guild: string; data: BackupData }) {
  const w = words(data.strings);
  const { preview, apply, point, exportUrl } = useBackup(guild);
  const [backup, setBackup] = useState<BackupDocument | null>(null);
  const [policy, setPolicy] = useState<ImportRequest["policy"]>("merge");
  const [fileError, setFileError] = useState<string | null>(null);
  const [filename, setFilename] = useState("");
  const pending = preview.isPending || apply.isPending || point.isPending;
  const summary = backupSummary(data, filename);
  const currentPreview = preview.variables?.backup === backup && preview.variables?.policy === policy;
  const ready = currentPreview && preview.data?.can_import && !preview.isPending;
  const clear = () => {
    preview.reset();
    apply.reset();
    setFileError(null);
  };
  return (
    <SettingsPage className="settings-page command-control-page" title={w("ui.title")} lead={w("ui.lead")}>
      <div className="settings-detail-list">
        <SettingsRow
          rowId="config-backup-export"
          title={w("ui.export")}
          description={dashboardT("Download this server's settings.")}
          summary={summary.export}
        >
          <p className="muted">{w("ui.export_hint")}</p>
          <a className="pixel-button pixel-button-primary" href={exportUrl} download>
            {w("ui.export")}
          </a>
        </SettingsRow>
        <SettingsRow
          rowId="config-backup-import"
          title={w("ui.import")}
          description={dashboardT("Preview and import a backup file.")}
          summary={summary.import}
          initialOpen
        >
          <div className="settings-detail-fields">
            <p className="muted">{w("ui.mapping_hint")}</p>
            <Field label={w("ui.file")} htmlFor="backup-file" error={fileError}>
              <input
                id="backup-file"
                className="pixel-input"
                type="file"
                accept=".json,application/json"
                disabled={pending}
                onChange={async (e) => {
                  const file = e.target.files?.[0];
                  clear();
                  setBackup(null);
                  setFilename(file?.name ?? "");
                  if (!file) return;
                  try {
                    if (file.size > 32 * 1024 * 1024) throw new Error();
                    const parsed: unknown = JSON.parse(await file.text());
                    if (
                      !parsed ||
                      typeof parsed !== "object" ||
                      !("version" in parsed) ||
                      !("sections" in parsed)
                    )
                      throw new Error();
                    setBackup(parsed as BackupDocument);
                  } catch {
                    setFileError(w("invalid"));
                  }
                }}
              />
            </Field>
            {filename && <p className="muted command-filename">{filename}</p>}
            <Field label={w("ui.policy")} htmlFor="backup-policy" hint={w("ui.merge_hint")}>
              <select
                id="backup-policy"
                className="pixel-input"
                value={policy}
                disabled={pending}
                onChange={(e) => {
                  clear();
                  setPolicy(e.target.value as ImportRequest["policy"]);
                }}
              >
                <option value="merge">{w("ui.merge")}</option>
                <option value="replace">{w("ui.replace")}</option>
              </select>
            </Field>
            <PixelButton
              disabled={!backup || pending}
              onClick={() => {
                if (backup) {
                  apply.reset();
                  preview.mutate({ backup, policy });
                }
              }}
            >
              {w(pending ? "ui.working" : "ui.preview")}
            </PixelButton>
            {(preview.error || apply.error || point.error) && (
              <p role="alert" className="field-error">
                {(preview.error ?? apply.error ?? point.error)?.message}
              </p>
            )}
            {apply.isSuccess && (
              <p role="status" className="notice">
                {apply.data.message}
              </p>
            )}
          </div>
          {preview.data && currentPreview && (
            <section className="backup-preview settings-detail-preview" aria-label={w("ui.diff")}>
              <h3>{w("ui.diff")}</h3>
              <p role="status" className="muted">
                {w("ui.ready")}
              </p>
              <div className="backup-table-scroll">
                <table className="backup-diff">
                  <thead>
                    <tr>
                      {["section", "added", "changed", "removed"].map((key) => (
                        <th key={key}>{w(`ui.${key}`)}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {preview.data.differences.map((d) => (
                      <tr key={d.section}>
                        <th scope="row">{w(`section.${d.section}`)}</th>
                        <td>{d.added}</td>
                        <td>{d.changed}</td>
                        <td>{d.removed}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {preview.data.unmapped.length > 0 && (
                <div role="alert">
                  <h4>{w("ui.unmapped")}</h4>
                  <ul>
                    {preview.data.unmapped.map((item) => (
                      <li key={item}>{item}</li>
                    ))}
                  </ul>
                </div>
              )}
              {preview.data.errors.length > 0 && (
                <div role="alert">
                  <h4>{w("ui.errors")}</h4>
                  <ul>
                    {preview.data.errors.map((item) => (
                      <li key={item}>{item}</li>
                    ))}
                  </ul>
                </div>
              )}
              <PixelButton
                disabled={!ready || pending || apply.isSuccess}
                onClick={() => {
                  if (backup) apply.mutate({ backup, policy });
                }}
              >
                {w("ui.apply")}
              </PixelButton>
            </section>
          )}
        </SettingsRow>
        <SettingsRow
          rowId="config-backup-restore-points"
          title={w("ui.restore_points")}
          description={dashboardT("Review saved restore points.")}
          summary={summary.points}
        >
          <p className="muted">{w("ui.restore_hint")}</p>
          {data.points.length === 0 && <p className="empty-row muted">{w("ui.no_points")}</p>}
          {data.points.map((p) => (
            <div className="backup-point" key={p.id}>
              <div>
                <strong>#{p.id}</strong>
                <time dateTime={p.created_at}>{dateTime(p.created_at)}</time>
              </div>
              <PixelButton
                variant="secondary"
                disabled={pending}
                onClick={() => {
                  clear();
                  setBackup(null);
                  point.mutate(p.id, {
                    onSuccess: (document) => {
                      setBackup(document);
                      setPolicy("replace");
                      setFilename(`#${p.id}`);
                      preview.mutate({ backup: document, policy: "replace" });
                    },
                  });
                }}
              >
                {w("ui.restore")}
              </PixelButton>
            </div>
          ))}
        </SettingsRow>
      </div>
    </SettingsPage>
  );
}
