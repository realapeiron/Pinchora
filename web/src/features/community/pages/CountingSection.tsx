import { translate as t } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ChannelPicker } from "../../../components/Pickers";
import { useEngagementContext } from "./engagementContext";
import { countingSettings } from "../api/communityEngagement";

export function CountingSection() {
  const { data, info, counting, setCounting, busy, write, summary } = useEngagementContext();
  return (
    <SettingsRow
      rowId="counting-section-counting"
      title={dt("row.Counting", {}, "COUNTING")}
      description={dt("Choose the counting channel and its rules.")}
      summary={summary.counting ?? dt("Off")}
    >
      <p className="panel-lead muted">
        {t(
          "One member posts the next number. A second turn by the same member fails. Simple arithmetic is optional.",
        )}
      </p>
      <p className="community-pack-metric">
        {counting.current}
        <small>{t("Current count")}</small>
      </p>
      <p className="muted">
        {t("Record")}: {counting.record}
      </p>
      <label>
        {t("Counting channel")}
        <ChannelPicker
          info={info}
          value={counting.channel_id || null}
          onChange={(value) => setCounting({ ...counting, channel_id: value ?? "" })}
          none={t("Choose a channel")}
        />
      </label>
      <label className="community-pack-check">
        <input
          type="checkbox"
          checked={counting.reset_on_fail}
          onChange={(e) => setCounting({ ...counting, reset_on_fail: e.target.checked })}
        />
        {t("Reset on a mistake")}
      </label>
      <label className="community-pack-check">
        <input
          type="checkbox"
          checked={counting.allow_math}
          onChange={(e) => setCounting({ ...counting, allow_math: e.target.checked })}
        />
        {t("Allow small arithmetic")}
      </label>
      <label>
        {t("Failure role")}
        <select
          className="pixel-input pixel-select"
          value={counting.fail_role_id ?? ""}
          onChange={(e) => setCounting({ ...counting, fail_role_id: e.target.value || null })}
        >
          <option value="">{t("No role")}</option>
          {info?.roles
            .filter((r) => r.action_ok && r.assignable)
            .map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
        </select>
      </label>
      <div className="community-pack-actions">
        <button
          className="pixel-button"
          disabled={busy || !counting.channel_id}
          onClick={() => void write("/counting", "PUT", countingSettings(counting))}
        >
          {t("Save counting")}
        </button>
        <button disabled={busy || !data.counting} onClick={() => void write("/counting/reset", "POST")}>
          {t("Reset count")}
        </button>
      </div>
    </SettingsRow>
  );
}
