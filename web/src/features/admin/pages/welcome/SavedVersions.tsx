import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { api } from "../../../../lib/api";
import { type CardView, history } from "../../../../lib/card";
import { studioError } from "../../api/messageStudio";
import { sideOf } from "./welcomeDesign";

export function SavedVersions({
  revisions,
  guild,
  kind,
  saved,
  setSaved,
  setHist,
  refreshRevisions,
  setToast,
  setErrors,
}: Pick<
  WelcomeModel,
  | "revisions"
  | "guild"
  | "kind"
  | "saved"
  | "setSaved"
  | "setHist"
  | "refreshRevisions"
  | "setToast"
  | "setErrors"
>) {
  return (
    <Panel title={deferredText("SAVED VERSIONS")}>
      <div className="card-test">
        {revisions.map((revision) => (
          <div key={revision.id} className="card-revision">
            <span>{new Date(revision.created_at).toLocaleString()}</span>
            <PixelButton
              variant="secondary"
              className="button-small"
              onClick={async () => {
                try {
                  const view = await api<CardView>(
                    `/api/guilds/${guild}/cards/${kind}/history/${revision.id}/rollback`,
                    "POST",
                  );
                  const next = { ...saved, [kind]: sideOf(view) };
                  setSaved(next);
                  setHist(history(next));
                  await refreshRevisions(kind);
                  setToast(deferredText("Version restored."));
                } catch (error) {
                  setErrors([studioError(error, deferredText("Restore failed."))]);
                }
              }}
            >
              {spaTranslate("Restore")}
            </PixelButton>
          </div>
        ))}
        {revisions.length === 0 && <p className="muted">{deferredText("No saved versions yet.")}</p>}
      </div>
    </Panel>
  );
}
