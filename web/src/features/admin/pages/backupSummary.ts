import { t } from "../../../i18n/dashboard";
import type { BackupData } from "../api/commandControl";

export function backupSummary(data: BackupData, filename: string) {
  return {
    export: t("Ready to export"),
    import: filename || t("No file selected"),
    points: data.points.length
      ? t(data.points.length === 1 ? "{count} restore point" : "{count} restore points", {
          count: data.points.length,
        })
      : t("No restore points"),
  };
}
