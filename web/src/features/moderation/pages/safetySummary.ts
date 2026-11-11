import { t } from "../../../i18n/dashboard";
export const dashboardText = t;
import type { SafetyConfig, SafetyData } from "../api/verification";

export function safetySummary(config: SafetyConfig, data: SafetyData, words: (key: string) => string) {
  const gate = config.verification;
  const raid = config.raid;
  return {
    gate: gate.enabled ? words(gate.mode) : t("Off"),
    avatar: gate.block_default_avatar ? t("On") : t("Off"),
    names: gate.block_name_patterns ? t("On") : t("Off"),
    dm: gate.dm_on_block ? t("On") : t("Off"),
    raid: raid.enabled ? `${raid.join_threshold} / ${raid.window_secs}s · ${words(raid.action)}` : t("Off"),
    similarNames: raid.similar_names ? t("On") : t("Off"),
    similarAvatars: raid.similar_avatars ? t("On") : t("Off"),
    lock: raid.lock_channels ? t("On") : t("Off"),
    invites: raid.pause_invites ? t("On") : t("Off"),
    message: gate.message ? gate.message.slice(0, 40) : t("No message"),
    exemptions: t("{roles} roles · {users} users", {
      roles: gate.exempt_roles.length,
      users: gate.exempt_users.length,
    }),
    joins: t("{count} recent joins", { count: data.joins.length }),
    events: t("{count} raid events", { count: data.events.length }),
  };
}
