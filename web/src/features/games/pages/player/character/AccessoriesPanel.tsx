import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { t } from "../../../../../i18n";
import { wearAccessory, type CharacterData } from "../../../api/webPlayer";
import { Field } from "../ProfileStudio";

export function AccessoriesPanel({
  catalog,
  shells,
  draft,
  edit,
  buyPending,
  onBuy,
}: {
  catalog: Pick<CharacterData["catalog"], "accessories" | "tints">;
  shells: number;
  draft: string;
  edit: (next: string) => void;
  buyPending: boolean;
  onBuy: (id: string) => void;
}) {
  const [, ...worn] = draft.split("+");
  return (
    <Panel title={t("Accessory wardrobe")}>
      <p className="muted">{t("One accessory per slot. Locked items explain how to unlock them.")}</p>
      <div className="player-collection">
        {catalog.accessories.map((a) => {
          const current = worn.find((v) => v.split("*")[0] === a.id);
          return (
            <article key={a.id} className={`player-collectible ${current ? "is-picked" : ""}`}>
              <h3>{a.name}</h3>
              {a.owned ? (
                <>
                  <PixelButton
                    variant="secondary"
                    aria-pressed={!!current}
                    onClick={() =>
                      edit(wearAccessory(draft, a.slot, current ? null : a.id, catalog.accessories))
                    }
                  >
                    {current ? t("Remove") : t("Wear")}
                  </PixelButton>
                  {current && a.tintable && (
                    <Field label={t("Accessory tint")}>
                      <select
                        value={current.split("*")[1] ?? ""}
                        onChange={(e) =>
                          edit(wearAccessory(draft, a.slot, a.id, catalog.accessories, e.target.value))
                        }
                      >
                        <option value="">{t("Original colour")}</option>
                        {catalog.tints.map((v) => (
                          <option key={v.id} value={v.id}>
                            {v.value}
                          </option>
                        ))}
                      </select>
                    </Field>
                  )}
                </>
              ) : a.buyable ? (
                <PixelButton disabled={buyPending || shells < a.price} onClick={() => onBuy(a.id)}>
                  {t("Buy · {price} shells", { price: a.price })}
                </PixelButton>
              ) : (
                <p className="muted">{a.unlock}</p>
              )}
            </article>
          );
        })}
      </div>
    </Panel>
  );
}
