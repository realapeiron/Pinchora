import { t } from "../../../i18n/dashboard";
import type { ManagerGrant } from "../../moderation";

export function accessSummary(grants: ManagerGrant[]) {
  return grants.length
    ? t(grants.length === 1 ? "{count} manager role" : "{count} manager roles", { count: grants.length })
    : t("No manager roles");
}
