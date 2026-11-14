import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import { changeCharacterColor, type CharacterData } from "../../../api/webPlayer";
import { Field } from "../ProfileStudio";

export function LookPanel({
  catalog,
  draft,
  edit,
}: {
  catalog: Pick<CharacterData["catalog"], "looks" | "channels" | "presets">;
  draft: string;
  edit: (next: string) => void;
}) {
  const [base = ""] = draft.split("+");
  const parts = base.split(".");
  return (
    <Panel title={t("Look & colours")}>
      <Field label={t("Look")}>
        <select value={parts[0]} onChange={(e) => edit(changeCharacterColor(draft, "look", e.target.value))}>
          {catalog.looks.map((look) => (
            <option key={look.id} value={look.id}>
              {look.name}
            </option>
          ))}
        </select>
      </Field>
      {catalog.channels.map((channel) => {
        const index = ["look", "skin", "hair", "top", "bottom", "eyes", "shoes"].indexOf(channel.id);
        const color = parts[index] ?? "0";
        return (
          <fieldset className="player-colour" key={channel.id}>
            <legend>{channel.name}</legend>
            <div className="player-swatches">
              {channel.swatches.map((sw) => (
                <button
                  type="button"
                  key={sw.id}
                  aria-label={t("{channel} colour {number}", {
                    channel: channel.name,
                    number: sw.id + 1,
                  })}
                  aria-pressed={color === String(sw.id)}
                  onClick={() => edit(changeCharacterColor(draft, channel.id, String(sw.id)))}
                >
                  <svg viewBox="0 0 32 32" aria-hidden="true">
                    <rect width="32" height="32" fill={sw.value} />
                  </svg>
                </button>
              ))}
            </div>
            <Field label={t("Custom {channel} colour", { channel: channel.name })}>
              <input
                maxLength={7}
                placeholder={t("Hex colour (#rrggbb)")}
                value={color.startsWith("#") ? color : ""}
                onChange={(e) => edit(changeCharacterColor(draft, channel.id, e.target.value || "0"))}
              />
            </Field>
          </fieldset>
        );
      })}
      <Field label={t("Start from a preset")}>
        <select
          value=""
          onChange={(e) => {
            const preset = catalog.presets.find((p) => p.id === e.target.value);
            if (preset) edit(preset.spec);
          }}
        >
          <option value="">{t("Choose a preset")}</option>
          {catalog.presets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </Field>
    </Panel>
  );
}
