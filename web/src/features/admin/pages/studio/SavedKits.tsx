import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";

export function SavedKits({
  kits,
  kitKey,
  choose,
  fresh,
  importJson,
  exportJson,
}: Pick<StudioModel, "kits" | "choose" | "fresh" | "importJson" | "exportJson"> & {
  kitKey: StudioModel["key"];
}) {
  const key = kitKey;
  return (
    <Panel title={deferredText("SAVED KITS")} aside={<span className="counter">{kits.length}/50</span>}>
      <div className="studio-actions">
        <PixelButton variant="secondary" onClick={fresh}>
          {deferredText("New kit")}
        </PixelButton>
        <select
          className="pixel-input pixel-select"
          aria-label={t("Saved kit")}
          value={key}
          onChange={(event) => {
            const item = kits.find((kit) => kit.key === event.target.value);
            if (item) choose(item);
            else fresh();
          }}
        >
          <option value="">{deferredText("Choose a saved kit")}</option>
          {kits.map((kit) => (
            <option key={kit.key} value={kit.key}>
              {kit.name}
            </option>
          ))}
        </select>
        <label className="button pixel-button">
          {deferredText("Import JSON")}
          <input
            type="file"
            accept="application/json,.json"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) void importJson(file);
              event.target.value = "";
            }}
          />
        </label>
        <PixelButton variant="ghost" onClick={exportJson}>
          {deferredText("Export JSON")}
        </PixelButton>
      </div>
    </Panel>
  );
}
