import { Link } from "react-router";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { Loading, Problem } from "../../../components/States";
import { birthdayWords, useBirthdays } from "../api/birthdays";
import { birthdaySettingsSummary } from "./communitySettingsSummary";

export function BirthdaySummary({ guild }: { guild: string }) {
  const birthdays = useBirthdays(guild);
  const data = birthdays.data;
  const t = birthdayWords(data?.strings, data?.language);
  return (
    <SettingsRow
      rowId="birthday-settings-link-birthdays"
      title={dt("row.Birthdays", {}, "BIRTHDAYS")}
      description={dt("Open birthday celebration settings.")}
      summary={birthdaySettingsSummary(!!data?.settings.enabled)}
    >
      <Link className="birthday-settings-link" to={`/g/${guild}/birthdays`}>
        {t("Edit settings →")}
      </Link>
      <p className="panel-lead muted">
        {t(
          "Members choose whether to share their date. Birth years stay private; celebrations use each member’s local midnight.",
        )}
      </p>
      {birthdays.isPending && <Loading label={t("Reading birthdays")} />}
      {birthdays.isError && <Problem error={birthdays.error} />}
      {data && (
        <>
          <p>
            <strong>{data.settings.enabled ? t("Announcements on") : t("Announcements off")}</strong>
            {data.settings.channel_id
              ? t(" · Channel {id}", { id: data.settings.channel_id })
              : t(" · No channel")}
            {t(" · {count} shell gift", { count: data.settings.shells_gift })}
          </p>
          <p className="muted">
            {data.upcoming.length
              ? t("Next: {list}", {
                  list: data.upcoming
                    .slice(0, 3)
                    .map((b) => `${b.name} (${b.day}/${b.month})`)
                    .join(", "),
                })
              : t("No shared birthdays in the next 30 days.")}
          </p>
          <Link to={`/g/${guild}/birthdays`}>
            {t("Set the role, message and time zone; see all upcoming dates")}
          </Link>
        </>
      )}
    </SettingsRow>
  );
}
