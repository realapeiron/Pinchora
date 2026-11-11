import type { Dispatch, SetStateAction } from "react";
import { dateTime } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./automodSummary";
import { ActionEditor } from "./AutoModControls";
import type { GuildInfo } from "../../../lib/api";
import type { AutoConfig, AutoData, Words } from "../api/automod";

export function AutoModSettings({
  config,
  saved,
  t,
  info,
  setConfig,
  summary,
}: {
  config: AutoConfig;
  saved: AutoData;
  t: Words;
  info: GuildInfo | undefined;
  setConfig: Dispatch<SetStateAction<AutoConfig>>;
  summary: string;
}) {
  return (
    <SettingsRow
      title={t("ui.settings")}
      description={dashboardText("Set alerts and escalation.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <Field label={t("ui.alert_channel")} htmlFor="am-alert">
          <ChannelPicker
            id="am-alert"
            info={info}
            value={config.alert_channel_id}
            onChange={(id) => setConfig((c) => ({ ...c, alert_channel_id: id }))}
            none={t("ui.mod_channel")}
          />
        </Field>
        <details className="automod-details">
          <summary>{t("ui.escalation")}</summary>
          <p className="muted">{t("ui.escalation_hint")}</p>
          <Field label={t("ui.window")} htmlFor="am-escalation-window">
            <input
              id="am-escalation-window"
              type="number"
              min="1"
              max="2592000"
              className="pixel-input"
              value={config.escalation[0]?.window_secs ?? 3600}
              onChange={(e) =>
                setConfig((c) => ({
                  ...c,
                  escalation: c.escalation.map((step) => ({
                    ...step,
                    window_secs: Number(e.target.value),
                  })),
                }))
              }
            />
          </Field>
          {config.escalation.map((step, i) => (
            <div className="automod-escalation" key={i}>
              <Field label={t("ui.strikes")} htmlFor={`am-strikes-${i}`}>
                <input
                  id={`am-strikes-${i}`}
                  type="number"
                  min="1"
                  max="100"
                  className="pixel-input"
                  value={step.strikes}
                  onChange={(e) =>
                    setConfig((c) => ({
                      ...c,
                      escalation: c.escalation.map((s, n) =>
                        n === i ? { ...s, strikes: Number(e.target.value) } : s,
                      ),
                    }))
                  }
                />
              </Field>
              <ActionEditor
                action={step.action}
                actions={saved.actions}
                info={info}
                t={t}
                onChange={(action) =>
                  setConfig((c) => ({
                    ...c,
                    escalation: c.escalation.map((s, n) => (n === i ? { ...s, action } : s)),
                  }))
                }
              />
              <PixelButton
                variant="secondary"
                onClick={() =>
                  setConfig((c) => ({ ...c, escalation: c.escalation.filter((_, n) => n !== i) }))
                }
              >
                {t("ui.remove")}
              </PixelButton>
            </div>
          ))}
          <PixelButton
            variant="secondary"
            disabled={config.escalation.length >= 10}
            onClick={() =>
              setConfig((c) => ({
                ...c,
                escalation: [
                  ...c.escalation,
                  {
                    strikes: Math.max(0, ...c.escalation.map((s) => s.strikes)) + 3,
                    window_secs: c.escalation[0]?.window_secs ?? 3600,
                    action: { kind: "timeout", secs: 600, role: null },
                  },
                ],
              }))
            }
          >
            {t("ui.add_step")}
          </PixelButton>
        </details>
        <p className="muted">
          {t("ui.sync")}:{" "}
          {saved.sync_error ?? (saved.synced_at ? dateTime(saved.synced_at) : t("ui.sync_pending"))}
        </p>
      </div>
    </SettingsRow>
  );
}
