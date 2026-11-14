import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { PixelButton } from "../../../../components/PixelButton";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { type Delivery, DELIVERY_LABELS, undo, redo } from "../../../../lib/card";
import { KINDS, same } from "./welcomeDesign";

export function WelcomeToolbar({
  kind,
  setKind,
  setSelected,
  draft,
  saved,
  side,
  setSide,
  cards,
  setDialog,
  setHist,
  hist,
}: Pick<
  WelcomeModel,
  | "kind"
  | "setKind"
  | "setSelected"
  | "draft"
  | "saved"
  | "side"
  | "setSide"
  | "cards"
  | "setDialog"
  | "setHist"
  | "hist"
>) {
  return (
    <div className="card-toolbar" role="toolbar" aria-label={spaTranslate("Editor")}>
      <div className="segmented" role="tablist" aria-label={deferredText("Which image")}>
        {KINDS.map((k) => (
          <button
            key={k.kind}
            type="button"
            role="tab"
            aria-selected={kind === k.kind}
            className={`segment ${kind === k.kind ? "is-checked" : ""}`}
            onClick={() => {
              setKind(k.kind);
              setSelected(null);
            }}
          >
            {spaTranslate(k.label)}
            {!same(draft[k.kind], saved[k.kind]) && (
              <span className="dot" aria-label={spaTranslate("unsaved")} />
            )}
          </button>
        ))}
      </div>
      <label className="card-send">
        <span className="pixel-label">{deferredText("SEND")}</span>
        <select
          className="pixel-input pixel-select"
          aria-label={deferredText("How the image is sent")}
          value={side.delivery}
          onChange={(event) => setSide({ delivery: event.target.value as Delivery })}
        >
          {cards[kind].deliveries.map((d) => (
            <option key={d} value={d}>
              {spaTranslate(DELIVERY_LABELS[d])}
            </option>
          ))}
        </select>
      </label>
      <span className="card-toolbar-gap" />
      <PixelButton variant="secondary" className="button-small" onClick={() => setDialog("templates")}>
        {" "}
        {deferredText("Templates")}{" "}
      </PixelButton>
      <PixelButton
        variant="ghost"
        className="button-small"
        onClick={() => setHist(undo)}
        disabled={hist.past.length === 0}
        aria-label={spaTranslate("Undo")}
      >
        {deferredText("↶ Undo")}
      </PixelButton>
      <PixelButton
        variant="ghost"
        className="button-small"
        onClick={() => setHist(redo)}
        disabled={hist.future.length === 0}
        aria-label={spaTranslate("Redo")}
      >
        {" "}
        {deferredText("↷ Redo")}{" "}
      </PixelButton>
    </div>
  );
}
