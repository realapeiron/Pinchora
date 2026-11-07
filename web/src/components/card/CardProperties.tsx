import { translate as spaTranslate } from "../../i18n";
import { type ReactNode, useRef, useState } from "react";

import {
  type AvatarShape,
  type Background,
  type CardDesign,
  type CardUpload,
  type Canvas,
  type Layer,
  type ShapeKind,
  type TextLayer,
  CARD,
  STICKER_GROUPS,
  clamp,
  fitLayer,
  maxAvatarSide,
  maxTextSize,
  stickerName,
  textHeight,
  unknownPlaceholders,
} from "../../lib/card";
import { PixelButton } from "../PixelButton";
import { PixelToggle } from "../PixelToggle";
import { ATLAS_SIZE, ATLAS_URL, type AtlasIndex, GLYPHS } from "./CardSvg";

/** Applies an edit; edits with the same `merge` key in a row make one undo step (typing, sliding). */
export type Edit<T> = (next: T, merge?: string) => void;

// /Small controls

function Row({
  label,
  htmlFor,
  children,
  wide,
}: {
  label: string;
  htmlFor?: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className={`prop ${wide ? "prop-wide" : ""}`}>
      <label className="prop-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
    </div>
  );
}

export function NumberField({
  id,
  label,
  value,
  range,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  range: readonly number[];
  onChange: (n: number) => void;
}) {
  return (
    <Row label={label} htmlFor={id}>
      <input
        id={id}
        className="pixel-input prop-number"
        type="number"
        inputMode="numeric"
        value={value}
        min={range[0]}
        max={range[1]}
        onChange={(event) => {
          const n = Number(event.target.value);
          if (event.target.value !== "" && Number.isFinite(n)) onChange(clamp(Math.round(n), range));
        }}
      />
    </Row>
  );
}

function Slider({
  id,
  label,
  value,
  range,
  unit = "",
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  range: readonly number[];
  unit?: string;
  onChange: (n: number) => void;
}) {
  return (
    <Row label={`${label}: ${value}${unit}`} htmlFor={id} wide>
      <input
        id={id}
        className="prop-slider"
        type="range"
        min={range[0]}
        max={range[1]}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </Row>
  );
}

export function ColorField({
  id,
  label,
  value,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (hex: string) => void;
}) {
  return (
    <Row label={label} htmlFor={id}>
      <span className="prop-color">
        <input
          type="color"
          aria-label={`${label} picker`}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <input
          id={id}
          className="pixel-input"
          value={value}
          maxLength={7}
          spellCheck={false}
          onChange={(event) => {
            const hex = event.target.value.trim();
            if (/^#[0-9a-fA-F]{6}$/.test(hex)) onChange(hex.toLowerCase());
          }}
        />
      </span>
    </Row>
  );
}

function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="prop prop-wide" role="radiogroup" aria-label={label}>
      <span className="prop-label">{label}</span>
      <span className="segmented">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={value === o.value}
            className={`segment ${value === o.value ? "is-checked" : ""}`}
            onClick={() => onChange(o.value)}
          >
            {o.label}
          </button>
        ))}
      </span>
    </div>
  );
}

function Switch({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <div className="prop prop-switch">
      <span className="prop-label">{label}</span>
      <PixelToggle label={label} checked={checked} onChange={onChange} />
    </div>
  );
}

