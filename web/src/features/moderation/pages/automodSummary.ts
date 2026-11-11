import type { AutoConfig, AutoData, AutoRule, Words } from "../api/automod";
import { t } from "../../../i18n/dashboard";
export const dashboardText = t;

export function automodRuleSummary(rule: AutoRule, words: Words) {
  if (!rule.enabled) return t("Off");
  if (rule.log_only) return words("ui.dry_result");
  return rule.actions.length
    ? rule.actions.map((action) => words(`action.${action.kind}`)).join(" → ")
    : t("No actions");
}

export function automodSummary(config: AutoConfig, data: AutoData, words: Words) {
  return {
    profile: words(`ui.${config.profile}`),
    rules: t("{count} rules", { count: config.rules.length }),
    settings: config.alert_channel_id
      ? t("Alert channel selected · {count} escalation steps", { count: config.escalation.length })
      : t("Default alert channel · {count} escalation steps", { count: config.escalation.length }),
    dm: config.dm_on_action ? t("On") : t("Off"),
    test: t("Preview only"),
    hits: data.hits.length ? t("{count} recent hits", { count: data.hits.length }) : t("No recent hits"),
  };
}
