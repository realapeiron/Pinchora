// The welcome and goodbye image editor's model: the design JSON the API stores (pinchora_core::card_design),
// the limits from rules.json, and the pure geometry of the canvas (moving, resizing, snapping) and its
// undo history. Nothing here touches the DOM, so all of it is unit-tested.

import rules from "./rules.json";

export const CARD = rules.card;

export type CardKind = "welcome" | "goodbye";
export type Delivery = "with_message" | "before_message" | "image_only" | "text_only" | "dm";

export type Background =
  | { type: "transparent" }
  | { type: "solid"; color: string }
  | {
      type: "gradient";
      from: string;
      to: string;
      direction: "vertical" | "horizontal" | "diagonal";
      pixel: boolean;
    }
  | { type: "sprite"; id: string }
  | { type: "upload"; id: string };

interface LayerBase {
  id: string;
  hidden: boolean;
  locked: boolean;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Percent. */
  opacity: number;
}

export type AvatarShape = "circle" | "square" | "rounded" | "notched";
export type ShapeKind = "rect" | "rounded" | "notched" | "ellipse" | "dashes";

export type AvatarLayer = LayerBase & {
  type: "avatar";
  shape: AvatarShape;
  border_color: string;
  border_width: number;
  pixelate: boolean;
};

export type TextLayer = LayerBase & {
  type: "text";
  text: string;
  font: "pixel" | "sans";
  bold: boolean;
  size: number;
  color: string;
  align: "left" | "center" | "right";
  uppercase: boolean;
  max_lines: number;
  shadow: boolean;
  outline_color: string;
  outline_width: number;
};

export type SpriteLayer = LayerBase & { type: "sprite"; sprite: string; color: string };

export type ShapeLayer = LayerBase & {
  type: "shape";
  shape: ShapeKind;
  fill: string;
  border_color: string;
  border_width: number;
};

export type Layer = AvatarLayer | TextLayer | SpriteLayer | ShapeLayer;

export interface CardDesign {
  version: number;
  width: number;
  height: number;
  background: Background;
  /** A black scrim over the background, in percent. */
  dim: number;
  /** Bottom to top. */
  layers: Layer[];
}

/** What the API says about one kind of image. */
export interface CardView {
  kind: CardKind;
  design: CardDesign;
  /** The server's own design, rather than the classic card from the welcome settings. */
  custom: boolean;
  delivery: Delivery;
  deliveries: Delivery[];
}

export interface CardTemplate {
  key: string;
  name: string;
  welcome: CardDesign;
  goodbye: CardDesign;
}

export interface CardUpload {
  id: string;
  width: number;
  height: number;
  url: string;
}

export interface Cards {
  welcome: CardView;
  goodbye: CardView;
  templates: CardTemplate[];
  uploads: CardUpload[];
}

export const DELIVERY_LABELS: Record<Delivery, string> = {
  with_message: "With the text, in one message",
  before_message: "Image first, then the text",
  image_only: "Only the image",
  text_only: "Only the text (no image)",
  dm: "By direct message to the new member",
};

export function clamp(n: number, range: readonly number[]): number {
  const [low = -Infinity, high = Infinity] = range;
  return Math.min(high, Math.max(low, n));
}

/** How tall a text layer of `size` px with `lines` lines is, as the bot measures it. */
export function textHeight(size: number, lines: number): number {
  return Math.round(size * CARD.line_height * Math.max(1, lines));
}

/** What a layer is, for the layer list. */
export function layerLabel(layer: Layer): string {
  switch (layer.type) {
    case "avatar":
      return "Avatar";
    case "text":
      return layer.text.trim().slice(0, 28) || "Text";
    case "sprite":
      return stickerName(layer.sprite);
    case "shape":
      return {
        rect: "Rectangle",
        rounded: "Rounded box",
        notched: "Pixel panel",
        ellipse: "Ellipse",
        dashes: "Dashes",
      }[layer.shape];
  }
}

