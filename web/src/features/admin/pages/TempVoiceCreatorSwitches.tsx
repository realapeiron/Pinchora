import { SwitchRow } from "../../../components/Field";
import { PixelToggle } from "../../../components/PixelToggle";
import { t } from "../api/tempVoiceI18n";
import type { CreatorPatch, Creator } from "../api/tempVoice";

export function CreatorSwitches({
  value,
  set,
}: {
  value: Creator;
  set: <K extends keyof CreatorPatch>(key: K, next: CreatorPatch[K]) => void;
}) {
  return (
    <div className="creator-switches">
      <SwitchRow
        title={t("Starts locked")}
        control={
          <PixelToggle
            label={t("Starts locked")}
            checked={value.default_locked}
            onChange={(on) => set("default_locked", on)}
          />
        }
      >
        {t("New rooms keep everyone out until the owner invites them or unlocks it. ")}
      </SwitchRow>
      <SwitchRow
        title={t("Button panel")}
        control={
          <PixelToggle label={t("Button panel")} checked={value.panel} onChange={(on) => set("panel", on)} />
        }
      >
        {t("Each room gets a message of buttons (lock, rename, kick…) in its own chat. ")}
      </SwitchRow>
      <SwitchRow
        title={t("Active")}
        control={
          <PixelToggle label={t("Active")} checked={value.enabled} onChange={(on) => set("enabled", on)} />
        }
      >
        {t("Off makes no new rooms; the ones that exist carry on and are cleaned up. ")}
      </SwitchRow>
    </div>
  );
}
