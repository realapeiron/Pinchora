import { t as deferredText } from "../../../i18n/dashboard";
import type { GuildInfo } from "../../../lib/api";
import { channelLabel } from "../../../components/Pickers";
import { t } from "../../../i18n/dashboard";
import type { BirthdaySettings } from "../api/birthdays";

export function birthdaySummary(settings: BirthdaySettings, info: GuildInfo | undefined, upcoming: number) {
  const channel = info?.channels.find((item) => item.id === settings.channel_id);
  const role = info?.roles.find((item) => item.id === settings.role_id);
  return {
    announcement: settings.enabled
      ? settings.channel_id
        ? channel
          ? channelLabel(channel)
          : t("Channel selected")
        : t("No channel")
      : t("Off"),
    role: settings.role_id ? (role ? `@${role.name}` : t("Role selected")) : t("Off"),
    zone: settings.server_tz || t("No time zone"),
    gift: settings.shells_gift ? t("{count} shells", { count: settings.shells_gift }) : t("No gift"),
    message: settings.message ? settings.message.slice(0, 44) : t("No message"),
    age: t(settings.allow_age ? "On" : "Off"),
    badge: t(settings.badge ? "On" : "Off"),
    upcoming: upcoming
      ? t("{count} shared birthdays", { count: upcoming })
      : deferredText("No shared birthdays in the next 30 days."),
  };
}
