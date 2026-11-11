import type { GuildInfo } from "../../../lib/api";
import type { RobloxData, RobloxSettings } from "../api/roblox";
import { t } from "../../../i18n/dashboard";

export function robloxSummary(value: RobloxSettings, binds: RobloxData["binds"], info?: GuildInfo) {
  const role = (id: string | null) => {
    if (!id) return t("No role");
    const found = info?.roles.find((item) => item.id === id);
    return found ? `@${found.name}` : t("Role selected");
  };
  const channel = info?.channels.find((item) => item.id === value.verify_channel_id);
  return {
    verification: value.enabled
      ? `${role(value.verified_role_id)} · ${value.verify_channel_id ? (channel ? `#${channel.name}` : t("Channel selected")) : t("No channel")}`
      : t("Off"),
    newcomers: value.require_verified ? role(value.unverified_role_id) : t("Off"),
    nickname: value.nickname_template || t("Discord names unchanged"),
    binds: binds.length
      ? t(binds.length === 1 ? "{count} role bind" : "{count} role binds", { count: binds.length })
      : t("No role binds yet"),
  };
}
