import type { Community as CommunityData, GuildInfo } from "../../../lib/api";
import { channelLabel } from "../../../components/Pickers";
import { t } from "../../../i18n/dashboard";

function channel(info: GuildInfo | undefined, id: string | null) {
  if (!id) return t("No channel");
  const found = info?.channels.find((item) => item.id === id);
  return found ? channelLabel(found) : t("Channel selected");
}

export function communitySettingsSummary(data: CommunityData, info?: GuildInfo) {
  return {
    suggestions: channel(info, data.suggestions.channel_id),
    critters: data.critters.enabled
      ? data.critters.channel_id
        ? channel(info, data.critters.channel_id)
        : t("Anywhere busy")
      : t("Off"),
    surge: data.surge.enabled
      ? `${channel(info, data.surge.channel_id)} · ${t("Every {count} hours", { count: data.surge.every_hours })}`
      : t("Off"),
  };
}

export function birthdaySettingsSummary(enabled: boolean) {
  return t(enabled ? "On" : "Off");
}
