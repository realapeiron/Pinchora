import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { ChannelPicker, RolePicker } from "../../../components/DashboardPickers";
import { Field } from "../../../components/Field";
import { SaveBar } from "../../../components/SaveBar";
import { SettingsRow } from "../../../components/SettingsRow";
import { Toast } from "../../../components/Toast";
import { ApiError, type Overview } from "../../../lib/api";
import { birthdayWords, type BirthdaySettings, useBirthdays, useSaveBirthdays } from "../api/birthdays";
import { useDiscord } from "../../../lib/queries";
import { Loading, Problem } from "../../../components/States";
import { SettingsPage } from "../../../components/PageHead";
import { birthdaySummary } from "./birthdaySummary";
import { BirthdayMessage } from "./BirthdayMessage";
import { BirthdayUpcoming } from "./BirthdayUpcoming";
import "../../../styles/birthdays.css";

export function Birthdays() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useBirthdays(guild);
  const save = useSaveBirthdays(guild);
  const [toast, setToast] = useState<string | null>(null);
  const t = birthdayWords(query.data?.strings, query.data?.language);
  return (
    <SettingsPage
      className="settings-page birthdays-page"
      title={t("BIRTHDAYS")}
      lead={t(
        "A small celebration for members who choose to share their day. Their birth year stays private.",
      )}
    >
      {query.isPending && <Loading label={t("Reading birthdays")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <BirthdayBody
          key={JSON.stringify(query.data.settings)}
          guild={guild}
          language={query.data.language}
          strings={query.data.strings}
          saved={query.data.settings}
          save={save}
          onSave={(draft) => save.mutate(draft, { onSuccess: () => setToast(t("Birthday settings saved.")) })}
          upcoming={query.data.upcoming}
          enabled={overview.modules.find((m) => m.key === "birthdays")?.enabled ?? true}
        />
      )}
      <Toast message={toast} onDone={() => setToast(null)} />
    </SettingsPage>
  );
}
function BirthdayBody({
  language,
  strings,
  guild,
  saved,
  save,
  onSave,
  upcoming,
  enabled,
}: {
  language: string;
  strings: Record<string, string>;
  guild: string;
  saved: BirthdaySettings;
  save: ReturnType<typeof useSaveBirthdays>;
  onSave: (draft: BirthdaySettings) => void;
  upcoming: { user_id: string; name: string; date: string; month: number; day: number }[];
  enabled: boolean;
}) {
  const t = birthdayWords(strings, language);
  const discord = useDiscord(guild);
  const [draft, setDraft] = useState(saved);
  const change = <K extends keyof BirthdaySettings>(key: K, value: BirthdaySettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const count = (Object.keys(saved) as (keyof BirthdaySettings)[]).filter(
    (k) => saved[k] !== draft[k],
  ).length;
  const problem = save.error instanceof ApiError ? save.error : null;
  const fieldError = (key: string) => problem?.errors.find((e) => e.field === key)?.message;
  const invalid =
    (!draft.channel_id && draft.enabled) ||
    draft.message.length > 500 ||
    !draft.message.includes("{user}") ||
    draft.shells_gift < 0 ||
    draft.shells_gift > 500 ||
    !draft.server_tz.trim();
  const summary = birthdaySummary(draft, discord.data, upcoming.length);
  return (
    <>
      {!enabled && (
        <p className="notice" role="note">
          {t("The Birthdays module is off.")} <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>
        </p>
      )}
      <div className="settings-detail-list">
        <SettingsRow
          rowId="birthdays-announce-birthdays"
          title={t("Announce birthdays")}
          description={t("Post a pixel cake card once on each member’s local birthday.")}
          summary={summary.announcement}
          enabled={draft.enabled}
          onToggle={(v) => change("enabled", v)}
          initialOpen
        >
          <div className="settings-detail-fields">
            <p className="panel-lead muted">
              {t(
                "Members save their date with /birthday set. A birthday is hidden from others until they choose to share it. Only members Pinchora has seen in this server appear in the upcoming list.",
              )}
            </p>
            <Field
              label={t("Announcement channel")}
              htmlFor="bd-channel"
              error={fieldError("channel_id")}
              hint={t("A text or announcement channel the bot can post in.")}
            >
              <ChannelPicker
                posting
                id="bd-channel"
                info={discord.data}
                value={draft.channel_id}
                onChange={(v) => change("channel_id", v)}
                none={t("Choose a channel")}
              />
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="birthdays-birthday-role"
          title={t("Birthday role")}
          description={t("Given for 24 hours, then removed by the timed roles ticker.")}
          summary={summary.role}
        >
          <div className="settings-detail-fields">
            <Field
              label={t("Birthday role")}
              htmlFor="bd-role"
              error={fieldError("role_id")}
              hint={t("Given for 24 hours, then removed by the timed roles ticker.")}
            >
              <RolePicker
                id="bd-role"
                info={discord.data}
                value={draft.role_id}
                onChange={(v) => change("role_id", v)}
                assignable
                none={t("No birthday role")}
              />
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="birthdays-server-time-zone"
          title={t("Server time zone")}
          description={t("Default for members who do not pick their own IANA time zone.")}
          summary={summary.zone}
        >
          <div className="settings-detail-fields">
            <Field
              label={t("Server time zone")}
              htmlFor="bd-zone"
              error={fieldError("server_tz")}
              hint={t("Default for members who do not pick their own IANA time zone.")}
            >
              <input
                id="bd-zone"
                className="pixel-input"
                value={draft.server_tz}
                onChange={(e) => change("server_tz", e.target.value)}
                list="birthday-zones"
              />
              <datalist id="birthday-zones">
                <option value="UTC" />
                <option value="Europe/Istanbul" />
                <option value="Europe/London" />
                <option value="America/New_York" />
                <option value="America/Los_Angeles" />
                <option value="Asia/Tokyo" />
              </datalist>
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="birthdays-shell-gift"
          title={t("Shell gift")}
          description={t("0–500 shells. System paid once a year per player across servers.")}
          summary={summary.gift}
        >
          <div className="settings-detail-fields">
            <Field
              label={t("Shell gift")}
              htmlFor="bd-gift"
              error={fieldError("shells_gift")}
              hint={t("0–500 shells. System paid once a year per player across servers.")}
            >
              <input
                id="bd-gift"
                type="number"
                min={0}
                max={500}
                className="pixel-input"
                value={draft.shells_gift}
                onChange={(e) => change("shells_gift", Number(e.target.value))}
              />
            </Field>
          </div>
        </SettingsRow>
        <BirthdayMessage
          draft={draft}
          change={change}
          fieldError={fieldError}
          t={t}
          summary={summary.message}
        />
        <SettingsRow
          rowId="birthdays-allow-ages"
          title={t("Allow ages")}
          description={t("Ages appear only for members who also choose to show theirs.")}
          summary={summary.age}
          enabled={draft.allow_age}
          onToggle={(v) => change("allow_age", v)}
        >
          <p className="muted">{t("Ages appear only for members who also choose to show theirs.")}</p>
        </SettingsRow>
        <SettingsRow
          rowId="birthdays-cake-day-badge"
          title={t("Cake Day badge")}
          description={t("Put a Cake Day ribbon on the birthday card.")}
          summary={summary.badge}
          enabled={draft.badge}
          onToggle={(v) => change("badge", v)}
        >
          <p className="muted">{t("Put a Cake Day ribbon on the birthday card.")}</p>
        </SettingsRow>
        <BirthdayUpcoming upcoming={upcoming} t={t} summary={summary.upcoming} />
      </div>
      <SaveBar
        words={t}
        count={count}
        saving={save.isPending}
        error={problem?.message}
        blocked={invalid}
        onReset={() => {
          setDraft(saved);
          save.reset();
        }}
        onSave={() => onSave(draft)}
      />
    </>
  );
}
