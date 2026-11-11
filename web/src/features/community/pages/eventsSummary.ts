import { t } from "../../../i18n/dashboard";
import type { Event } from "../api/events";

export function eventsSummary(timezone: string, upcoming: Event[], past: Event[]) {
  return {
    clock: timezone || t("No time zone"),
    upcoming: upcoming.length
      ? t(upcoming.length === 1 ? "{count} upcoming event" : "{count} upcoming events", {
          count: upcoming.length,
        })
      : t("No events yet"),
    history: past.length
      ? t(past.length === 1 ? "{count} past event" : "{count} past events", { count: past.length })
      : t("No past events"),
  };
}
