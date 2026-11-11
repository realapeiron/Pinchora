import { t } from "../../../i18n/dashboard";
import type { Counters } from "../api/stats";

export function statsSummary(counters: Counters | undefined, isPublic: boolean, card: number | null = null) {
  const count = counters?.counters.length ?? 0;
  return {
    counters: count
      ? t(count === 1 ? "{count} counter channel" : "{count} counter channels", { count })
      : t("No counter channels"),
    publicPage: isPublic ? t("On") : t("Off"),
    card: card ? t("Card preview loaded") : t("Card ready to preview"),
  };
}
