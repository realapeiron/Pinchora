import type { Settings } from "../../../lib/api";
import { t } from "../../../i18n/dashboard";

export function pointsSummary(settings: Settings, storeCount = 0) {
  return {
    earning: settings.points_enabled
      ? `${settings.points_min}–${settings.points_max} ${settings.points_emoji} ${settings.points_name}`
      : t("Off"),
    store: t("{count} items on sale", { count: storeCount }),
  };
}
