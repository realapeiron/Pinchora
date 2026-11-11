import { channelLabel } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { t } from "../../../i18n/dashboard";

export function moderationSummary(channel: string | null, dm: boolean, rungs: number, info?: GuildInfo) {
  const selected = info?.channels.find((item) => item.id === channel);
  return {
    log: channel ? (selected ? channelLabel(selected) : t("Channel selected")) : t("Server log default"),
    dm: dm ? t("On") : t("Off"),
    ladder: t("{count} warn rungs", { count: rungs }),
  };
}
