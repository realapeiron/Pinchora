import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { Picker } from "./Fields";

export function ShowcasePanel({ model }: { model: Pick<ProfileEditorModel, "data" | "draft" | "update"> }) {
  const { data, draft, update } = model;
  return (
    <Panel title={t("Showcase your favourites")}>
      <Picker
        label={t("Fish")}
        value={draft.showcase_fish}
        options={data.showcase.fish}
        onChange={(v) => update("showcase_fish", v)}
      />
      <Picker
        label={t("Pet")}
        value={draft.showcase_pet}
        options={data.showcase.pets}
        onChange={(v) => update("showcase_pet", v)}
      />
      <Picker
        label={t("Furniture")}
        value={draft.showcase_item}
        options={data.showcase.items}
        onChange={(v) => update("showcase_item", v)}
      />
      <label className="player-check">
        <input
          type="checkbox"
          checked={draft.showcase_house}
          disabled={!data.showcase.house}
          onChange={(e) => update("showcase_house", e.target.checked)}
        />
        {t("Show my house")}
      </label>
      {!data.showcase.house && <p className="muted">{t("Open /house in Discord to start your home.")}</p>}
    </Panel>
  );
}
