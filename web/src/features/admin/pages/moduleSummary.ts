import { t as deferredText } from "../../../i18n/dashboard";
import { channelLabel } from "../../../components/Pickers";
import { t } from "../../../i18n/dashboard";
import type { GuildInfo, ModuleEntry } from "../../../lib/api";
import { effective, type ModuleDraft } from "../api/modules";

export function moduleSummary(saved: ModuleEntry, draft: ModuleDraft | undefined, info?: GuildInfo) {
  const now = effective(saved, draft);
  if (!now.enabled) return t("Off");
  if (saved.server_only) return deferredText("Whole server");
  if (now.mode === "all") return t("All channels");
  const names = now.channels.map((id) => {
    const channel = info?.channels.find((item) => item.id === id);
    return channel ? channelLabel(channel) : t("Channel selected");
  });
  if (names.length === 0) return now.mode === "only" ? t("Only in: no channels") : t("Except: no channels");
  return `${t(now.mode === "only" ? "Only in" : "Everywhere but")}: ${names.join(", ")}`;
}
