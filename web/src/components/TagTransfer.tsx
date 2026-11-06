import { useRef, useState } from "react";

import { ApiError, type ImportReport } from "../lib/api";
import { type TagKind, exportTags, useImportTags } from "../lib/queries";
import { exportFileName } from "../lib/tags";
import { Panel } from "./Panel";
import { PixelButton } from "./PixelButton";

/** Export to a file and import from one: the way to move commands between servers, or to back them up. */
export function TagTransfer({ guild, kind }: { guild: string; kind: TagKind }) {
  const input = useRef<HTMLInputElement>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [replace, setReplace] = useState(false);
  const importing = useImportTags(guild, kind);
  const noun = kind === "commands" ? "commands" : "auto-responders";

  const download = async () => {
    setProblem(null);
    try {
      const file = await exportTags(guild, kind);
      const blob = new Blob([JSON.stringify(file, null, 2) + "\n"], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = exportFileName(kind, guild);
      link.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      setProblem(error instanceof ApiError ? error.message : "The export did not work. Try again.");
    }
  };

  const upload = async (file: File | undefined) => {
    setProblem(null);
    setReport(null);
    if (!file) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(await file.text());
    } catch {
      setProblem("That file isn't JSON. Choose a file you exported from Pinchora.");
      return;
    }
    const body = parsed as { version?: unknown; items?: unknown };
    if (!body || typeof body !== "object" || !Array.isArray(body.items)) {
      setProblem("That file doesn't hold a list of " + noun + ". Choose a file you exported from Pinchora.");
      return;
    }
    importing.mutate(
      {
        ...(typeof body.version === "number" ? { version: body.version } : {}),
        items: body.items,
        ...(kind === "commands" ? { on_conflict: replace ? ("replace" as const) : ("skip" as const) } : {}),
      },
      {
        onSuccess: setReport,
        onError: (error) => setProblem(error.message),
      },
    );
  };

  return (
    <Panel title="IMPORT AND EXPORT">
      <p className="panel-lead muted">
        Export your {noun} to a file, and import a file into this or another server. Roles and channels the
        other server lacks are left out.
      </p>
      <div className="transfer-row">
        <PixelButton variant="secondary" onClick={() => void download()}>
          Export to a file
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => input.current?.click()}
          disabled={importing.isPending}
        >
          {importing.isPending ? "Importing…" : "Import from a file"}
        </PixelButton>
        {kind === "commands" && (
          <label className="check">
            <input type="checkbox" checked={replace} onChange={(event) => setReplace(event.target.checked)} />
            Replace commands that have the same name
          </label>
        )}
        <input
          ref={input}
          type="file"
          accept="application/json,.json"
          hidden
          aria-label={`Import ${noun} file`}
          onChange={(event) => {
            void upload(event.target.files?.[0]);
            event.target.value = "";
          }}
        />
      </div>
      {problem && (
        <p className="field-error" role="alert">
          {problem}
        </p>
      )}
      {report && <ImportResult report={report} />}
    </Panel>
  );
}

function ImportResult({ report }: { report: ImportReport }) {
  const parts = [
    report.created.length > 0 && `${report.created.length} added`,
    report.replaced.length > 0 && `${report.replaced.length} replaced`,
    report.skipped.length > 0 && `${report.skipped.length} skipped (already there)`,
    report.failed.length > 0 && `${report.failed.length} refused`,
  ].filter(Boolean);
  return (
    <div className="import-result" role="status">
      <p>
        <strong>{parts.length > 0 ? parts.join(", ") : "Nothing to import"}.</strong>
      </p>
      <ul className="preview-notes">
        {[...report.created, ...report.replaced].map((line) => (
          <li key={line}>{line}</li>
        ))}
        {report.failed.map((f) => (
          <li key={f.index} className="import-failed">
            {f.name ? `${f.name}: ` : `Entry ${f.index + 1}: `}
            {f.message}
          </li>
        ))}
      </ul>
    </div>
  );
}
