import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { CARD, layerLabel, newLayer } from "../../../../lib/card";

export function LayerList({
  design,
  share,
  ordered,
  selected,
  setLayer,
  setSelected,
  restack,
  layer,
  duplicate,
  removeLayer,
  addLayer,
  setDialog,
}: Pick<
  WelcomeModel,
  | "design"
  | "share"
  | "ordered"
  | "selected"
  | "setLayer"
  | "setSelected"
  | "restack"
  | "layer"
  | "duplicate"
  | "removeLayer"
  | "addLayer"
  | "setDialog"
>) {
  return (
    <div className="card-layers">
      <Panel
        title={spaTranslate("LAYERS")}
        aside={
          <span className="counter">
            {design.layers.length}/{CARD.max_layers}
          </span>
        }
      >
        <div className="layer-add">
          <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "text"))}>
            {" "}
            {deferredText("+ Text")}{" "}
          </PixelButton>
          <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "avatar"))}>
            {" "}
            {deferredText("+ Avatar")}{" "}
          </PixelButton>
          <PixelButton className="button-small" onClick={() => setDialog("sticker")}>
            {" "}
            {deferredText("+ Sticker")}{" "}
          </PixelButton>
          <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "shape"))}>
            {" "}
            {deferredText("+ Shape")}{" "}
          </PixelButton>
        </div>
        <div className={`cost-meter ${share > 100 ? "is-over" : share >= 80 ? "is-high" : ""}`}>
          <span className="cost-label">
            <span className="pixel-label">{deferredText("DRAW COST")}</span>
            <span>{share}%</span>
          </span>
          <span
            className="cost-bar"
            role="meter"
            aria-label={deferredText("How much of a card's drawing allowance the visible layers use")}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.min(100, share)}
          >
            <span style={{ width: `${Math.min(100, share)}%` }} />
          </span>
        </div>
        <ol className="layer-list" aria-label={deferredText("Layers, top first")}>
          {ordered.map((l) => {
            const index = design.layers.indexOf(l);
            return (
              <li
                key={l.id}
                className={`layer-row ${l.id === selected ? "is-selected" : ""} ${l.hidden ? "is-hidden" : ""}`}
              >
                <button
                  type="button"
                  className="layer-icon"
                  aria-label={
                    l.hidden
                      ? deferredText("Show {arg1}", { arg1: layerLabel(l) })
                      : deferredText("Hide {arg1}", { arg1: layerLabel(l) })
                  }
                  aria-pressed={!l.hidden}
                  onClick={() => setLayer({ ...l, hidden: !l.hidden })}
                >
                  {l.hidden ? "◌" : "●"}
                </button>
                <button
                  type="button"
                  className="layer-icon layer-lock"
                  aria-label={
                    l.locked
                      ? deferredText("Unlock {arg1}", { arg1: layerLabel(l) })
                      : deferredText("Lock {arg1}", { arg1: layerLabel(l) })
                  }
                  aria-pressed={l.locked}
                  onClick={() => setLayer({ ...l, locked: !l.locked })}
                >
                  {l.locked ? "🔒" : "🔓"}
                </button>
                <button
                  type="button"
                  className="layer-name"
                  onClick={() => setSelected(l.id)}
                  aria-current={l.id === selected || undefined}
                >
                  <span className="layer-kind pixel-label">
                    {l.type === "sprite" ? "STICKER" : l.type.toUpperCase()}
                  </span>
                  <span className="layer-label">{layerLabel(l)}</span>
                </button>
                <span className="layer-moves">
                  <button
                    type="button"
                    className="layer-icon"
                    aria-label={deferredText("Move {arg1} up", { arg1: layerLabel(l) })}
                    disabled={index === design.layers.length - 1}
                    onClick={() => restack(l.id, 1)}
                  >
                    ▲
                  </button>
                  <button
                    type="button"
                    className="layer-icon"
                    aria-label={deferredText("Move {arg1} down", { arg1: layerLabel(l) })}
                    disabled={index === 0}
                    onClick={() => restack(l.id, -1)}
                  >
                    ▼
                  </button>
                </span>
              </li>
            );
          })}
          {design.layers.length === 0 && (
            <li className="empty-row muted">
              {" "}
              {deferredText("Nothing on the canvas yet. Add a layer or pick a template.")}{" "}
            </li>
          )}
        </ol>
        {layer && (
          <div className="layer-actions">
            <PixelButton variant="secondary" className="button-small" onClick={() => duplicate(layer)}>
              {deferredText("Duplicate")}
            </PixelButton>
            <PixelButton variant="ghost" className="button-small" onClick={() => removeLayer(layer.id)}>
              {spaTranslate("Delete")}
            </PixelButton>
          </div>
        )}
      </Panel>
    </div>
  );
}
