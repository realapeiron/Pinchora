import { dateTime } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./automodSummary";
import type { AutoConfig, Words, useAutoMod } from "../api/automod";

type ProfileMutation = ReturnType<typeof useAutoMod>["profile"];
export function AutoModProfile({
  config,
  t,
  profile,
  pending,
  observe,
  summary,
}: {
  config: AutoConfig;
  t: Words;
  profile: ProfileMutation;
  pending: boolean;
  observe: boolean;
  summary: string;
}) {
  return (
    <SettingsRow
      title={t("ui.profiles")}
      description={dashboardText("Choose a protection profile.")}
      summary={summary}
      initialOpen
    >
      <div className="settings-detail-fields">
        <div className="automod-profiles">
          {["relaxed", "standard", "strict", "off"].map((p) => (
            <button
              key={p}
              className="automod-profile"
              type="button"
              aria-pressed={config.profile === p}
              disabled={pending}
              onClick={() => profile.mutate(p)}
            >
              <span className="pixel-label">{t(`ui.${p}`)}</span>
              <span>{t(`ui.${p}_hint`)}</span>
            </button>
          ))}
        </div>
        <p className="automod-observation">{t("ui.observe")}</p>
        {observe && (
          <p className="automod-until">
            {t("ui.observing_until", { time: dateTime(config.observe_until!) })}
          </p>
        )}
        {profile.isError && (
          <p role="alert" className="field-error">
            {profile.error.message}
          </p>
        )}
      </div>
    </SettingsRow>
  );
}
