import { t } from "../../../i18n/dashboard";
export const dashboardText = t;
import type { AntiNukeConfig, AntiNukeData } from "../api/anti_nuke";

export function antiNukeSummary(config: AntiNukeConfig, data: AntiNukeData, words: (key: string) => string) {
  return {
    protection: config.enabled ? words(`response.${config.response}`) : t("Off"),
    autoRestore: config.auto_restore ? t("On") : t("Off"),
    revertVip: config.revert_vip_bans ? t("On") : t("Off"),
    removeBots: config.remove_bots ? t("On") : t("Off"),
    ownerOnly: config.owner_only_config ? t("On") : t("Off"),
    limits: t("{count} action limits", { count: Object.keys(config.limits).length }),
    trusted: config.trusted_users.length
      ? t("{count} trusted members", { count: config.trusted_users.length })
      : t("No trusted members"),
    vip: config.vip_roles.length
      ? t("{count} VIP roles", { count: config.vip_roles.length })
      : t("No VIP roles"),
    preview: t("Preview only"),
    events: data.events.length
      ? t("{count} recent events", { count: data.events.length })
      : t("No recent events"),
  };
}
