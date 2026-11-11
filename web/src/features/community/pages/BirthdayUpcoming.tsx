import { locale } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";

type Upcoming = { user_id: string; name: string; date: string; month: number; day: number }[];
export function BirthdayUpcoming({
  upcoming,
  t,
  summary,
}: {
  upcoming: Upcoming;
  t: (key: string, args?: Record<string, string | number>) => string;
  summary: string;
}) {
  return (
    <SettingsRow
      title={t("UPCOMING 30 DAYS")}
      description={dt("See members who shared their birthday.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="panel-lead muted">
          {t(
            "Only members who allow others to see their birthday appear here. Years and ages stay off this list.",
          )}
        </p>
        {upcoming.length === 0 ? (
          <p className="empty-row muted">{t("No shared birthdays in the next 30 days.")}</p>
        ) : (
          <ul className="birthday-upcoming">
            {upcoming.map((row) => (
              <li key={row.user_id}>
                <time dateTime={row.date}>
                  {new Date(`${row.date}T12:00:00`).toLocaleDateString(
                    locale() === "tr" ? "tr-TR" : "en-US",
                    {
                      month: "short",
                      day: "numeric",
                    },
                  )}
                </time>
                <span>{row.name}</span>
                <span className="pixel-label">#{row.user_id.slice(-4)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </SettingsRow>
  );
}
