import { Panel } from "../../../../../components/Panel";
import { t } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { PixelButton } from "../../../../../components/PixelButton";

export function BackgroundPanel({
  model,
}: {
  model: Pick<ProfileEditorModel, "data" | "draft" | "update" | "buy">;
}) {
  const { data, draft, update, buy } = model;
  return (
    <Panel title={t("Card backgrounds")}>
      <PixelButton
        variant="secondary"
        aria-pressed={draft.background === null}
        onClick={() => update("background", null)}
      >
        {t("Plain background")}
      </PixelButton>
      <div className="player-collection">
        {data.backgrounds.map((bg) => (
          <article
            key={bg.id}
            className={`player-collectible ${draft.background === bg.id ? "is-picked" : ""}`}
          >
            <img
              className="player-background-art"
              src={`/api/me/art/background/${bg.id}`}
              alt=""
              loading="lazy"
            />
            <h3>{bg.name}</h3>
            <PixelButton
              variant="secondary"
              aria-pressed={draft.background === bg.id}
              onClick={() => update("background", bg.id)}
            >
              {bg.owned ? t("Equip") : t("Try background")}
            </PixelButton>
            {!bg.owned && (
              <PixelButton
                disabled={buy.isPending || data.shells < bg.price}
                onClick={() => buy.mutate({ kind: "backgrounds", id: bg.id })}
              >
                {t("Buy · {price} shells", { price: bg.price })}
              </PixelButton>
            )}
          </article>
        ))}
      </div>
    </Panel>
  );
}
