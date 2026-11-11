import type { Settings } from "../../../lib/api";
import type { LevelingPolicy } from "../api/levelingPolicy";
import { t } from "../../../i18n/dashboard";

const destinations: Record<string, string> = {
  channel: "Level-up channel",
  current: "Where they chatted",
  dm: "Direct message",
  off: "Off",
};

export function memberXpSummary(name?: string) {
  return name ?? t("No member selected");
}

export function levelingSummary(
  settings: Partial<Settings>,
  counts: { multipliers?: number; levelRoles?: number; repRoles?: number; voiceRoles?: number } = {},
  policy?: LevelingPolicy,
) {
  return {
    chat: settings.leveling_enabled
      ? t("Level-up destination: {mode}", {
          mode: t(destinations[settings.levelup_mode ?? "off"] ?? settings.levelup_mode ?? "Off"),
        })
      : t("Off"),
    voice:
      settings.voice_xp_enabled && settings.leveling_enabled
        ? t("{count} XP per minute", { count: settings.voice_xp_amount ?? 0 })
        : t("Off"),
    multipliers: t("{count} multipliers", { count: counts.multipliers ?? 0 }),
    levelRoles: t("{count} level roles", { count: counts.levelRoles ?? 0 }),
    repRoles: t("{count} reputation roles", { count: counts.repRoles ?? 0 }),
    policy: policy
      ? t("{count}–{max} message XP", { count: policy.message_min, max: policy.message_max })
      : t("Loading settings…"),
    voiceRoles: t("{count} voice roles", { count: counts.voiceRoles ?? 0 }),
    import: t("No import yet"),
  };
}
