import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./antiNukeSummary";
import {
  draftProblem,
  type AntiNukeConfig,
  type AntiNukeData,
  type useAntiNuke,
  type words,
} from "../api/anti_nuke";

type Text = ReturnType<typeof words>;
type Actions = ReturnType<typeof useAntiNuke>;
export function AntiNukeLimits({
  saved,
  config,
  t,
  summary,
  locked,
  onLimits,
}: {
  saved: AntiNukeData;
  config: AntiNukeConfig;
  t: Text;
  summary: string;
  locked: boolean;
  onLimits: (value: AntiNukeConfig["limits"]) => void;
}) {
  return (
    <SettingsRow
      rowId="anti-nuke-rows-limits"
      title={t("ui.limits")}
      description={dashboardText("Set action counts and time windows.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="muted antinuke-guidance">{t("ui.limit_hint")}</p>
        <div className="antinuke-limits">
          {saved.kinds
            .filter((k) => !k.startsWith("vip_"))
            .map((k) => {
              const limit = config.limits[k]!;
              return (
                <div className="antinuke-limit" key={k}>
                  <h3>{t(`kind.${k}`)}</h3>
                  <Field label={t("ui.count")} htmlFor={`an-count-${k}`}>
                    <div className="antinuke-slider">
                      <input
                        id={`an-count-${k}`}
                        aria-label={`${t(`kind.${k}`)} · ${t("ui.count")}`}
                        type="range"
                        min="1"
                        max="100"
                        value={limit.count}
                        disabled={locked}
                        onChange={(e) =>
                          onLimits({
                            ...config.limits,
                            [k]: { ...limit, count: Number(e.target.value) },
                          })
                        }
                      />
                      <output htmlFor={`an-count-${k}`}>{limit.count}</output>
                    </div>
                  </Field>
                  <Field label={t("ui.seconds")} htmlFor={`an-window-${k}`}>
                    <input
                      id={`an-window-${k}`}
                      aria-label={`${t(`kind.${k}`)} · ${t("ui.seconds")}`}
                      className="pixel-input"
                      type="number"
                      min="5"
                      max="300"
                      value={limit.window_secs}
                      disabled={locked}
                      onChange={(e) =>
                        onLimits({
                          ...config.limits,
                          [k]: { ...limit, window_secs: Number(e.target.value) },
                        })
                      }
                    />
                  </Field>
                </div>
              );
            })}
        </div>
      </div>
    </SettingsRow>
  );
}

export function AntiNukePreview({
  saved,
  config,
  t,
  summary,
  preview,
  kind,
  setKind,
  count,
  setCount,
}: {
  saved: AntiNukeData;
  config: AntiNukeConfig;
  t: Text;
  summary: string;
  preview: Actions["preview"];
  kind: string;
  setKind: (value: string) => void;
  count: number;
  setCount: (value: number) => void;
}) {
  return (
    <SettingsRow
      rowId="anti-nuke-rows-preview"
      title={t("ui.preview")}
      description={t("ui.preview_hint")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="muted">{t("ui.preview_hint")}</p>
        <div className="antinuke-preview">
          <Field label={t("ui.kind")} htmlFor="an-preview-kind">
            <select
              id="an-preview-kind"
              className="pixel-input pixel-select"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              {saved.kinds.map((k) => (
                <option key={k} value={k}>
                  {t(`kind.${k}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("ui.preview_count")} htmlFor="an-preview-count">
            <input
              id="an-preview-count"
              className="pixel-input"
              type="number"
              min="1"
              max="1000"
              value={count}
              onChange={(e) => setCount(Number(e.target.value))}
            />
          </Field>
          <PixelButton
            variant="secondary"
            disabled={preview.isPending || draftProblem(config) || count < 1 || count > 1000}
            onClick={() => preview.mutate({ config, kind, count })}
          >
            {t("ui.run_preview")}
          </PixelButton>
        </div>
      </div>
      <div className="settings-detail-preview">
        {!preview.data && !preview.isError && <p className="muted">{summary}</p>}
        {preview.data && (
          <p className="notice" role="status">
            {preview.data.message}
          </p>
        )}
        {preview.isError && (
          <p className="field-error" role="alert">
            {preview.error.message}
          </p>
        )}
      </div>
    </SettingsRow>
  );
}
