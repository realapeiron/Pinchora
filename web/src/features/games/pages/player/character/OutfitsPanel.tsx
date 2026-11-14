import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { t } from "../../../../../i18n";
import type { Dispatch, SetStateAction } from "react";
import { outfitChanged, type CharacterData } from "../../../api/webPlayer";
import { Field } from "../ProfileStudio";

export function OutfitsPanel({
  outfits,
  draft,
  names,
  setNames,
  edit,
  outfitPending,
  saveBlocked,
  onOutfit,
}: {
  outfits: CharacterData["outfits"];
  draft: string;
  names: Record<number, string>;
  setNames: Dispatch<SetStateAction<Record<number, string>>>;
  edit: (next: string) => void;
  outfitPending: boolean;
  saveBlocked: boolean;
  onOutfit: (slot: number, remove: boolean) => void;
}) {
  return (
    <Panel title={t("Saved outfits")}>
      <p className="muted">{t("Keep three favourite looks. Loading an outfit changes your draft.")}</p>
      <div className="player-outfits">
        {[1, 2, 3].map((slot) => {
          const savedOutfit = outfits.find((o) => o.slot === slot);
          return (
            <section key={slot}>
              <h3>{t("Outfit {slot}", { slot })}</h3>
              {savedOutfit && (
                <img
                  src={`/api/me/character/preview.png?spec=${encodeURIComponent(savedOutfit.spec)}&scale=2`}
                  alt={savedOutfit.name}
                />
              )}
              <Field label={t("Outfit name {slot}", { slot })}>
                <input
                  maxLength={32}
                  value={names[slot] ?? savedOutfit?.name ?? ""}
                  onChange={(e) => setNames((p) => ({ ...p, [slot]: e.target.value }))}
                />
              </Field>
              <div className="player-actions">
                <PixelButton
                  variant="secondary"
                  disabled={
                    outfitPending ||
                    saveBlocked ||
                    !outfitChanged(
                      savedOutfit,
                      draft,
                      names[slot] ?? savedOutfit?.name ?? t("Outfit {slot}", { slot }),
                    )
                  }
                  onClick={() => onOutfit(slot, false)}
                >
                  {t("Save to slot")}
                </PixelButton>
                {savedOutfit && (
                  <>
                    <PixelButton
                      variant="secondary"
                      disabled={draft === savedOutfit.spec}
                      onClick={() => edit(savedOutfit.spec)}
                    >
                      {t("Load outfit")}
                    </PixelButton>
                    <PixelButton
                      variant="ghost"
                      disabled={outfitPending}
                      onClick={() => onOutfit(slot, true)}
                    >
                      {t("Delete")}
                    </PixelButton>
                  </>
                )}
              </div>
            </section>
          );
        })}
      </div>
    </Panel>
  );
}
