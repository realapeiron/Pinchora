import { translate as spaTranslate } from "../../../i18n";
import { useOutletContext } from "react-router";
import { Field } from "../../../components/Field";
import { MessageField } from "../../../components/MessageField";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelLink } from "../../../components/PixelButton";
import { SaveBar } from "../../../components/SaveBar";
import { SettingsRow, FieldGrid } from "../../../components/SettingsRow";
import { Toast } from "../../../components/Toast";
import { RichText } from "../../../components/RichText";
import { SettingsPage } from "../../../components/PageHead";
import { PreviewFrame } from "../components/PreviewFrame";
import { t } from "../../../i18n/dashboard";
import type { FieldError, Overview, Settings } from "../../../lib/api";
import { DELIVERY_LABELS } from "../../../lib/card";
import { useCards, useDiscord } from "../../../lib/queries";
import { checkMessage } from "../../../lib/rules";
import { useSettingsForm } from "../../../lib/useSettingsForm";
import { WelcomePreview } from "./WelcomePreview";
import { welcomeSummary } from "./welcomeSummary";

// Retained catalogue copy from the earlier panels:
// WELCOME MESSAGES; Post a card with the new member's avatar when someone joins. Needs the Server Members intent.
// Design the welcome and goodbye pictures on a canvas: move the avatar and the text, add pixel stickers, pick a pixel-art background or upload one, start from a template, and choose how they are sent.
// JOIN ROLE; GOODBYE; Say goodbye in a channel when a member leaves. Pick a channel to turn it on.

function check(value: Settings): (FieldError | null)[] {
  const fail = <T,>(c: { ok: true; value: T } | { ok: false; field: string; message: string }) =>
    c.ok ? null : { field: c.field, message: c.message };
  return [
    value.welcome_message === null ? null : fail(checkMessage("welcome", value.welcome_message)),
    value.goodbye_message === null ? null : fail(checkMessage("goodbye", value.goodbye_message)),
  ];
}

export function Welcome() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const cards = useCards(guild);
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;
  const joinRole = discord.data?.roles.find((role) => role.id === value.join_role_id);
  const summary = welcomeSummary(
    value,
    discord.data,
    cards.data ? spaTranslate(DELIVERY_LABELS[cards.data.welcome.delivery]).toLowerCase() : undefined,
  );

  return (
    <SettingsPage
      className="settings-page welcome-settings"
      title={spaTranslate("WELCOME")}
      lead={spaTranslate(
        "Greet new members with a card, give them a role, and say goodbye when someone leaves.",
      )}
    >
      <div className="settings-detail-list">
        <SettingsRow
          title={t("Welcome message")}
          description={t("Post a welcome message when someone joins.")}
          summary={summary.welcome}
          initialOpen
        >
          <div className="settings-detail-fields">
            <FieldGrid>
              <Field
                label={spaTranslate("Channel")}
                htmlFor="welcome-channel"
                error={errors.welcome_channel_id}
                hint={
                  value.welcome_enabled && !value.welcome_channel_id
                    ? spaTranslate("Pick a channel, or no card is posted.")
                    : spaTranslate("Text or announcement channels the bot can see.")
                }
              >
                <ChannelPicker
                  id="welcome-channel"
                  info={discord.data}
                  value={value.welcome_channel_id}
                  onChange={(id) => set("welcome_channel_id", id)}
                  none={spaTranslate("Not set")}
                  invalid={!!errors.welcome_channel_id}
                />
              </Field>
              <Field label={t("Format")} hint={t("Change the delivery format in the image editor.")}>
                <span className="welcome-format-value">
                  {cards.data ? spaTranslate(DELIVERY_LABELS[cards.data.welcome.delivery]) : "…"}
                </span>
                <PixelLink href={`/g/${guild}/welcome-image`} variant="ghost" className="button-small">
                  {spaTranslate("Open the editor")}
                </PixelLink>
              </Field>
            </FieldGrid>
            <MessageField
              guild={guild}
              kind="welcome"
              label={spaTranslate("Text above the card")}
              value={value.welcome_message}
              onChange={(text) => set("welcome_message", text)}
              error={errors.welcome_message}
              showPreview={false}
            />
            <div className="welcome-card-link">
              <strong>{t("Welcome card")}</strong>
              <span>
                {cards.data
                  ? `${cards.data.welcome.custom ? spaTranslate("Your own design") : spaTranslate("The classic card")}, ${spaTranslate(DELIVERY_LABELS[cards.data.welcome.delivery]).toLowerCase()}.`
                  : "…"}
              </span>
            </div>
          </div>
          <div className="settings-detail-preview">
            <WelcomePreview guild={guild} kind="welcome" message={value.welcome_message} />
            {cards.data && (
              <div className="designer">
                <div className="designer-preview">
                  <PreviewFrame
                    path={`/api/guilds/${guild}/preview/card`}
                    body={{ design: cards.data.welcome.design }}
                    alt={spaTranslate("The welcome image a new member gets")}
                    width={cards.data.welcome.design.width}
                    height={cards.data.welcome.design.height}
                  />
                </div>
              </div>
            )}
          </div>
        </SettingsRow>

        <SettingsRow
          title={t("Join role")}
          description={t("Give a role to each new member.")}
          summary={summary.role}
        >
          <div className="settings-detail-fields">
            <Field
              label={spaTranslate("Role new members get")}
              htmlFor="join-role"
              error={errors.join_role_id ?? (joinRole && !joinRole.assignable ? joinRole.why_not : null)}
              hint={spaTranslate("Bots are skipped. The bot's role must sit above it.")}
            >
              <RolePicker
                id="join-role"
                info={discord.data}
                value={value.join_role_id}
                onChange={(id) => set("join_role_id", id)}
                none={spaTranslate("No role")}
                assignable
                invalid={!!errors.join_role_id}
              />
            </Field>
            {discord.data && !discord.data.bot.manage_roles && (
              <p className="notice notice-heart">
                <RichText text="The bot lacks **Manage Roles**, so it can't hand out any role yet." />
              </p>
            )}
          </div>
        </SettingsRow>

        <SettingsRow
          title={t("Goodbye message")}
          description={t("Say goodbye in a channel when a member leaves.")}
          summary={summary.goodbye}
        >
          <div className="settings-detail-fields">
            <FieldGrid>
              <Field
                label={spaTranslate("Channel")}
                htmlFor="goodbye-channel"
                error={errors.goodbye_channel_id}
              >
                <ChannelPicker
                  id="goodbye-channel"
                  info={discord.data}
                  value={value.goodbye_channel_id}
                  onChange={(id) => set("goodbye_channel_id", id)}
                  none={spaTranslate("Off: no goodbye messages")}
                  invalid={!!errors.goodbye_channel_id}
                />
              </Field>
            </FieldGrid>
            <MessageField
              guild={guild}
              kind="goodbye"
              label={spaTranslate("Goodbye text")}
              value={value.goodbye_message}
              onChange={(text) => set("goodbye_message", text)}
              error={errors.goodbye_message}
              showPreview={false}
            />
          </div>
          <div className="settings-detail-preview">
            <WelcomePreview guild={guild} kind="goodbye" message={value.goodbye_message} />
          </div>
        </SettingsRow>
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
        message={form.toast === "Previous values restored." ? t("Previous values restored.") : form.toast}
        onDone={form.clearToast}
        action={form.undo ? { label: spaTranslate("Undo"), onClick: form.undo } : undefined}
      />
    </SettingsPage>
  );
}
