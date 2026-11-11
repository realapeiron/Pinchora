import type { Settings } from "../api/playerEconomy";
import { t } from "../../../i18n/dashboard";

export function economySummary(settings: Settings, percent = 100, affected?: number) {
  return {
    pool: t("{total} of {goal} shells", { total: settings.pool_total, goal: settings.pool_goal }),
    store: settings.shop_enabled
      ? t("Level {level} · {days} days", { level: settings.shop_min_level, days: settings.shop_min_days })
      : t("Off"),
    balances:
      affected === undefined
        ? t("Keep {percent}%", { percent })
        : t("{count} members · keep {percent}%", { count: affected, percent }),
  };
}
