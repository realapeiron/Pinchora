import { translate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { PixelToggle } from "../../../components/PixelToggle";
import { Toast } from "../../../components/Toast";
import type { ModuleEntry } from "../../../lib/api";
import { useModuleToggle } from "./useModuleToggle";

export function ModuleMaster({ guild, module }: { guild: string; module: ModuleEntry }) {
  const switcher = useModuleToggle(guild);
  return (
    <>
      <div className="module-master">
        <span>{t(module.enabled ? "On" : "Off")}</span>
        <PixelToggle
          label={`${t("Module")}: ${translate(module.label)}`}
          checked={module.enabled}
          disabled={switcher.pending}
          onChange={() => switcher.toggle(module)}
        />
      </div>
      <Toast
        message={switcher.notice}
        onDone={switcher.clear}
        action={switcher.undo ? { label: translate("Undo"), onClick: switcher.undo } : undefined}
      />
    </>
  );
}