/** A piece of the atlas as a small picture, pixels kept square. */
export function AtlasThumb({ atlas, id, size }: { atlas: AtlasIndex | undefined; id: string; size: number }) {
  const region = atlas?.[id];
  if (!region) return <span className="atlas-thumb" style={{ width: size, height: size }} />;
  const k = Math.min(size / region.w, size / region.h);
  return (
    <svg
      className="atlas-thumb"
      width={region.w * k}
      height={region.h * k}
      viewBox={`${region.x} ${region.y} ${region.w} ${region.h}`}
      aria-hidden="true"
    >
      <image
        href={ATLAS_URL}
        width={ATLAS_SIZE.w}
        height={ATLAS_SIZE.h}
        style={{ imageRendering: "pixelated" }}
      />
    </svg>
  );
}

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
      <h3 className="pixel-label props-title">CANVAS</h3>
      {!simple && (
        <div className="prop-grid">
          <NumberField
            id="canvas-w"
            label="Width"
            value={design.width}
            range={CARD.width}
            onChange={(width) => onChange({ ...design, width }, "width")}
          />
          <NumberField
            id="canvas-h"
            label="Height"
            value={design.height}
            range={CARD.height}
            onChange={(height) => onChange({ ...design, height }, "height")}
          />
        </div>
      )}
      <Choice label="Background" value={bg.type} options={BACKGROUNDS} onChange={switchTo} />
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
              label="From"
              value={bg.from}
              onChange={(from) => setBackground({ ...bg, from })}
            />
            <ColorField id="bg-to" label="To" value={bg.to} onChange={(to) => setBackground({ ...bg, to })} />
          </div>
          <Choice
            label="Direction"
            value={bg.direction}
            options={[
              { value: "vertical", label: "Down" },
              { value: "horizontal", label: "Across" },
              { value: "diagonal", label: "Corner" },
            ]}
            onChange={(direction) => setBackground({ ...bg, direction })}
          />
          <Switch
            label="Pixel dither & sparkles"
            checked={bg.pixel}
            onChange={(pixel) => setBackground({ ...bg, pixel })}
          />
        </>
      )}
      {bg.type === "sprite" && (
        <div className="picture-grid" role="listbox" aria-label="Background pictures">
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
        <div className="picture-grid" role="listbox" aria-label="Uploaded pictures">
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
          {uploading ? "Uploading…" : "Upload a picture"}
        </PixelButton>
        <p className="fine muted">
          PNG, JPEG, WebP or GIF up to {CARD.upload_max_bytes / 1024 / 1024} MB. It is shrunk to fit and
          covers the canvas.
        </p>
      </div>
      {bg.type !== "transparent" && (
        <Slider
          id="canvas-dim"
          label="Darken"
          value={design.dim}
          range={[0, CARD.dim_max]}
          unit="%"
          onChange={(dim) => onChange({ ...design, dim }, "dim")}
        />
      )}
    </div>
  );
}

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
            label="Shape"
            value={layer.shape}
            options={AVATAR_SHAPES}
            onChange={(shape) => set({ ...layer, shape })}
          />
          <Slider
            id={id("border")}
            label="Border"
            value={layer.border_width}
            range={[0, Math.min(CARD.border_max, Math.floor(layer.w / 4))]}
            unit=" px"
            onChange={(border_width) => set({ ...layer, border_width }, "border")}
          />
          {layer.border_width > 0 && (
            <ColorField
              id={id("border-color")}
              label="Border colour"
              value={layer.border_color}
              onChange={(border_color) => set({ ...layer, border_color })}
            />
          )}
          {!simple && (
            <Switch
              label="Chunky pixels"
              checked={layer.pixelate}
              onChange={(pixelate) => set({ ...layer, pixelate })}
            />
          )}
        </>
      )}
      {layer.type === "sprite" && (
        <>
          <div className="prop prop-wide">
            <span className="prop-label">Picture</span>
            <PixelButton variant="secondary" className="button-small" onClick={onPickSticker}>
              {stickerName(layer.sprite)} · change
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
            label="Shape"
            value={layer.shape}
            options={SHAPES}
            onChange={(shape) => set({ ...layer, shape })}
          />
          <ColorField
            id={id("fill")}
            label="Fill"
            value={layer.fill}
            onChange={(fill) => set({ ...layer, fill })}
          />
          {!simple && (
            <Slider
              id={id("border")}
              label="Border"
              value={layer.border_width}
              range={[0, CARD.border_max]}
              unit=" px"
              onChange={(border_width) => set({ ...layer, border_width }, "border")}
            />
          )}
          {!simple && layer.border_width > 0 && (
            <ColorField
              id={id("border-color")}
              label="Border colour"
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
            label="Opacity"
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
              label="Width"
              value={layer.w}
              range={[CARD.size[0] ?? 4, layer.type === "avatar" ? maxAvatarSide(canvas) : canvas.width]}
              onChange={(w) => set(layer.type === "avatar" ? { ...layer, w, h: w } : { ...layer, w }, "w")}
            />
            {layer.type !== "text" && layer.type !== "avatar" && (
              <NumberField
                id={id("h")}
                label="Height"
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

function TextProperties({
  layer,
  canvas,
  onChange,
  simple,
}: {
  layer: TextLayer;
  canvas: Canvas;
  onChange: Edit<TextLayer>;
  simple: boolean;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const unknown = unknownPlaceholders(layer.text);
  const setSize = (size: number, merge?: string) =>
    onChange({ ...layer, size, h: textHeight(size, layer.max_lines) }, merge);
  const insert = (token: string) => {
    const el = area.current;
    const [start, end] = el ? [el.selectionStart, el.selectionEnd] : [layer.text.length, layer.text.length];
    const text = (layer.text.slice(0, start) + token + layer.text.slice(end)).slice(0, CARD.text_max);
    onChange({ ...layer, text });
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(start + token.length, start + token.length);
    });
  };
  return (
    <>
      <Row label={spaTranslate("Text")} htmlFor="layer-text" wide>
        <textarea
          id="layer-text"
          ref={area}
          className="pixel-input pixel-textarea prop-text"
          value={layer.text}
          maxLength={CARD.text_max}
          rows={2}
          aria-invalid={unknown.length > 0 || layer.text.trim() === "" || undefined}
          onChange={(event) => onChange({ ...layer, text: event.target.value }, "text")}
        />
      </Row>
      <div className="chips" aria-label="Placeholders">
        {CARD.placeholders.map((p) => (
          <button key={p} type="button" className="chip" onClick={() => insert(p)}>
            {p}
          </button>
        ))}
      </div>
      {unknown.length > 0 && (
        <p className="field-error" role="alert">
          {unknown[0]} isn&apos;t a placeholder. Use {CARD.placeholders.join(" ")}.
        </p>
      )}
      {layer.text.trim() === "" && (
        <p className="field-error" role="alert">
          Write something, or delete the layer.
        </p>
      )}
      <Choice
        label="Font"
        value={layer.font}
        options={[
          { value: "sans", label: "Noto Sans" },
          { value: "pixel", label: "Pixel" },
        ]}
        onChange={(font) => onChange({ ...layer, font })}
      />
      <Slider
        id="layer-size"
        label="Size"
        value={layer.size}
        range={[CARD.text_size[0] ?? 8, maxTextSize(layer.max_lines, canvas.height)]}
        unit=" px"
        onChange={(size) => setSize(size, "size")}
      />
      <ColorField
        id="layer-color"
        label={spaTranslate("Colour")}
        value={layer.color}
        onChange={(color) => onChange({ ...layer, color })}
      />
      <Choice
        label="Align"
        value={layer.align}
        options={[
          { value: "left", label: "Left" },
          { value: "center", label: "Centre" },
          { value: "right", label: "Right" },
        ]}
        onChange={(align) => onChange({ ...layer, align })}
      />
      {!simple && (
        <>
          <div className="prop-grid">
            <Switch label="Bold" checked={layer.bold} onChange={(bold) => onChange({ ...layer, bold })} />
            <Switch
              label="Capitals"
              checked={layer.uppercase}
              onChange={(uppercase) => onChange({ ...layer, uppercase })}
            />
            <Switch
              label="Shadow"
              checked={layer.shadow}
              onChange={(shadow) => onChange({ ...layer, shadow })}
            />
          </div>
          <Slider
            id="layer-lines"
            label="Lines at most"
            value={layer.max_lines}
            range={[1, CARD.max_lines]}
            onChange={(max_lines) =>
              onChange({ ...layer, max_lines, h: textHeight(layer.size, max_lines) }, "lines")
            }
          />
          <Slider
            id="layer-outline"
            label="Outline"
            value={layer.outline_width}
            range={[0, CARD.outline_max]}
            unit=" px"
            onChange={(outline_width) => onChange({ ...layer, outline_width }, "outline")}
          />
          {layer.outline_width > 0 && (
            <ColorField
              id="layer-outline-color"
              label="Outline colour"
              value={layer.outline_color}
              onChange={(outline_color) => onChange({ ...layer, outline_color })}
            />
          )}
        </>
      )}
    </>
  );
}

/** The sticker chooser: the pixel glyphs, then the atlas by group, with a search. */
export function StickerPicker({
  atlas,
  onPick,
  onClose,
}: {
  atlas: AtlasIndex | undefined;
  onPick: (sprite: { id: string; w: number; h: number }) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const names = Object.keys(atlas ?? {}).sort();
  const groups = [
    { name: "Pixel glyphs", ids: CARD.glyphs },
    ...STICKER_GROUPS.map((g) => ({ name: g.name, ids: names.filter((n) => n.startsWith(g.prefix)) })),
  ];
  const q = query.trim().toLowerCase();
  const pick = (id: string) => {
    const glyph = GLYPHS[id];
    const region = atlas?.[id];
    if (glyph) onPick({ id, w: Math.max(...glyph.map((r) => r.length)), h: glyph.length });
    else if (region) onPick({ id, w: region.w, h: region.h });
  };
  return (
    <Dialog title="CHOOSE A STICKER" onClose={onClose}>
      <input
        className="pixel-input"
        placeholder="Search, e.g. cat, koi, lantern"
        aria-label="Search stickers"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <div className="sticker-groups">
        {groups.map((g) => {
          // Furniture is a long list: it shows when searched for.
          const ids = g.ids.filter((id) =>
            q ? stickerName(id).toLowerCase().includes(q) : g.name !== "Furniture",
          );
          if (ids.length === 0) return null;
          return (
            <section key={g.name}>
              <h4 className="pixel-label sticker-group">{g.name.toUpperCase()}</h4>
              <div className="sticker-grid">
                {ids.slice(0, 120).map((id) => (
                  <button
                    key={id}
                    type="button"
                    className="sticker"
                    title={stickerName(id)}
                    aria-label={stickerName(id)}
                    onClick={() => pick(id)}
                  >
                    {GLYPHS[id] ? <GlyphThumb id={id} /> : <AtlasThumb atlas={atlas} id={id} size={44} />}
                  </button>
                ))}
              </div>
            </section>
          );
        })}
        {!q && <p className="fine muted">Search to find furniture too.</p>}
      </div>
    </Dialog>
  );
}

/** A glyph sticker drawn as its pixels. */
function GlyphThumb({ id }: { id: string }) {
  const rows = GLYPHS[id] ?? [];
  const cols = Math.max(...rows.map((r) => r.length));
  return (
    <svg
      className="atlas-thumb"
      width={cols * 5}
      height={rows.length * 5}
      viewBox={`0 0 ${cols} ${rows.length}`}
      aria-hidden="true"
      shapeRendering="crispEdges"
    >
      {rows.flatMap((row, r) =>
        [...row].map((ch, c) =>
          ch === "#" || ch === "o" ? (
            <rect
              key={`${r}-${c}`}
              x={c}
              y={r}
              width={1}
              height={1}
              fill={ch === "#" ? "#ffd166" : "#fff1e8"}
            />
          ) : null,
        ),
      )}
    </svg>
  );
}

/** A modal panel with a close button; Escape closes it too. */
export function Dialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onPointerDown={(event) => event.target === event.currentTarget && onClose()}
      onKeyDown={(event) => event.key === "Escape" && onClose()}
    >
      <div className="panel panel-gold dialog card-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <div className="panel-body">
          <header className="panel-head">
            <h2 className="pixel-title">{title}</h2>
            <PixelButton variant="ghost" className="button-small" onClick={onClose} autoFocus>
              {spaTranslate("Close")}
            </PixelButton>
          </header>
          {children}
        </div>
      </div>
    </div>
  );
}
