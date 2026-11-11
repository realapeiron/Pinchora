import { t } from "../../../i18n/dashboard";
import type { GuildInfo } from "../../../lib/api";
import type { AutoRole, Panel, RolesView } from "../api/roles";

export function autoRolesSummary(items: AutoRole[]) {
  if (items.length === 0) return t("No auto roles yet");
  const active = items.filter((item) => item.enabled).length;
  return t("{count} auto roles · {active} active", { count: items.length, active });
}

export function stickyRolesSummary(enabled: boolean) {
  return t(enabled ? "On" : "Off");
}

export function rolePanelsSummary(items: Panel[]) {
  return items.length
    ? t(items.length === 1 ? "{count} panel" : "{count} panels", { count: items.length })
    : t("No panels yet");
}

export function rolePanelSummary(panel: Panel, info?: GuildInfo) {
  const channel = info?.channels.find((item) => item.id === panel.channel_id);
  const location = panel.channel_id ? (channel ? `#${channel.name}` : t("Channel selected")) : t("Draft");
  return `${t(panel.roles.length === 1 ? "{count} role" : "{count} roles", { count: panel.roles.length })} · ${t(panel.style === "select" ? "Menu" : panel.style === "buttons" ? "Buttons" : "Reactions")} · ${location}`;
}

export function colorRolesSummary(colors: RolesView["colors"]) {
  if (!colors.enabled) return t("Off");
  return `${t(colors.mode === "hex" ? "Any colour" : "Palette colours")} · ${t("{count} roles made", { count: colors.roles.length })}`;
}
