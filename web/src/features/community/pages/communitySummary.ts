import { t } from "../../../i18n/dashboard";
import type { Data, Personal } from "./communityToolsShared";

function count(size: number, singular: string, plural: string, empty: string) {
  return size === 0 ? t(empty) : t(size === 1 ? singular : plural, { count: size });
}

export function personalSummary(personal: Personal) {
  return {
    reminders: count(personal.reminders.length, "{count} reminder", "{count} reminders", "No reminders yet"),
    checklist: count(personal.checklist.length, "{count} task", "{count} tasks", "No tasks yet"),
    highlights: count(personal.highlights.length, "{count} word", "{count} words", "No words followed"),
  };
}

export function anniversarySummary(channel: string | null, enabled: boolean) {
  return enabled ? (channel ? t("Channel selected") : t("No channel")) : t("Off");
}

export function confessionSettingsSummary(enabled: boolean) {
  return t(enabled ? "On" : "Off");
}

export function communitySummary(data: Data) {
  const pending = data.confessions.filter((item) => item.status === "pending").length;
  const reported = data.confessions.filter((item) => item.reports > 0).length;
  return {
    giveaways: count(data.giveaways.length, "{count} giveaway", "{count} giveaways", "No giveaways yet."),
    polls: count(data.polls.length, "{count} poll", "{count} polls", "No polls yet."),
    suggestions: count(
      data.suggestions.length,
      "{count} suggestion",
      "{count} suggestions",
      "No suggestions yet.",
    ),
    availability: data.message_content ? t("Available") : t("Needs Message Content intent"),
    channels: count(
      data.autopost.length,
      "{count} channel rule",
      "{count} channel rules",
      "No channel rules yet.",
    ),
    anniversary: anniversarySummary(data.anniversary.channel_id, data.anniversary.enabled),
    confessionSettings: confessionSettingsSummary(data.confession_settings.enabled),
    queue: count(pending, "{count} pending", "{count} pending", "Queue is clear."),
    reports: count(reported, "{count} reported", "{count} reported", "No reports"),
  };
}