/** `p_drowsepatch_cat` → `Drowsepatch cat`; a glyph by its name. */
export function stickerName(id: string): string {
  const bare = id.replace(/^(p|c|b|f|bg)_/, "").replaceAll("_", " ");
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

/** The atlas sprites that can be stickers, by group, from `atlas.json`'s names. */
export const STICKER_GROUPS: { prefix: string; name: string }[] = [
  { prefix: "p_", name: "Pets" },
  { prefix: "c_", name: "Critters & fish" },
  { prefix: "b_", name: "Badges" },
  { prefix: "f_", name: "Furniture" },
];

/** A new id no layer in `design` has. */
export function newId(design: CardDesign, kind: string): string {
  const taken = new Set(design.layers.map((l) => l.id));
  for (let n = 1; ; n++) {
    const id = `${kind}-${n}`;
    if (!taken.has(id)) return id;
  }
}

const base = (design: CardDesign, kind: string, w: number, h: number): LayerBase => ({
  id: newId(design, kind),
  hidden: false,
  locked: false,
  x: Math.round((design.width - w) / 2),
  y: Math.round((design.height - h) / 2),
  w,
  h,
  opacity: 100,
});

/** A fresh layer of `kind`, in the middle of the canvas. */
export function newLayer(
  design: CardDesign,
  kind: Layer["type"],
  sprite?: { id: string; w: number; h: number },
): Layer {
  switch (kind) {
    case "avatar": {
      const size = Math.round(Math.min(design.width, design.height) * 0.4);
      return {
        ...base(design, "avatar", size, size),
        type: "avatar",
        shape: "circle",
        border_color: "#ffd166",
        border_width: 8,
        pixelate: false,
      };
    }
    case "text": {
      const w = Math.round(design.width * 0.7);
      return {
        ...base(design, "text", w, textHeight(40, 1)),
        type: "text",
        text: "Welcome, {user}!",
        font: "sans",
        bold: true,
        size: 40,
        color: "#fff1e8",
        align: "center",
        uppercase: false,
        max_lines: 1,
        shadow: false,
        outline_color: "#000000",
        outline_width: 0,
      };
    }
    case "sprite": {
      const id = sprite?.id ?? "star";
      // Atlas sprites at a whole scale near a fifth of the canvas height; glyphs (7 or 9 pixels across) at 8×.
      const [w, h] = sprite ? [sprite.w, sprite.h] : [7, 6];
      const scale = Math.max(1, Math.round((design.height * 0.22) / h));
      return {
        ...base(design, "sprite", w * scale, h * scale),
        type: "sprite",
        sprite: id,
        color: "#ffd166",
      };
    }
    case "shape": {
      const [w, h] = [Math.round(design.width * 0.5), Math.round(design.height * 0.3)];
      return {
        ...base(design, "shape", w, h),
        type: "shape",
        shape: "rounded",
        fill: "#0b1029",
        border_color: "#2e3d75",
        border_width: 0,
        opacity: 70,
      };
    }
  }
}

/** `layer` with its text box as tall as its lines, as the API keeps it. */
export function tidy(layer: Layer): Layer {
  return layer.type === "text" ? { ...layer, h: textHeight(layer.size, layer.max_lines) } : layer;
}

// /Geometry

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A line the canvas draws while something snaps to it: vertical at `x`, or horizontal at `y`. */
export type Guide = { axis: "x"; at: number } | { axis: "y"; at: number };

/** Where `box` snaps when moved: to the canvas' edges and middle and to the other layers' edges and middles, on each axis
 * the nearest within `threshold`. Returns the moved box and the guides to draw. */
export function snapMove(
  box: Box,
  others: Box[],
  canvas: { width: number; height: number },
  threshold: number,
): { box: Box; guides: Guide[] } {
  const xs = [0, canvas.width / 2, canvas.width, ...others.flatMap((o) => [o.x, o.x + o.w / 2, o.x + o.w])];
  const ys = [0, canvas.height / 2, canvas.height, ...others.flatMap((o) => [o.y, o.y + o.h / 2, o.y + o.h])];
  const along = (start: number, size: number, lines: number[]) => {
    let best: { shift: number; at: number } | null = null;
    for (const edge of [start, start + size / 2, start + size]) {
      for (const line of lines) {
        const shift = line - edge;
        if (Math.abs(shift) <= threshold && (!best || Math.abs(shift) < Math.abs(best.shift)))
          best = { shift, at: line };
      }
    }
    return best;
  };
  const sx = along(box.x, box.w, xs);
  const sy = along(box.y, box.h, ys);
  const guides: Guide[] = [];
  if (sx) guides.push({ axis: "x", at: sx.at });
  if (sy) guides.push({ axis: "y", at: sy.at });
  return {
    box: { ...box, x: Math.round(box.x + (sx?.shift ?? 0)), y: Math.round(box.y + (sy?.shift ?? 0)) },
    guides,
  };
}

/** A resize handle: which edges it moves. */
export type Handle = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
export const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

/** How a layer may change shape: freely, kept square, kept in proportion, or (sprites) in whole steps of `unit`. */
export type Keep =
  { kind: "free" } | { kind: "square" } | { kind: "aspect"; unit?: { w: number; h: number } };

/** `start` with `handle` dragged by `(dx, dy)`; never smaller than `min` a side. The opposite corner or edge stays put. */
export function resizeBox(start: Box, handle: Handle, dx: number, dy: number, keep: Keep, min = 8): Box {
  const east = handle.includes("e");
  const west = handle.includes("w");
  const south = handle.includes("s");
  const north = handle.includes("n");
  let w = start.w + (east ? dx : west ? -dx : 0);
  let h = start.h + (south ? dy : north ? -dy : 0);
  if (keep.kind !== "free") {
    const ratio = keep.kind === "square" ? 1 : start.w / start.h;
    // The larger change wins; an edge handle drives its own side.
    const byWidth =
      east || west ? (north || south ? Math.abs(w - start.w) >= Math.abs(h - start.h) * ratio : true) : false;
    if (byWidth) h = w / ratio;
    else w = h * ratio;
    if (keep.kind === "aspect" && keep.unit) {
      // Whole steps of the sprite's own size, so its pixels stay square blocks.
      const steps = Math.max(1, Math.round(w / keep.unit.w));
      w = steps * keep.unit.w;
      h = steps * keep.unit.h;
    }
  }
  w = Math.max(min, Math.round(w));
  h = Math.max(min, Math.round(h));
  if (keep.kind === "square") h = w;
  const x = west
    ? start.x + start.w - w
    : !east && (north || south) && keep.kind !== "free"
      ? Math.round(start.x + (start.w - w) / 2)
      : start.x;
  const y = north
    ? start.y + start.h - h
    : !south && (east || west) && keep.kind !== "free"
      ? Math.round(start.y + (start.h - h) / 2)
      : start.y;
  return { x, y, w, h };
}

/** A text layer resized by a handle: the side handles change its width, the corners its font size too. */
export function resizeText(layer: TextLayer, handle: Handle, dx: number, dy: number): TextLayer {
  if (handle === "e" || handle === "w") {
    const box = resizeBox(layer, handle, dx, 0, { kind: "free" }, 24);
    return { ...layer, x: box.x, w: box.w };
  }
  const box = resizeBox(layer, handle, dx, dy, { kind: "aspect" }, 8);
  const size = clamp(Math.round((layer.size * box.w) / layer.w), CARD.text_size);
  const w = Math.round((layer.w * size) / layer.size);
  const x = handle.includes("w") ? layer.x + layer.w - w : layer.x;
  const h = textHeight(size, layer.max_lines);
  const y = handle.includes("n") ? layer.y + layer.h - h : layer.y;
  return { ...layer, size, w, x, y, h };
}

/** How an atlas background covers a `width`×`height` canvas: scaled up by a whole number, centred, as the bot does. */
export function coverScale(sw: number, sh: number, width: number, height: number) {
  const k = Math.max(1, Math.ceil(width / sw), Math.ceil(height / sh));
  return {
    k,
    x: Math.trunc((width - sw * k) / 2),
    y: Math.trunc((height - sh * k) / 2),
    w: sw * k,
    h: sh * k,
  };
}

// /Placeholders

export interface Fill {
  user: string;
  username: string;
  server: string;
  members: number | null;
}

const thousands = (n: number) => n.toLocaleString("en-US");

export function ordinal(n: number): string {
  const suffix =
    n % 100 >= 11 && n % 100 <= 13
      ? "th"
      : (({ 1: "st", 2: "nd", 3: "rd" } as Record<number, string>)[n % 10] ?? "th");
  return thousands(n) + suffix;
}

/** `text` with its placeholders filled in, in one pass, as the bot fills them. */
export function fillPlaceholders(text: string, fill: Fill): string {
  const values: Record<string, string> = {
    user: fill.user,
    username: fill.username,
    server: fill.server,
    members: fill.members === null ? "?" : thousands(fill.members),
    ordinal: fill.members === null ? "newest" : ordinal(fill.members),
  };
  return text.replace(/\{([a-z]+)\}/g, (whole, key: string) => values[key] ?? whole);
}

/** The `{word}`s in `text` the bot does not know. */
export function unknownPlaceholders(text: string): string[] {
  return [...text.matchAll(/\{([A-Za-z0-9_]+)\}/g)]
    .map((m) => m[0])
    .filter((p) => !CARD.placeholders.includes(p));
}

// /History

export interface History<T> {
  past: T[];
  present: T;
  future: T[];
}

/** How many steps back the editor keeps. */
export const HISTORY_MAX = 100;

export function history<T>(present: T): History<T> {
  return { past: [], present, future: [] };
}

/** `next` as a new step; the redo steps are gone. */
export function push<T>(h: History<T>, next: T): History<T> {
  if (Object.is(next, h.present)) return h;
  return { past: [...h.past, h.present].slice(-HISTORY_MAX), present: next, future: [] };
}

/** `next` in place of the present step: a drag or a word typed, still going on. */
export function replace<T>(h: History<T>, next: T): History<T> {
  return { ...h, present: next };
}

/** A step that began at `before` and ends at the present: one undo step for the whole drag. */
export function commit<T>(h: History<T>, before: T): History<T> {
  if (Object.is(before, h.present)) return h;
  return { past: [...h.past, before].slice(-HISTORY_MAX), present: h.present, future: [] };
}

export function undo<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  return {
    past: h.past.slice(0, -1),
    present: h.past[h.past.length - 1] as T,
    future: [h.present, ...h.future],
  };
}

export function redo<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  return { past: [...h.past, h.present], present: h.future[0] as T, future: h.future.slice(1) };
}
