import { t } from "../../../i18n/dashboard";
import type { UtilityList } from "../api/utility";

export function utilitySummary(data: UtilityList) {
  return {
    timezone: data.timezone,
    sticky: data.sticky.length
      ? t(data.sticky.length === 1 ? "{count} sticky message" : "{count} sticky messages", {
          count: data.sticky.length,
        })
      : t("No sticky messages"),
    scheduled: data.scheduled.length
      ? t(data.scheduled.length === 1 ? "{count} scheduled message" : "{count} scheduled messages", {
          count: data.scheduled.length,
        })
      : t("No scheduled messages"),
  };
}
