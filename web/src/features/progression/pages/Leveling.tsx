import { translate as spaTranslate } from "../../../i18n";
import { useOutletContext } from "react-router";

import { Choice } from "../../../components/Choice";
import { Field } from "../../../components/Field";
import { MessageField } from "../../../components/MessageField";
import { ChannelPicker } from "../../../components/Pickers";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { FieldError, Overview, Settings } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { LIMITS, checkMessage, checkVoiceAmount } from "../../../lib/rules";
import { useSettingsForm } from "../../../lib/useSettingsForm";
import { SettingsPage } from "../../../components/PageHead";
import { XpAdmin } from "./XpAdmin";
import { Multipliers } from "./LevelingMultipliers";
import { LevelRoles } from "./LevelingRoles";
import { RepRoles } from "./ReputationRoles";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { levelingSummary } from "./levelingSummary";
import { LevelUpPreview } from "./LevelUpPreview";
import { LevelingPolicyPanel } from "./LevelingPolicy";

// Retained catalogue copy: Members earn XP for chatting, climb levels and get a level-up message. Levels already earned stay when it is off.

const MODES = [
  {
    value: "channel",
    title: "Level-up channel",
    detail: "The channel below, or where they chatted if none.",
  },
  { value: "current", title: "Where they chatted", detail: "Always the channel the member was talking in." },
  { value: "dm", title: "Direct message", detail: "A private message to the member." },
  { value: "off", title: "Off", detail: "No message. Level roles are still given." },
];

function check(value: Settings): (FieldError | null)[] {
  const out: (FieldError | null)[] = [];
  if (value.levelup_message !== null) {
    const c = checkMessage("levelup", value.levelup_message);
    out.push(c.ok ? null : c);
  }
  const voice = checkVoiceAmount(value.voice_xp_amount);
  out.push(voice.ok ? null : voice);
  return out;
}

export function Leveling() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;
  const summary = levelingSummary(value);

  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("LEVELING")}
      lead={spaTranslate(
        "XP for chatting and voice, where level-ups go, multipliers, and the roles levels earn.",
      )}
    >
      <div className="settings-detail-list">
        <SettingsRow
          title={spaTranslate("Level-up message")}
          description={t("Choose message XP and level-up delivery.")}
          summary={summary.chat}
          initialOpen
        >
          <div className="settings-detail-fields">
            <Field label={spaTranslate("Where level-ups go")} error={errors.levelup_mode}>
              <Choice
                name="Where level-ups go"
                value={value.levelup_mode}
                options={MODES.map((mode) => ({
                  ...mode,
                  title: spaTranslate(mode.title),
                  detail: spaTranslate(mode.detail),
                }))}
                onChange={(mode) => set("levelup_mode", mode)}
              />
            </Field>
            <div className="form-grid">
              {value.levelup_mode === "channel" && (
                <Field
                  label={spaTranslate("Level-up channel")}
                  htmlFor="levelup-channel"
                  error={errors.levelup_channel_id}
                  hint={spaTranslate("Empty posts where the member chatted.")}
                >
                  <ChannelPicker
                    id="levelup-channel"
                    info={discord.data}
                    value={value.levelup_channel_id}
                    onChange={(id) => set("levelup_channel_id", id)}
                    none={spaTranslate("Where they chatted")}
                    invalid={!!errors.levelup_channel_id}
                  />
                </Field>
              )}
              <MessageField
                guild={guild}
                kind="levelup"
                label={spaTranslate("Level-up message")}
                value={value.levelup_message}
                onChange={(text) => set("levelup_message", text)}
                error={errors.levelup_message}
                showPreview={false}
              />
            </div>
          </div>
          <LevelUpPreview guild={guild} message={value.levelup_message} />
        </SettingsRow>

        <SettingsRow
          title={spaTranslate("VOICE XP")}
          description={t("Earn XP in voice channels.")}
          summary={summary.voice}
          enabled={value.voice_xp_enabled}
          onToggle={(on) => set("voice_xp_enabled", on)}
        >
          <div className="settings-detail-fields">
            <p className="muted">
              {spaTranslate(
                "Members in a voice channel with someone else earn XP every minute; not while muted or deafened, and not in the AFK channel.",
              )}
            </p>
            <Field
              label={spaTranslate("XP a minute")}
              htmlFor="voice-amount"
              error={errors.voice_xp_amount}
              hint={`${LIMITS.voice_xp[0]} to ${LIMITS.voice_xp[1]}. Chat gives 15 to 25 a minute.`}
            >
              <input
                id="voice-amount"
                className="pixel-input input-short"
                type="number"
                min={LIMITS.voice_xp[0]}
                max={LIMITS.voice_xp[1]}
                value={value.voice_xp_amount}
                onChange={(event) => set("voice_xp_amount", Number(event.target.value))}
                aria-invalid={errors.voice_xp_amount ? true : undefined}
              />
            </Field>
            {value.voice_xp_enabled && !value.leveling_enabled && (
              <p className="notice">
                {spaTranslate("Leveling is off, so nobody earns XP. Turn it on above.")}
              </p>
            )}
          </div>
        </SettingsRow>

        <Multipliers guild={guild} info={discord.data} />
        <LevelingPolicyPanel guild={guild} info={discord.data} />

        <LevelRoles
          guild={guild}
          info={discord.data}
          stack={value.level_roles_stack}
          setStack={(s) => set("level_roles_stack", s)}
        />

        <RepRoles guild={guild} info={discord.data} />
        <XpAdmin guild={guild} />
      </div>

      <SaveBar
        count={form.changes}
        saving={form.saving}
        blocked={form.blocked}
        error={form.barError}
        onSave={form.submit}
        onReset={form.reset}
      />
      <Toast
        message={form.toast ? t(form.toast) : null}
        onDone={form.clearToast}
        action={form.undo ? { label: spaTranslate("Undo"), onClick: form.undo } : undefined}
      />
    </SettingsPage>
  );
}
