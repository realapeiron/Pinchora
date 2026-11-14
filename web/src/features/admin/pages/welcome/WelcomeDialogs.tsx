import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { CardSvg } from "../../../../components/card/CardSvg";
import { Dialog, StickerPicker } from "../../../../components/card/CardProperties";
import { newLayer } from "../../../../lib/card";

export function WelcomeDialogs({
  dialog,
  setDialog,
  cards,
  setDesign,
  kind,
  setSelected,
  fill,
  me,
  uploadUrls,
  atlas,
  layer,
  setLayer,
  addLayer,
  design,
}: Pick<
  WelcomeModel,
  | "dialog"
  | "setDialog"
  | "cards"
  | "setDesign"
  | "kind"
  | "setSelected"
  | "fill"
  | "me"
  | "uploadUrls"
  | "atlas"
  | "layer"
  | "setLayer"
  | "addLayer"
  | "design"
>) {
  return (
    <>
      {dialog === "templates" && (
        <Dialog title={spaTranslate("TEMPLATES")} onClose={() => setDialog(null)}>
          <p className="muted">
            {deferredText("Start from one of these; your current design stays one undo away.")}
          </p>
          <div className="template-grid">
            {cards.templates.map((t) => (
              <button
                key={t.key}
                type="button"
                className="template"
                onClick={() => {
                  setDesign(t[kind]);
                  setSelected(null);
                  setDialog(null);
                }}
              >
                <span className="template-frame">
                  <CardSvg
                    design={t[kind]}
                    fill={fill}
                    avatarUrl={me.user.avatar_url}
                    uploads={uploadUrls}
                    atlas={atlas}
                    width={240}
                    className="template-art"
                  />
                </span>
                <span className="template-name">{t.name}</span>
              </button>
            ))}
          </div>
        </Dialog>
      )}
      {(dialog === "sticker" || dialog === "swap-sticker") && (
        <StickerPicker
          atlas={atlas}
          onClose={() => setDialog(null)}
          onPick={(sprite) => {
            if (dialog === "swap-sticker" && layer?.type === "sprite") {
              const k = Math.max(1, Math.round(layer.h / sprite.h));
              setLayer({ ...layer, sprite: sprite.id, w: sprite.w * k, h: sprite.h * k });
            } else {
              addLayer(newLayer(design, "sprite", sprite));
            }
            setDialog(null);
          }}
        />
      )}
    </>
  );
}
