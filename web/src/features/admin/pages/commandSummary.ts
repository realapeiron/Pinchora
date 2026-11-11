import { t } from "../../../i18n/dashboard";
import type { ControlCommand, ControlData } from "../api/commandControl";

export function commandSummary(data: ControlData, commands: ControlCommand[]) {
  const disabled = commands.filter((command) => data.config.rules[command.path]?.enabled === false).length;
  return t(commands.length === 1 ? "{count} command · {disabled} off" : "{count} commands · {disabled} off", {
    count: commands.length,
    disabled,
  });
}

export function commandExtrasSummary(data: ControlData) {
  const aliases = Object.keys(data.config.aliases).length;
  return {
    aliases: aliases
      ? t(aliases === 1 ? "{count} alias" : "{count} aliases", { count: aliases })
      : t("No aliases"),
    permissions:
      data.permissions === null
        ? t("Permissions unavailable")
        : t(data.permissions.length === 1 ? "{count} permission" : "{count} permissions", {
            count: data.permissions.length,
          }),
  };
}
