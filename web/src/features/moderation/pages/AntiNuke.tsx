import { useState } from "react";
import { useOutletContext } from "react-router";
import { Field } from "../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { SaveBar } from "../../../components/SaveBar";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { draftProblem, useAntiNuke, words, type AntiNukeData, type AntiNukeConfig } from "../api/anti_nuke";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { AntiNukeLimits, AntiNukePreview } from "./AntiNukeRows";
import { AntiNukeEvents } from "./AntiNukeEvents";
import { AntiNukePeople } from "./AntiNukePeople";
import { AntiNukeFlagRow } from "./AntiNukeFlagRow";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./antiNukeSummary";
import { antiNukeSummary } from "./antiNukeSummary";
import "../../../styles/anti_nuke.css";

export function AntiNuke() {
  const overview = useOutletContext<Overview>();
  const { query } = useAntiNuke(overview.guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Editor key={JSON.stringify(query.data.config)} guild={overview.guild.id} saved={query.data} />;
}
function Editor({ guild, saved }: { guild: string; saved: AntiNukeData }) {
  const t = words(saved.strings, saved.language);
  const [config, setConfig] = useState(saved.config);
  const [kind, setKind] = useState("channel_delete");
  const [count, setCount] = useState(6);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const { save, preview, restore } = useAntiNuke(guild);
  const discord = useDiscord(guild);
  const summary = antiNukeSummary(config, saved, t);
  const dirty = JSON.stringify(config) !== JSON.stringify(saved.config);
  const locked = !saved.can_edit || save.isPending;
  const set = <K extends keyof AntiNukeConfig>(key: K, value: AntiNukeConfig[K]) =>
    setConfig((c) => ({ ...c, [key]: value }));
  return (
    <SettingsPage className="settings-page antinuke-page" title={t("ui.title")} lead={t("ui.lead")}>
      {!saved.can_edit && <p className="notice">{t("ui.readonly")}</p>}
      <div className="settings-detail-list">
        <SettingsRow
          rowId="anti-nuke-protection"
          title={t("ui.protection")}
          description={dashboardText("Choose the response to destructive actions.")}
          summary={summary.protection}
          enabled={config.enabled}
          onToggle={(v) => set("enabled", v)}
          toggleDisabled={locked}
          toggleLabel={t("ui.enabled")}
          initialOpen
        >
          <div className="settings-detail-fields">
            <div className="antinuke-settings-grid">
              <Field label={t("ui.response")} htmlFor="an-response">
                <select
                  id="an-response"
                  className="pixel-input pixel-select"
                  value={config.response}
                  disabled={locked}
                  onChange={(e) => set("response", e.target.value)}
                >
                  {saved.responses.map((r) => (
                    <option key={r} value={r}>
                      {t(`response.${r}`)}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("ui.alert_channel")} htmlFor="an-alert">
                <ChannelPicker
                  id="an-alert"
                  info={discord.data}
                  value={config.log_channel_id}
                  none={t("ui.fallback")}
                  disabled={locked}
                  onChange={(v) => set("log_channel_id", v)}
                />
              </Field>
              {config.response === "quarantine" && (
                <Field label={t("ui.quarantine")} htmlFor="an-quarantine">
                  <RolePicker
                    id="an-quarantine"
                    info={discord.data}
                    value={config.quarantine_role_id}
                    none={t("ui.none")}
                    disabled={locked}
                    onChange={(v) => set("quarantine_role_id", v)}
                  />
                </Field>
              )}
            </div>
            <p className="muted antinuke-guidance">{t("ui.recovery_hint")}</p>
          </div>
        </SettingsRow>
        <AntiNukeFlagRow
          rowId="antinuke-flag-auto-restore"
          title={t("ui.auto_restore")}
          description={dashboardText("Restore damaged settings automatically.")}
          summary={summary.autoRestore}
          enabled={config.auto_restore}
          disabled={locked}
          onToggle={(v) => set("auto_restore", v)}
        />
        <AntiNukeFlagRow
          rowId="antinuke-flag-revert-vip"
          title={t("ui.revert_vip")}
          description={dashboardText("Reverse bans on protected members.")}
          summary={summary.revertVip}
          enabled={config.revert_vip_bans}
          disabled={locked}
          onToggle={(v) => set("revert_vip_bans", v)}
        />
        <AntiNukeFlagRow
          rowId="antinuke-flag-remove-bots"
          title={t("ui.remove_bots")}
          description={dashboardText("Remove bots involved in destructive actions.")}
          summary={summary.removeBots}
          enabled={config.remove_bots}
          disabled={locked}
          onToggle={(v) => set("remove_bots", v)}
        />
        <AntiNukeLimits
          saved={saved}
          config={config}
          t={t}
          summary={summary.limits}
          locked={locked}
          onLimits={(value) => set("limits", value)}
        />
        <AntiNukePeople
          saved={saved}
          config={config}
          t={t}
          info={discord.data}
          locked={locked}
          saving={save.isPending}
          summaryTrusted={summary.trusted}
          summaryVip={summary.vip}
          onList={(key, value) => set(key, value)}
        />
        <AntiNukeFlagRow
          rowId="antinuke-flag-owner-only"
          title={t("ui.owner_only")}
          description={dashboardText("Allow only the owner to edit trusted members.")}
          summary={summary.ownerOnly}
          enabled={config.owner_only_config}
          disabled={!saved.can_trust || save.isPending}
          onToggle={(v) => set("owner_only_config", v)}
        />
        <AntiNukePreview
          saved={saved}
          config={config}
          t={t}
          summary={summary.preview}
          preview={preview}
          kind={kind}
          setKind={setKind}
          count={count}
          setCount={setCount}
        />
        <AntiNukeEvents
          saved={saved}
          t={t}
          summary={summary.events}
          restore={restore}
          confirm={confirm}
          setConfirm={setConfirm}
          notice={notice}
          setNotice={setNotice}
        />
      </div>
      <SaveBar
        words={(key, values) => t(`ui.${key}`, values)}
        count={dirty ? 1 : 0}
        saving={save.isPending}
        error={draftProblem(config) ? t("invalid") : save.error?.message}
        blocked={locked || draftProblem(config)}
        onSave={() => save.mutate(config, { onSuccess: () => setNotice(t("saved")) })}
        onReset={() => {
          setConfig(saved.config);
          save.reset();
        }}
      />
    </SettingsPage>
  );
}
