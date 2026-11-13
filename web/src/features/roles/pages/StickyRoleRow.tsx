import { t as deferredText } from "../../../i18n/dashboard";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { stickyRolesSummary } from "./roleSummary";

export function StickyRoleRow({
  sticky,
  pending,
  onChange,
}: {
  sticky: boolean;
  pending: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <SettingsRow
      rowId="sticky-role-row-sticky-roles"
      title={deferredText("Sticky roles")}
      description={t("Restore eligible roles when a member returns.")}
      summary={stickyRolesSummary(sticky)}
      enabled={sticky}
      onToggle={onChange}
      toggleLabel={deferredText("Sticky roles")}
      toggleDisabled={pending}
    >
      <div className="settings-detail-fields">
        {" "}
        {deferredText(
          "A member who leaves and comes back within 60 days gets their roles back, as far as the bot can give them (not roles above its own, roles with admin powers, or roles an integration owns). Turning it on starts from the roles members hold now; turning it off forgets what was kept.",
        )}{" "}
      </div>
    </SettingsRow>
  );
}
