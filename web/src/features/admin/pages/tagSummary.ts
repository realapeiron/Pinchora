import type { AutoResponder, CustomCommand } from "../../../lib/api";
import { t } from "../../../i18n/dashboard";

export function commandListSummary(items: CustomCommand[]) {
  return items.length
    ? t(items.length === 1 ? "{count} command" : "{count} commands", { count: items.length })
    : t("No commands yet");
}

export function commandItemSummary(item: CustomCommand) {
  if (!item.enabled) return t("Off");
  const reply = item.responses.find((text) => text.trim());
  if (reply) return reply.replace(/\s+/g, " ").trim().slice(0, 80);
  if (item.embed) return t("Embed reply");
  return item.actions.length ? t("{count} actions", { count: item.actions.length }) : t("No reply");
}

export function responderListSummary(items: AutoResponder[]) {
  return items.length
    ? t(items.length === 1 ? "{count} responder" : "{count} responders", { count: items.length })
    : t("No responders yet");
}

export function responderItemSummary(item: AutoResponder) {
  if (!item.enabled) return t("Off");
  const reply = item.responses.find((text) => text.trim());
  if (reply) return reply.replace(/\s+/g, " ").trim().slice(0, 80);
  if (item.embed) return t("Embed reply");
  return item.actions.length ? t("{count} actions", { count: item.actions.length }) : t("No reply");
}
