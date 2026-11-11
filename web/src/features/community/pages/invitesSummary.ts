import { t } from "../../../i18n/dashboard";
import type { InviteSettings, Reward } from "../api/invites";

export function invitesSummary(settings: InviteSettings, rewards: Reward[]) {
  return {
    tracking: settings.enabled ? t("{count} day window", { count: settings.fake_days }) : t("Off"),
    links: settings.temp_enabled ? t("{count} uses per link", { count: settings.temp_max_uses }) : t("Off"),
    rewards: rewards.length
      ? t(rewards.length === 1 ? "{count} reward role" : "{count} reward roles", { count: rewards.length })
      : t("No reward roles"),
  };
}
