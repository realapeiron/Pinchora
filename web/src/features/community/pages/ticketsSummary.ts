import type { TicketSettings, TicketsOverview } from "../api/tickets";
import { t } from "../../../i18n/dashboard";

export function ticketsSummary(data: TicketsOverview) {
  return {
    panels: data.panels.length
      ? t(data.panels.length === 1 ? "{count} ticket panel" : "{count} ticket panels", {
          count: data.panels.length,
        })
      : t("No ticket panels yet"),
    queue: data.summary.week.open_now
      ? t("{count} open tickets", { count: data.summary.week.open_now })
      : t("No open tickets"),
  };
}

export function ticketSettingsSummary(value: TicketSettings) {
  return {
    placement: value.open_category_id ? t("Category selected") : t("No category"),
    transcripts: value.dm_transcript ? t("On") : t("Off"),
    ratings: value.rating_enabled ? t("On") : t("Off"),
    limits: t("{count} per member · {hours} h", {
      count: value.max_open_per_user,
      hours: value.auto_close_hours,
    }),
  };
}
