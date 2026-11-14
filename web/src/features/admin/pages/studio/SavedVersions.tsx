import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";

export function SavedVersions({
  revisions,
  busy,
  restore,
}: Pick<StudioModel, "revisions" | "busy" | "restore">) {
  return (
    <Panel title={deferredText("SAVED VERSIONS")}>
      <div className="studio-actions">
        {revisions.map((revision) => (
          <div className="studio-revision" key={revision.version}>
            <span>
              v{revision.version} · {new Date(revision.created_at).toLocaleString()}
            </span>
            <PixelButton variant="secondary" disabled={busy} onClick={() => void restore(revision)}>
              {t("Restore")}
            </PixelButton>
          </div>
        ))}
        {revisions.length === 0 && <p className="muted">{deferredText("No saved versions yet.")}</p>}
      </div>
    </Panel>
  );
}
