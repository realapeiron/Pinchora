import type { Achievement, AchievementDraft } from "../api/achievements";
import { t } from "../../../i18n/dashboard";

export function achievementsSummary(items: Achievement[]) {
  return items.length
    ? t(items.length === 1 ? "{count} achievement" : "{count} achievements", { count: items.length })
    : t("No achievements yet");
}

export function achievementSummary(item: AchievementDraft) {
  return `${item.enabled ? item.kind : t("Off")} · ${item.target} · ${item.announce_channel_id ? t("Announcement channel selected") : t("No announcement channel")}`;
}
