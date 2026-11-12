import { useState } from "react";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { SaveBar } from "../../../components/SaveBar";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { ApiError } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useInviteWords } from "../api/inviteWords";
import { type InviteSettings, useInvites } from "../api/invites";
import { invitesSummary } from "./invitesSummary";

export function InviteSettingsForm({
  guild,
  saved,
  save,
  onToast,
}: {
  guild: string;
  saved: InviteSettings;
  save: ReturnType<typeof useInvites>["save"];
  onToast: (s: string) => void;
}) {
  const { t } = useInviteWords();
  const discord = useDiscord(guild);
  const [draft, setDraft] = useState(saved);
  const set = <K extends keyof InviteSettings>(key: K, value: InviteSettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const fields: (keyof InviteSettings)[] = [
    "enabled",
    "log_channel_id",
    "fake_days",
    "temp_enabled",
    "temp_channel_id",
    "temp_max_age_secs",
    "temp_max_uses",
    "temp_per_user_daily",
  ];
  const count = fields.filter((key) => draft[key] !== saved[key]).length;
  const error = save.error instanceof ApiError ? save.error : null;
  const problem = (key: string) => (error?.field === key ? error.message : null);
  const summary = invitesSummary(draft, []);
  return (
    <>
      <div className="settings-detail-list">
        <SettingsRow
          rowId="invite-settings-form-tracking"
          title={dashboardT("Tracking", {}, "TRACKING")}
          description={dashboardT("Count joins and choose the log channel.")}
          summary={summary.tracking}
          enabled={draft.enabled}
          onToggle={(v) => set("enabled", v)}
          toggleLabel={t("Count invitations")}
          initialOpen
        >
          <div className="settings-detail-fields">
            <p className="muted">{t("Count new members when their invite code can be identified.")}</p>
            <Field
              label={t("Join log channel")}
              htmlFor="inv-log"
              hint={t("Optional. Keep it visible only to the people who should review joins.")}
              error={problem("log_channel_id")}
            >
              <ChannelPicker
                posting
                id="inv-log"
                info={discord.data}
                value={draft.log_channel_id}
                onChange={(v) => set("log_channel_id", v)}
                none={t("No dedicated join log")}
              />
            </Field>
            <Field
              label={t("New account window")}
              htmlFor="inv-fake"
              hint={t(
                "Members with younger Discord accounts are shown separately and do not earn invite rewards.",
              )}
              error={problem("fake_days")}
            >
              <div className="inv-unit">
                <input
                  id="inv-fake"
                  type="number"
                  min="0"
                  max="365"
                  className="pixel-input"
                  value={draft.fake_days}
                  onChange={(e) => set("fake_days", Number(e.target.value))}
                />
                <span>{t("days")}</span>
              </div>
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="invite-settings-form-friend-links"
          title={dashboardT("Friend links", {}, "FRIEND LINKS")}
          description={dashboardT("Let members create temporary invite links.")}
          summary={summary.links}
          enabled={draft.temp_enabled}
          onToggle={(v) => set("temp_enabled", v)}
          toggleLabel={t("Let members make links")}
        >
          <div className="settings-detail-fields">
            <p className="muted">{t("Each link is sent privately to the member who requests it.")}</p>
            <Field
              label={t("Invite channel")}
              htmlFor="inv-temp-channel"
              hint={t("The bot needs Create Instant Invite in this channel.")}
              error={problem("temp_channel_id")}
            >
              <ChannelPicker
                posting
                id="inv-temp-channel"
                info={discord.data}
                value={draft.temp_channel_id}
                onChange={(v) => set("temp_channel_id", v)}
                none={t("Choose a channel")}
              />
            </Field>
            <div className="inv-number-grid">
              <Field label={t("Link lifetime")} htmlFor="inv-age" error={problem("temp_max_age_secs")}>
                <div className="inv-unit">
                  <input
                    id="inv-age"
                    type="number"
                    min="60"
                    max="604800"
                    className="pixel-input"
                    value={draft.temp_max_age_secs}
                    onChange={(e) => set("temp_max_age_secs", Number(e.target.value))}
                  />
                  <span>{t("sec")}</span>
                </div>
              </Field>
              <Field label={t("Uses per link")} htmlFor="inv-uses" error={problem("temp_max_uses")}>
                <input
                  id="inv-uses"
                  type="number"
                  min="1"
                  max="100"
                  className="pixel-input"
                  value={draft.temp_max_uses}
                  onChange={(e) => set("temp_max_uses", Number(e.target.value))}
                />
              </Field>
              <Field label={t("Per member daily")} htmlFor="inv-daily" error={problem("temp_per_user_daily")}>
                <input
                  id="inv-daily"
                  type="number"
                  min="1"
                  max="20"
                  className="pixel-input"
                  value={draft.temp_per_user_daily}
                  onChange={(e) => set("temp_per_user_daily", Number(e.target.value))}
                />
              </Field>
            </div>
            <p className="muted inv-small-note">
              {t("Members use")} <code>/invites temp</code>{" "}
              {t("or the “Invite a friend” button. Post the button with")}{" "}
              <code>/integrations invites post</code>.
            </p>
          </div>
        </SettingsRow>
      </div>
      <SaveBar
        words={t}
        count={count}
        saving={save.isPending}
        error={error?.message}
        blocked={draft.temp_enabled && !draft.temp_channel_id}
        onReset={() => {
          setDraft(saved);
          save.reset();
        }}
        onSave={() =>
          save.mutate(draft, {
            onSuccess: (now) => {
              setDraft(now);
              onToast(t("Invite settings saved."));
            },
          })
        }
      />
    </>
  );
}
