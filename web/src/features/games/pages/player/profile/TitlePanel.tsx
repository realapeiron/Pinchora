import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { Picker } from "./Fields";

export function TitlePanel({ model }: { model: Pick<ProfileEditorModel, "data" | "draft" | "update"> }) {
  const { data, draft, update } = model;
  return (
    <Panel title={t("Your title")}>
      <Picker
        label={t("Title")}
        value={draft.title}
        options={data.titles.filter((v) => v.owned)}
        onChange={(v) => update("title", v)}
      />
      <ul className="player-unlocks">
        {data.titles
          .filter((v) => !v.owned)
          .map((v) => (
            <li key={v.id}>
              <strong>{v.name}</strong>
              <span className="muted">{v.description}</span>
            </li>
          ))}
      </ul>
    </Panel>
  );
}
