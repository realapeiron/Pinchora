import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { PixelButton } from "../../../../../components/PixelButton";
import { Field } from "./Fields";

export function StylePanel({
  model,
}: {
  model: Pick<ProfileEditorModel, "data" | "draft" | "update" | "setDraft">;
}) {
  const { data, draft, update, setDraft } = model;
  return (
    <Panel title={t("Card style")}>
      <Field label={t("Accent colour")}>
        <input
          value={draft.accent ?? ""}
          placeholder={t("Background default")}
          maxLength={7}
          onChange={(e) => update("accent", e.target.value || null)}
        />
      </Field>
      <div className="player-swatches">
        {data.accents.map((a) => (
          <button
            type="button"
            key={a.value}
            title={a.name}
            aria-label={a.name}
            aria-pressed={draft.accent === a.value}
            onClick={() => update("accent", a.value)}
          >
            <svg viewBox="0 0 32 32" aria-hidden="true">
              <rect width="32" height="32" fill={a.value} />
            </svg>
          </button>
        ))}
      </div>
      <Field label={t("Panel opacity")}>
        <select value={draft.panel_opacity} onChange={(e) => update("panel_opacity", Number(e.target.value))}>
          {data.opacities.map((o) => (
            <option key={o.id} value={o.id}>
              {o.name}
            </option>
          ))}
        </select>
      </Field>
      <Field label={t("Frame")}>
        <select value={draft.frame} onChange={(e) => update("frame", e.target.value)}>
          {data.frames.map((f) => (
            <option key={f.id} value={f.id} disabled={data.level < f.level}>
              {f.name}
              {data.level < f.level ? ` · ${t("Level {level}", { level: f.level })}` : ""}
            </option>
          ))}
        </select>
      </Field>
      <PixelButton
        variant="secondary"
        onClick={() => setDraft((p) => ({ ...p, accent: null, panel_opacity: 1, frame: "plain" }))}
      >
        {t("Reset style")}
      </PixelButton>
    </Panel>
  );
}
