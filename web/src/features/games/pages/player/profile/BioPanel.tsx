import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { Field } from "./Fields";

export function BioPanel({ model }: { model: Pick<ProfileEditorModel, "draft" | "update"> }) {
  const { draft, update } = model;
  return (
    <Panel title={t("About you")}>
      <Field label={t("Bio")}>
        <textarea rows={6} value={draft.bio} onChange={(e) => update("bio", e.target.value)} />
      </Field>
      <p className="muted">{[...draft.bio].length}/300</p>
    </Panel>
  );
}
