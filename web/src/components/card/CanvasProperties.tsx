import { t as deferredText } from "../../i18n/dashboard";
import { translate as spaTranslate } from "../../i18n";
import { useRef } from "react";
import { type Background, type CardDesign, type CardUpload, CARD, stickerName } from "../../lib/card";
import { PixelButton } from "../PixelButton";
import { type AtlasIndex } from "./CardSvg";
import { type Edit, NumberField, ColorField, Choice, Slider, Switch, AtlasThumb } from "./PropertyControls";

// /The canvas

const BACKGROUNDS: { value: Background["type"]; label: string }[] = [
  { value: "gradient", label: "Gradient" },
  { value: "solid", label: "Colour" },
  { value: "sprite", label: "Pixel art" },
  { value: "upload", label: "Upload" },
  { value: "transparent", label: "None" },
];

/** What the canvas itself has: its size, background and dimming. */
export function CanvasProperties({
  design,
  onChange,
  atlas,
  uploads,
  onUpload,
  uploading,
  simple,
}: {
  design: CardDesign;
  onChange: Edit<CardDesign>;
  atlas: AtlasIndex | undefined;
  uploads: CardUpload[];
  onUpload: (file: File) => void;
  uploading: boolean;
  simple: boolean;
}) {
  const file = useRef<HTMLInputElement>(null);
  const bg = design.background;
  const pictures = Object.keys(atlas ?? {})
    .filter((id) => id.startsWith(CARD.background_prefix))
    .sort();
  const setBackground = (next: Background) => onChange({ ...design, background: next });
  const switchTo = (type: Background["type"]) => {
    switch (type) {
      case "transparent":
        return setBackground({ type });
      case "solid":
        return setBackground({ type, color: bg.type === "gradient" ? bg.from : "#1d2b53" });
      case "gradient":
        return setBackground({
          type,
          from: bg.type === "solid" ? bg.color : "#1d2b53",
          to: "#0f1631",
          direction: "vertical",
          pixel: true,
        });
      case "sprite":
        return setBackground({ type, id: pictures[0] ?? "bg_starlit_jetty" });
      case "upload":
        if (uploads[0]) return setBackground({ type, id: uploads[0].id });
        file.current?.click();
    }
  };
  return (
    <div className="props">
      <h3 className="pixel-label props-title">{deferredText("CANVAS")}</h3>
      {!simple && (
        <div className="prop-grid">
          <NumberField
            id="canvas-w"
            label={spaTranslate("Width")}
            value={design.width}
            range={CARD.width}
            onChange={(width) => onChange({ ...design, width }, "width")}
          />
          <NumberField
            id="canvas-h"
            label={spaTranslate("Height")}
            value={design.height}
            range={CARD.height}
            onChange={(height) => onChange({ ...design, height }, "height")}
          />
        </div>
      )}
      <Choice label={spaTranslate("Background")} value={bg.type} options={BACKGROUNDS} onChange={switchTo} />
      {bg.type === "solid" && (
        <ColorField
          id="bg-color"
          label={spaTranslate("Colour")}
          value={bg.color}
          onChange={(color) => setBackground({ ...bg, color })}
        />
      )}
      {bg.type === "gradient" && (
        <>
          <div className="prop-grid">
            <ColorField
              id="bg-from"
              label={spaTranslate("From")}
              value={bg.from}
              onChange={(from) => setBackground({ ...bg, from })}
            />
            <ColorField
              id="bg-to"
              label={spaTranslate("To")}
              value={bg.to}
              onChange={(to) => setBackground({ ...bg, to })}
            />
          </div>
          <Choice
            label={spaTranslate("Direction")}
            value={bg.direction}
            options={[
              { value: "vertical", label: spaTranslate("Down") },
              { value: "horizontal", label: deferredText("Across") },
              { value: "diagonal", label: deferredText("Corner") },
            ]}
            onChange={(direction) => setBackground({ ...bg, direction })}
          />
          <Switch
            label={deferredText("Pixel dither & sparkles")}
            checked={bg.pixel}
            onChange={(pixel) => setBackground({ ...bg, pixel })}
          />
        </>
      )}
      {bg.type === "sprite" && (
        <div className="picture-grid" role="listbox" aria-label={deferredText("Background pictures")}>
          {pictures.map((id) => (
            <button
              key={id}
              type="button"
              role="option"
              aria-selected={bg.id === id}
              title={stickerName(id)}
              className={`picture ${bg.id === id ? "is-checked" : ""}`}
              onClick={() => setBackground({ type: "sprite", id })}
            >
              <AtlasThumb atlas={atlas} id={id} size={88} />
            </button>
          ))}
        </div>
      )}
      {bg.type === "upload" && uploads.length > 0 && (
        <div className="picture-grid" role="listbox" aria-label={deferredText("Uploaded pictures")}>
          {uploads.map((u) => (
            <button
              key={u.id}
              type="button"
              role="option"
              aria-selected={bg.id === u.id}
              className={`picture ${bg.id === u.id ? "is-checked" : ""}`}
              onClick={() => setBackground({ type: "upload", id: u.id })}
            >
              <img src={u.url} alt={`Upload ${u.width}×${u.height}`} />
            </button>
          ))}
        </div>
      )}
      <div className="prop prop-wide">
        <input
          ref={file}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          hidden
          data-testid="card-upload"
          onChange={(event) => {
            const picked = event.target.files?.[0];
            if (picked) onUpload(picked);
            event.target.value = "";
          }}
        />
        <PixelButton
          variant="secondary"
          className="button-small"
          onClick={() => file.current?.click()}
          disabled={uploading}
        >
          {uploading ? deferredText("Uploading…") : deferredText("Upload a picture")}
        </PixelButton>
        <p className="fine muted">
          {" "}
          {deferredText("PNG, JPEG, WebP or GIF up to")} {CARD.upload_max_bytes / 1024 / 1024}{" "}
          {deferredText("MB. It is shrunk to fit and covers the canvas.")}{" "}
        </p>
      </div>
      {bg.type !== "transparent" && (
        <Slider
          id="canvas-dim"
          label={deferredText("Darken")}
          value={design.dim}
          range={[0, CARD.dim_max]}
          unit="%"
          onChange={(dim) => onChange({ ...design, dim }, "dim")}
        />
      )}
    </div>
  );
}
