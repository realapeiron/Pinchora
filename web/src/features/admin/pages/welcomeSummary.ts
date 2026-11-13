import { t as deferredText } from "../../../i18n/dashboard";
import type { GuildInfo, Settings } from "../../../lib/api";
import { channelLabel } from "../../../components/Pickers";
import { t } from "../../../i18n/dashboard";

function channel(info: GuildInfo | undefined, id: string | null) {
  if (!id) return t("No channel");
  const found = info?.channels.find((item) => item.id === id);
  return found ? channelLabel(found) : t("Channel selected");
}

export function welcomeSummary(settings: Settings, info: GuildInfo | undefined, delivery?: string) {
  const role = info?.roles.find((item) => item.id === settings.join_role_id);
  return {
    welcome:
      settings.welcome_enabled && settings.welcome_channel_id
        ? `${channel(info, settings.welcome_channel_id)} · ${delivery ?? t("Welcome card")}`
        : t("Off"),
    role: settings.join_role_id ? (role ? `@${role.name}` : t("Role selected")) : t("Off"),
    goodbye: settings.goodbye_channel_id
      ? `${channel(info, settings.goodbye_channel_id)} · ${settings.goodbye_message?.slice(0, 34) ?? deferredText("Default message")}`
      : t("Off"),
  };
}
