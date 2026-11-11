import { Field } from "../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { SettingsRow } from "../../../components/SettingsRow";
import { SafetyFlagRow } from "./SafetyFlagRow";
import { dashboardText, type safetySummary } from "./safetySummary";
import type { GuildInfo } from "../../../lib/api";
import type { Gate, Raid, words } from "../api/verification";

type Text = ReturnType<typeof words>;
type Change = (section: "verification" | "raid", key: string, value: unknown) => void;
export function SafetyChecks({
  v,
  r,
  t,
  info,
  summary,
  onChange,
}: {
  v: Gate;
  r: Raid;
  t: Text;
  info: GuildInfo | undefined;
  summary: ReturnType<typeof safetySummary>;
  onChange: Change;
}) {
  const number = (
    section: "verification" | "raid",
    key: string,
    label: string,
    value: number,
    min: number,
    max: number,
  ) => (
    <Field label={t(label)} htmlFor={`safety-${key}`}>
      <input
        className="pixel-input"
        id={`safety-${key}`}
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(section, key as never, Number(e.target.value))}
      />
    </Field>
  );
  const select = (
    section: "verification" | "raid",
    key: string,
    label: string,
    value: string,
    options: string[],
  ) => (
    <Field label={t(label)} htmlFor={`safety-${key}`}>
      <select
        className="pixel-input pixel-select"
        id={`safety-${key}`}
        value={value}
        onChange={(e) => onChange(section, key as never, e.target.value)}
      >
        {options.map((o) => (
          <option key={o} value={o}>
            {t(o)}
          </option>
        ))}
      </select>
    </Field>
  );
  return (
    <div className="safety-main">
      <SettingsRow
        title={t("gate")}
        description={t("gate_hint")}
        summary={summary.gate}
        enabled={v.enabled}
        onToggle={(next) => onChange("verification", "enabled", next)}
        toggleLabel={t("enabled")}
        initialOpen
      >
        <div className="settings-detail-fields">
          <p className="muted">{t("gate_hint")}</p>
          {select("verification", "mode", "mode", v.mode, ["button", "captcha", "web"])}
          <div className="safety-fields">
            {(["verified_role_id", "unverified_role_id"] as const).map((key) => (
              <Field
                key={key}
                label={t(key === "verified_role_id" ? "verified_role" : "unverified_role")}
                htmlFor={`safety-${key}`}
              >
                <RolePicker
                  id={`safety-${key}`}
                  info={info}
                  value={v[key]}
                  none="—"
                  onChange={(id) => onChange("verification", key, id)}
                />
              </Field>
            ))}
          </div>
          <p className="field-hint muted">{t("onboarding")}</p>
          <div className="safety-fields">
            <Field label={t("channel")} htmlFor="safety-channel">
              <ChannelPicker
                id="safety-channel"
                info={info}
                value={v.channel_id}
                none="—"
                onChange={(id) => onChange("verification", "channel_id", id)}
              />
            </Field>
            {number("verification", "kick_after_mins", "timeout", v.kick_after_mins, 0, 10080)}
            {number("verification", "min_account_age_hours", "age", v.min_account_age_hours, 0, 8760)}
            {select("verification", "rule_action", "rule_action", v.rule_action, ["hold", "notify", "kick"])}
          </div>
        </div>
      </SettingsRow>
      <SafetyFlagRow
        title={t("avatar")}
        description={dashboardText("Block members with default avatars.")}
        summary={summary.avatar}
        enabled={v.block_default_avatar}
        onToggle={(next) => onChange("verification", "block_default_avatar", next)}
      />
      <SafetyFlagRow
        title={t("names")}
        description={dashboardText("Block names that match suspicious patterns.")}
        summary={summary.names}
        enabled={v.block_name_patterns}
        onToggle={(next) => onChange("verification", "block_name_patterns", next)}
      />
      <SafetyFlagRow
        title={t("dm")}
        description={dashboardText("Message blocked members.")}
        summary={summary.dm}
        enabled={v.dm_on_block}
        onToggle={(next) => onChange("verification", "dm_on_block", next)}
      />
      <SettingsRow
        title={t("raid")}
        description={t("raid_hint")}
        summary={summary.raid}
        enabled={r.enabled}
        onToggle={(next) => onChange("raid", "enabled", next)}
        toggleLabel={t("enabled")}
      >
        <div className="settings-detail-fields">
          <p className="muted">{t("raid_hint")}</p>
          <div className="safety-fields">
            {number("raid", "join_threshold", "threshold", r.join_threshold, 2, 500)}
            {number("raid", "window_secs", "window", r.window_secs, 5, 600)}
            {number("raid", "similar_threshold", "similar", r.similar_threshold, 2, 500)}
            {number("raid", "new_account_hours", "young", r.new_account_hours, 0, 8760)}
          </div>
          <div className="safety-fields">
            {select("raid", "action", "action", r.action, ["alert", "lockdown", "kick", "ban"])}
            {number("raid", "lockdown_minutes", "duration", r.lockdown_minutes, 1, 1440)}
          </div>
          <Field
            label={t("channels")}
            htmlFor="safety-channels"
            aside={t("used", { count: r.channels.length, max: 25 })}
          >
            <select
              id="safety-channels"
              className="pixel-input safety-multi"
              multiple
              value={r.channels}
              onChange={(e) =>
                onChange(
                  "raid",
                  "channels",
                  Array.from(e.target.selectedOptions, (o) => o.value),
                )
              }
            >
              {info?.channels
                .filter((ch) => [0, 5].includes(ch.type))
                .map((ch) => (
                  <option key={ch.id} value={ch.id}>
                    #{ch.name}
                  </option>
                ))}
            </select>
          </Field>
          {number("raid", "slowmode_secs", "slowmode", r.slowmode_secs, 0, 21600)}
        </div>
      </SettingsRow>
      <SafetyFlagRow
        title={t("similar_names")}
        description={dashboardText("Detect similar names in a join spike.")}
        summary={summary.similarNames}
        enabled={r.similar_names}
        onToggle={(next) => onChange("raid", "similar_names", next)}
      />
      <SafetyFlagRow
        title={t("similar_avatars")}
        description={dashboardText("Detect similar avatars in a join spike.")}
        summary={summary.similarAvatars}
        enabled={r.similar_avatars}
        onToggle={(next) => onChange("raid", "similar_avatars", next)}
      />
      <SafetyFlagRow
        title={t("lock")}
        description={dashboardText("Lock selected channels during a raid.")}
        summary={summary.lock}
        enabled={r.lock_channels}
        onToggle={(next) => onChange("raid", "lock_channels", next)}
      />
      <SafetyFlagRow
        title={t("invites")}
        description={dashboardText("Pause invites during a raid.")}
        summary={summary.invites}
        enabled={r.pause_invites}
        onToggle={(next) => onChange("raid", "pause_invites", next)}
      />
    </div>
  );
}
