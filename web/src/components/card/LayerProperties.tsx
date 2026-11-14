import { PixelButton } from "../PixelButton";
import { t as deferredText } from "../../i18n/dashboard";
import { translate as spaTranslate } from "../../i18n";
import {
  type AvatarShape,
  type Canvas,
  type Layer,
  type ShapeKind,
  CARD,
  fitLayer,
  maxAvatarSide,
  stickerName,
} from "../../lib/card";
import { type Edit, NumberField, ColorField, Choice, Slider, Switch } from "./PropertyControls";
import { TextProperties } from "./TextProperties";

// /One layer

const AVATAR_SHAPES: { value: AvatarShape; label: string }[] = [
  { value: "circle", label: "Circle" },
  { value: "rounded", label: "Rounded" },
  { value: "square", label: "Square" },
  { value: "notched", label: "Pixel" },
];

const SHAPES: { value: ShapeKind; label: string }[] = [
  { value: "rounded", label: "Rounded" },
  { value: "rect", label: "Box" },
  { value: "notched", label: "Pixel" },
  { value: "ellipse", label: "Ellipse" },
  { value: "dashes", label: "Dashes" },
];

/** The selected layer's settings. On a phone (`simple`) only the ones a thumb needs. */
export function LayerProperties({
  layer,
  canvas,
  onChange,
  onPickSticker,
  simple,
}: {
  layer: Layer;
  /** What a layer has to fit in: no bigger than the canvas. */
  canvas: Canvas;
  onChange: Edit<Layer>;
  onPickSticker: () => void;
  simple: boolean;
}) {
  const id = (name: string) => `layer-${name}`;
  const set = <L extends Layer>(next: L, merge?: string) => onChange(fitLayer(next, canvas), merge);
  return (
    <div className="props">
      <h3 className="pixel-label props-title">
        {layer.type === "sprite" ? "STICKER" : layer.type.toUpperCase()}
      </h3>
      {layer.type === "text" && (
        <TextProperties layer={layer} canvas={canvas} onChange={set} simple={simple} />
      )}
      {layer.type === "avatar" && (
        <>
          <Choice
            label={spaTranslate("Shape")}
            value={layer.shape}
            options={AVATAR_SHAPES}
            onChange={(shape) => set({ ...layer, shape })}
          />
          <Slider
            id={id("border")}
            label={spaTranslate("Border")}
            value={layer.border_width}
            range={[0, Math.min(CARD.border_max, Math.floor(layer.w / 4))]}
            unit=" px"
            onChange={(border_width) => set({ ...layer, border_width }, "border")}
          />
          {layer.border_width > 0 && (
            <ColorField
              id={id("border-color")}
              label={deferredText("Border colour")}
              value={layer.border_color}
              onChange={(border_color) => set({ ...layer, border_color })}
            />
          )}
          {!simple && (
            <Switch
              label={deferredText("Chunky pixels")}
              checked={layer.pixelate}
              onChange={(pixelate) => set({ ...layer, pixelate })}
            />
          )}
        </>
      )}
      {layer.type === "sprite" && (
        <>
          <div className="prop prop-wide">
            <span className="prop-label">{spaTranslate("Picture")}</span>
            <PixelButton variant="secondary" className="button-small" onClick={onPickSticker}>
              {stickerName(layer.sprite)} {spaTranslate("· change")}{" "}
            </PixelButton>
          </div>
          {CARD.glyphs.includes(layer.sprite) && (
            <ColorField
              id={id("color")}
              label={spaTranslate("Colour")}
              value={layer.color}
              onChange={(color) => set({ ...layer, color })}
            />
          )}
        </>
      )}
      {layer.type === "shape" && (
        <>
          <Choice
            label={spaTranslate("Shape")}
            value={layer.shape}
            options={SHAPES}
            onChange={(shape) => set({ ...layer, shape })}
          />
          <ColorField
            id={id("fill")}
            label={spaTranslate("Fill")}
            value={layer.fill}
            onChange={(fill) => set({ ...layer, fill })}
          />
          {!simple && (
            <Slider
              id={id("border")}
              label={spaTranslate("Border")}
              value={layer.border_width}
              range={[0, CARD.border_max]}
              unit=" px"
              onChange={(border_width) => set({ ...layer, border_width }, "border")}
            />
          )}
          {!simple && layer.border_width > 0 && (
            <ColorField
              id={id("border-color")}
              label={deferredText("Border colour")}
              value={layer.border_color}
              onChange={(border_color) => set({ ...layer, border_color })}
            />
          )}
        </>
      )}
      {!simple && (
        <>
          <Slider
            id={id("opacity")}
            label={spaTranslate("Opacity")}
            value={layer.opacity}
            range={[0, 100]}
            unit="%"
            onChange={(opacity) => set({ ...layer, opacity }, "opacity")}
          />
          <div className="prop-grid">
            <NumberField
              id={id("x")}
              label="X"
              value={layer.x}
              range={CARD.position}
              onChange={(x) => set({ ...layer, x }, "x")}
            />
            <NumberField
              id={id("y")}
              label="Y"
              value={layer.y}
              range={CARD.position}
              onChange={(y) => set({ ...layer, y }, "y")}
            />
            <NumberField
              id={id("w")}
              label={spaTranslate("Width")}
              value={layer.w}
              range={[CARD.size[0] ?? 4, layer.type === "avatar" ? maxAvatarSide(canvas) : canvas.width]}
              onChange={(w) => set(layer.type === "avatar" ? { ...layer, w, h: w } : { ...layer, w }, "w")}
            />
            {layer.type !== "text" && layer.type !== "avatar" && (
              <NumberField
                id={id("h")}
                label={spaTranslate("Height")}
                value={layer.h}
                range={[CARD.size[0] ?? 4, canvas.height]}
                onChange={(h) => set({ ...layer, h }, "h")}
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}
