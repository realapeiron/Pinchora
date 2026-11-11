import { useState } from "react";
import { useOutletContext } from "react-router";
import { translate as t } from "../../../i18n";
import { Field } from "../../../components/Field";
import { RolePicker, ChannelPicker } from "../../../components/Pickers";
import { SaveBar } from "../../../components/SaveBar";
import { useDiscord } from "../../../lib/queries";
import { type Overview } from "../../../lib/api";
import {
  nicknamePreview,
  type RobloxData,
  type RobloxSettings,
  useRoblox,
  useRobloxWrite,
} from "../api/roblox";
import { Loading, Problem } from "../../../components/States";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { robloxSummary } from "./robloxSummary";
import { RobloxBinds } from "./RobloxBinds";
import "../../../styles/roblox.css";

export function Roblox() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useRoblox(guild);
  const { save } = useRobloxWrite(guild);
  const [notice, setNotice] = useState("");
  return (
    <SettingsPage
      className="settings-page roblox-page"
      title={t("Roblox verification")}
      lead={t("Link Roblox profiles, then assign safe roles from group membership and ranks.")}
    >
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <RobloxEditor
          key={`${guild}:${JSON.stringify(query.data.settings)}`}
          guild={guild}
          data={query.data}
          save={save}
          onSave={(draft) => save.mutate(draft, { onSuccess: () => setNotice(t("Roblox settings saved.")) })}
        />
      )}
      {notice && <p role="status">{notice}</p>}
    </SettingsPage>
  );
}
function RobloxEditor({
  guild,
  data,
  save,
  onSave,
}: {
  guild: string;
  data: RobloxData;
  save: ReturnType<typeof useRobloxWrite>["save"];
  onSave: (draft: RobloxSettings) => void;
}) {
  const discord = useDiscord(guild).data;
  const [draft, setDraft] = useState<RobloxSettings>(data.settings);
  const change = <K extends keyof RobloxSettings>(key: K, value: RobloxSettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(data.settings);
  const preview = nicknamePreview(
    draft.nickname_template ?? "{roblox-name}",
    "DiscordMember",
    "RobloxPlayer",
  );
  return (
    <>
      <div className="settings-detail-list">
        <SettingsRow
          rowId="roblox-verification-settings"
          title={t("Verification settings")}
          description={dt("Link members to their Roblox profiles.")}
          summary={robloxSummary(draft, data.binds, discord).verification}
          enabled={draft.enabled}
          onToggle={(v) => change("enabled", v)}
          toggleLabel={t("Enable Roblox verification")}
        >
          <div className="settings-detail-fields">
            <p>{t("Members use /roblox verify to link their profile.")}</p>
            <Field label={t("Verified role")} htmlFor="rb-verified">
              <RolePicker
                id="rb-verified"
                info={discord}
                value={draft.verified_role_id}
                onChange={(v) => change("verified_role_id", v)}
                assignable
                forActions
                none={t("No role")}
              />
            </Field>
            <Field label={t("Verification channel")} htmlFor="rb-channel">
              <ChannelPicker
                id="rb-channel"
                info={discord}
                value={draft.verify_channel_id}
                onChange={(v) => change("verify_channel_id", v)}
                none={t("No channel")}
              />
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="roblox-hold-unverified-newcomers"
          title={t("Hold unverified newcomers")}
          description={dt("Choose how newcomers are held.")}
          summary={robloxSummary(draft, data.binds, discord).newcomers}
          enabled={draft.require_verified}
          onToggle={(v) => change("require_verified", v)}
        >
          <div className="settings-detail-fields">
            <p>{t("Requires the server join gate and a verification channel.")}</p>
            <Field label={t("Unverified role")} htmlFor="rb-unverified">
              <RolePicker
                id="rb-unverified"
                info={discord}
                value={draft.unverified_role_id}
                onChange={(v) => change("unverified_role_id", v)}
                assignable
                forActions
                none={t("No role")}
              />
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="roblox-nickname-template"
          title={t("Nickname template")}
          description={dt("Set member display names.")}
          summary={robloxSummary(draft, data.binds, discord).nickname}
        >
          <div className="settings-detail-fields">
            <Field
              label={t("Nickname template")}
              htmlFor="rb-template"
              hint={t("Use {roblox-name} and {discord-name}. Leave empty to keep Discord names.")}
            >
              <input
                id="rb-template"
                className="pixel-input"
                value={draft.nickname_template ?? ""}
                onChange={(e) => change("nickname_template", e.target.value || null)}
                maxLength={128}
              />
            </Field>
          </div>
          <div className="settings-detail-preview">
            <span className="field-label">{t("Preview")}</span>
            <strong>{preview}</strong>
          </div>
        </SettingsRow>
        <RobloxBinds guild={guild} data={data} discord={discord} />
      </div>
      <SaveBar
        count={dirty ? 1 : 0}
        saving={save.isPending}
        error={save.error?.message}
        onSave={() => onSave(draft)}
        onReset={() => setDraft(data.settings)}
      />
    </>
  );
}
