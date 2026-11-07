// `render_room` of `pinchora-render/src/room.rs`, drawn into a pixel buffer at native resolution: a 2:1
// pixel-art projection where every art pixel is one pixel. The page scales the result by a whole number
// so the pixels stay crisp. Every loop here is a line-for-line port, so the pixels match the bot's PNG.

import { type Img, type Rgba, blank, flipHorizontal, overlay } from "./image";
import { drawOrder } from "./order";
import {
  LEVEL_H,
  MARGIN,
  SLAB,
  WALL_H,
  type WallSide,
  floorTop,
  levelTop,
  plane,
  wallColumn,
} from "./projection";
import type { Cursor, Scene, SceneItem } from "./scene";
import { shade } from "./sprites";

const OUTLINE: Rgba = [0x1b, 0x1d, 0x2b, 255];
const GOLD: Rgba = [0xff, 0xd1, 0x66, 255];
const OK: Rgba = [0x06, 0xd6, 0xa0, 255];
const BAD: Rgba = [0xff, 0x4d, 0x6d, 255];

/** Size of one font pixel in scene units, and how far labels sit outside the floor edge. */
const GLYPH_PX = 3;
const LABEL_GAP = 14;

const f32 = Math.fround;

/** The picture and the map from its pixels to the scene item that drew them. */
export interface Rendered {
  image: Img;
  /** Per pixel: 1 + the index in `scene.items` of the item drawn there last, 0 for none. */
  hit: Int32Array<ArrayBuffer>;
}

type Box = [number, number, number, number];

/** Where a floor, surface or figure sprite goes: its top-left corner in scene space. */
export function anchor(item: SceneItem, sprite: Img): [number, number] {
  const { x, y, w, d } = item;
  const full = (w + d) * 32;
  const sw = sprite.width;
  const cx = 32 * (x - y) + 16 * (w - d);
  const cy = 16 * (x + y) + 8 * (w + d);
  // A sprite narrower than its footprint stands on a smaller diamond in the middle.
  const bottom = item.layer === "figure" ? cy + 6 : cy + Math.floor(Math.min(sw, full) / 4) - item.lift;
  return [cx - Math.floor(sw / 2), bottom - sprite.height];
}

const standing = (item: SceneItem) =>
  item.layer === "floor" || item.layer === "surface" || item.layer === "figure";

/** Pixel bounds (min x, min y, max x, max y) of everything the scene draws. */
export function bounds(scene: Scene, sprites: readonly Img[]): Box {
  const [w, d] = [scene.width, scene.depth];
  let b: Box = [-32 * d - 2, 1 - WALL_H - 6, 32 * w + 2, 16 * (w + d) + SLAB + 2];
  if (scene.grid) b = [b[0] - LABEL_GAP, b[1], b[2] + LABEL_GAP, b[3] + LABEL_GAP + 4 * GLYPH_PX];
  scene.items.forEach((item, i) => {
    if (!standing(item)) return;
    const sprite = sprites[i]!;
    const [left, top] = anchor(item, sprite);
    b = [
      Math.min(b[0], left),
      Math.min(b[1], top),
      Math.max(b[2], left + sprite.width),
      Math.max(b[3], top + sprite.height),
    ];
  });
  return b;
}

class Canvas {
  readonly img: Img;
  readonly ox: number;
  readonly oy: number;
  readonly hit: Int32Array<ArrayBuffer>;
  /** 1 + the index of the item being drawn, or 0 while drawing the room itself. */
  tag = 0;

  constructor(img: Img, ox: number, oy: number) {
    this.img = img;
    this.ox = ox;
    this.oy = oy;
    this.hit = new Int32Array(img.width * img.height);
  }

  private at(lx: number, ly: number): number {
    const x = lx + this.ox;
    const y = ly + this.oy;
    return x >= 0 && y >= 0 && x < this.img.width && y < this.img.height ? y * this.img.width + x : -1;
  }

  set(lx: number, ly: number, c: Rgba): void {
    const p = this.at(lx, ly);
    if (p < 0) return;
    const d = this.img.data;
    d[p * 4] = c[0];
    d[p * 4 + 1] = c[1];
    d[p * 4 + 2] = c[2];
    d[p * 4 + 3] = c[3];
    if (this.tag) this.hit[p] = this.tag;
  }

  /** A translucent colour over what is there: integer maths on the three colour channels. */
  blend(lx: number, ly: number, c: Rgba): void {
    if (c[3] === 255) {
      this.set(lx, ly, c);
      return;
    }
    const p = this.at(lx, ly);
    if (p < 0) return;
    const d = this.img.data;
    const a = c[3];
    for (let i = 0; i < 3; i++) d[p * 4 + i] = Math.floor((d[p * 4 + i]! * (255 - a) + c[i]! * a) / 255);
    if (this.tag) this.hit[p] = this.tag;
  }

  /** `imageops::overlay` at the scene position (left, top); remembers which pixels the sprite covers. */
  sprite(sprite: Img, left: number, top: number): void {
    overlay(this.img, sprite, left + this.ox, top + this.oy);
    if (!this.tag) return;
    for (let sy = 0; sy < sprite.height; sy++) {
      for (let sx = 0; sx < sprite.width; sx++) {
        if (sprite.data[(sy * sprite.width + sx) * 4 + 3] === 0) continue;
        const p = this.at(left + sx, top + sy);
        if (p >= 0) this.hit[p] = this.tag;
      }
    }
  }
}

function average(img: Img): Rgba {
  const sum = [0, 0, 0];
  let n = 0;
  for (let i = 0; i < img.data.length; i += 4) {
    if (img.data[i + 3]! === 0) continue;
    for (let c = 0; c < 3; c++) sum[c]! += img.data[i + c]!;
    n++;
  }
  n = Math.max(n, 1);
  return [Math.floor(sum[0]! / n), Math.floor(sum[1]! / n), Math.floor(sum[2]! / n), 255];
}

const mod = (a: number, n: number) => ((a % n) + n) % n;

/** Texture lookup that wraps in both directions. */
function texel(tex: Img, u: number, v: number): Rgba {
  const at = (mod(v, Math.max(tex.height, 1)) * tex.width + mod(u, Math.max(tex.width, 1))) * 4;
  return [tex.data[at]!, tex.data[at + 1]!, tex.data[at + 2]!, tex.data[at + 3]!];
}

const sideOf = (item: SceneItem): WallSide => (item.flipped ? "left" : "right");

export function renderRoom(
  scene: Scene,
  pass: "all" | "background" | "foreground" = "all",
  fixedBounds?: Box,
  underlay?: Img,
  transparentBackdrop = false,
): Rendered {
  // Floor, surface and figure sprites are mirrored when flipped; wall items hang on the other wall instead.
  const sprites = scene.items.map((item) =>
    item.flipped && standing(item) ? flipHorizontal(item.sprite) : item.sprite,
  );
  const [minX, minY, maxX, maxY] = fixedBounds ?? bounds(scene, sprites);
  const [cw, ch] = [maxX - minX + 2 * MARGIN, maxY - minY + 2 * MARGIN];
  const img = underlay ?? blank(cw, ch);
  if (pass !== "foreground" && !transparentBackdrop)
    for (let y = 0; y < ch; y++) {
      const t = f32(y / ch);
      const u = f32(1 - t);
      const mix = (a: number, b: number) => Math.trunc(f32(f32(a * u) + f32(b * t)));
      const c: Rgba = [mix(0x2a, 0x12), mix(0x2c, 0x14), mix(0x48, 0x20), 255];
      for (let x = 0; x < cw; x++) img.data.set(c, (y * cw + x) * 4);
    }
  const cv = new Canvas(img, MARGIN - minX, MARGIN - minY);

  if (pass !== "foreground") {
    drawWalls(cv, scene);
    drawFloor(cv, scene);
    scene.items.forEach((item, i) => {
      if (item.layer !== "rug") return;
      cv.tag = i + 1;
      drawRug(cv, item);
    });
    cv.tag = 0;
    if (scene.grid) drawGrid(cv, scene);
    scene.items.forEach((item, i) => {
      cv.tag = i + 1;
      drawShadow(cv, item);
    });
    scene.items.forEach((item, i) => {
      if (item.layer !== "wall") return;
      cv.tag = i + 1;
      drawWallItem(cv, item, sprites[i]!);
    });
  }
  if (pass === "all") {
    const on: number[] = [];
    scene.items.forEach((item, i) => standing(item) && on.push(i));
    for (const i of drawOrder(scene.items, on)) {
      const [left, top] = anchor(scene.items[i]!, sprites[i]!);
      cv.tag = i + 1;
      cv.sprite(sprites[i]!, left, top);
    }
  }
  cv.tag = 0;
  if (pass !== "background") {
    for (const item of scene.items) if (item.selected) outlineItem(cv, item, GOLD);
    if (scene.cursor) drawCursor(cv, scene.cursor);
    if (scene.grid) drawLabels(cv, scene);
  }
  return { image: img, hit: cv.hit };
}

function drawWalls(cv: Canvas, scene: Scene): void {
  const paper = scene.wallpaper;
  const cap = shade(average(paper), 1.18);
  const walls: [WallSide, number, number][] = [
    ["left", scene.depth, 1.0],
    ["right", scene.width, 0.84],
  ];
  for (const [side, len, light] of walls) {
    const span = len * 32;
    for (let m = 0; m < span; m++) {
      const lx = wallColumn(side, m);
      const base = floorTop(lx);
      const top = base - WALL_H;
      for (let ly = top; ly < base; ly++) {
        let c = shade(texel(paper, Math.floor(m / 2), Math.floor((ly - top) / 2)), light);
        if (base - ly <= 3) c = shade(c, 0.62);
        cv.set(lx, ly, c);
      }
      // Wall top: a light cap with an outline above it.
      cv.set(lx, top - 1, cap);
      cv.set(lx, top - 2, cap);
      cv.set(lx, top - 3, OUTLINE);
      if (m === span - 1) for (let ly = top - 3; ly < base; ly++) cv.set(lx, ly, OUTLINE);
    }
  }
  // The corner where the walls meet.
  for (let ly = floorTop(0) - WALL_H; ly < floorTop(0); ly++) cv.blend(0, ly, [0, 0, 0, 50]);
}

function drawFloor(cv: Canvas, scene: Scene): void {
  const [w, d] = [scene.width, scene.depth];
  const tex = scene.floor;
  const [tw, th] = [Math.max(tex.width, 1), Math.max(tex.height, 1)];
  const slab = average(tex);
  const frontX = 32 * (w - d);
  for (let lx = -32 * d - 1; lx < 32 * w + 1; lx++) {
    let bottom: number | null = null;
    for (let ly = 0; ly < 16 * (w + d) + 2; ly++) {
      const [a, b] = plane(lx, ly);
      if (a < 0 || b < 0 || a >= 64 * w || b >= 64 * d) continue;
      let c = texel(tex, Math.floor((a * tw) / 64), Math.floor((b * th) / 64));
      // Soft shade where the floor meets the walls.
      if (ly - floorTop(lx) < 3) c = shade(c, 0.8);
      cv.set(lx, ly, c);
      bottom = ly;
    }
    if (bottom !== null) {
      const f = lx < frontX ? 0.62 : 0.48;
      for (let ly = bottom + 1; ly <= bottom + SLAB; ly++) cv.set(lx, ly, shade(slab, f));
      cv.set(lx, bottom + SLAB + 1, OUTLINE);
    }
  }
}

/** Visits every screen pixel over a footprint with its floor-plane coordinates relative to the footprint origin. */
function eachFootprintPixel(
  x: number,
  y: number,
  w: number,
  d: number,
  visit: (lx: number, ly: number, a: number, b: number) => void,
): void {
  for (let lx = 32 * (x - y - d) - 1; lx <= 32 * (x + w - y); lx++) {
    for (let ly = 16 * (x + y) - 1; ly <= 16 * (x + y + w + d); ly++) {
      const [a, b] = plane(lx, ly);
      if (a >= 64 * x && a < 64 * (x + w) && b >= 64 * y && b < 64 * (y + d))
        visit(lx, ly, a - 64 * x, b - 64 * y);
    }
  }
}

/** Rugs are flat top-down textures projected onto their footprint. */
function drawRug(cv: Canvas, item: SceneItem): void {
  const tex = item.sprite;
  const [tw, th] = [Math.max(tex.width, 1), Math.max(tex.height, 1)];
  const [w, d] = [item.w, item.d];
  eachFootprintPixel(item.x, item.y, w, d, (lx, ly, a, b) => {
    // A flipped rug is turned a quarter: its own width now runs along y.
    const u = item.flipped ? Math.floor((b * tw) / (64 * d)) : Math.floor((a * tw) / (64 * w));
    const v = item.flipped ? Math.floor((a * th) / (64 * w)) : Math.floor((b * th) / (64 * d));
    const at = (Math.min(Math.max(v, 0), th - 1) * tex.width + Math.min(Math.max(u, 0), tw - 1)) * 4;
    const alpha = tex.data[at + 3]!;
    if (alpha > 0) cv.blend(lx, ly, [tex.data[at]!, tex.data[at + 1]!, tex.data[at + 2]!, alpha]);
  });
}

function drawGrid(cv: Canvas, scene: Scene): void {
  const [w, d] = [scene.width, scene.depth];
  eachFootprintPixel(0, 0, w, d, (lx, ly, a, b) => {
    if (a % 64 < 2 || b % 64 < 2) cv.blend(lx, ly, [255, 255, 255, 80]);
  });
  const walls: [WallSide, number][] = [
    ["left", d],
    ["right", w],
  ];
  for (const [side, len] of walls) {
    for (let m = 0; m < len * 32; m++) {
      const lx = wallColumn(side, m);
      const top = levelTop(lx);
      for (let ly = top; ly < floorTop(lx); ly++) {
        if (m % 32 === 0 || (ly - top) % LEVEL_H === 0) cv.blend(lx, ly, [255, 255, 255, 28]);
      }
    }
  }
}

/** 3×5 glyphs for the tile coordinates, one row per string, `#` for ink. */
const GLYPHS: Record<string, string[]> = {
  "0": ["###", "#.#", "#.#", "#.#", "###"],
  "1": [".#.", "##.", ".#.", ".#.", "###"],
  "2": ["##.", "..#", ".#.", "#..", "###"],
  "3": ["##.", "..#", ".#.", "..#", "##."],
  "4": ["#.#", "#.#", "###", "..#", "..#"],
  "5": ["###", "#..", "##.", "..#", "##."],
  "6": [".##", "#..", "###", "#.#", "###"],
  "7": ["###", "..#", ".#.", ".#.", ".#."],
  "8": ["###", "#.#", "###", "#.#", "###"],
  "9": ["###", "#.#", "###", "..#", "##."],
  A: [".#.", "#.#", "###", "#.#", "#.#"],
  B: ["##.", "#.#", "##.", "#.#", "##."],
  C: [".##", "#..", "#..", "#..", ".##"],
  D: ["##.", "#.#", "#.#", "#.#", "##."],
  E: ["###", "#..", "##.", "#..", "###"],
  F: ["###", "#..", "##.", "#..", "#.."],
  G: [".##", "#..", "#.#", "#.#", ".##"],
  H: ["#.#", "#.#", "###", "#.#", "#.#"],
  I: ["###", ".#.", ".#.", ".#.", "###"],
  J: ["..#", "..#", "..#", "#.#", ".#."],
  K: ["#.#", "#.#", "##.", "#.#", "#.#"],
  L: ["#..", "#..", "#..", "#..", "###"],
  M: ["#.#", "###", "###", "#.#", "#.#"],
  N: ["##.", "#.#", "#.#", "#.#", "#.#"],
};

/** Column letter of a floor tile's x: A, B, C… */
export function columnLetter(x: number): string {
  return String.fromCharCode(65 + (x % 26));
}

/** White text with a dark outline, centred on (cx, cy). */
function drawText(cv: Canvas, text: string, cx: number, cy: number): void {
  const chars = [...text].map((c) => GLYPHS[c]).filter((g): g is string[] => g !== undefined);
  const width = chars.length * 4 - 1;
  const left = cx - Math.trunc((width * GLYPH_PX) / 2);
  const top = cy - Math.trunc((5 * GLYPH_PX) / 2);
  const ink = (gx: number, gy: number): boolean => {
    const i = Math.floor(gx / 4);
    const col = mod(gx, 4);
    return col < 3 && gy >= 0 && gy < 5 && gx >= 0 && chars[i]?.[gy]?.[col] === "#";
  };
  for (let gy = -1; gy < 6; gy++) {
    for (let gx = -1; gx <= width; gx++) {
      const on = ink(gx, gy);
      let edge = false;
      if (!on)
        for (let dy = -1; dy <= 1 && !edge; dy++)
          for (let dx = -1; dx <= 1; dx++) edge ||= ink(gx + dx, gy + dy);
      if (!on && !edge) continue;
      const color: Rgba = on ? [0xff, 0xff, 0xff, 255] : OUTLINE;
      for (let py = 0; py < GLYPH_PX; py++) {
        for (let px = 0; px < GLYPH_PX; px++)
          cv.set(left + gx * GLYPH_PX + px, top + gy * GLYPH_PX + py, color);
      }
    }
  }
}

/**
 * Tile coordinates under the two front edges: letters for x along the left edge, numbers for y along
 * the right one, so a spot reads like a chess square ("C4").
 */
function drawLabels(cv: Canvas, scene: Scene): void {
  const [w, d] = [scene.width, scene.depth];
  const drop = SLAB + Math.trunc(LABEL_GAP / 2) + Math.trunc((5 * GLYPH_PX) / 2);
  const half = Math.trunc(LABEL_GAP / 2);
  for (let x = 0; x < w; x++) {
    const [mx, my] = [32 * (x - d) + 16, 16 * (x + d) + 8];
    drawText(cv, columnLetter(x), mx - half, my + drop);
  }
  for (let y = 0; y < d; y++) {
    const [mx, my] = [32 * (w - y) - 16, 16 * (w + y) + 8];
    drawText(cv, String(y + 1), mx + half, my + drop);
  }
}

function drawShadow(cv: Canvas, item: SceneItem): void {
  if (item.layer === "floor") {
    const [w, d] = [item.w, item.d];
    eachFootprintPixel(item.x, item.y, w, d, (lx, ly, a, b) => {
      if (a > 4 && b > 4 && a < 64 * w - 4 && b < 64 * d - 4) cv.blend(lx, ly, [0, 0, 0, 44]);
    });
  } else if (item.layer === "figure") {
    const cx = 32 * (item.x - item.y);
    const cy = 16 * (item.x + item.y) + 20;
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -14; dx <= 14; dx++) {
        if (dx * dx * 16 + dy * dy * 196 <= 14 * 14 * 16) cv.blend(cx + dx, cy + dy, [0, 0, 0, 60]);
      }
    }
  }
}

/** Wall items are drawn flat and sheared onto the wall, one pixel down every two across. */
function drawWallItem(cv: Canvas, item: SceneItem, sprite: Img): void {
  const side = sideOf(item);
  // Centre the art in its cells if it is a little smaller than them.
  const dx = Math.floor(Math.max(item.w * 32 - sprite.width, 0) / 2);
  const dy = Math.floor(Math.max(item.d * LEVEL_H - sprite.height, 0) / 2);
  for (let sy = 0; sy < sprite.height; sy++) {
    for (let sx = 0; sx < sprite.width; sx++) {
      const at = (sy * sprite.width + sx) * 4;
      const alpha = sprite.data[at + 3]!;
      if (alpha === 0) continue;
      const m = item.x * 32 + dx + sx;
      const lx = wallColumn(side, m);
      const ly = levelTop(lx) + item.y * LEVEL_H + dy + sy;
      cv.blend(lx, ly, [sprite.data[at]!, sprite.data[at + 1]!, sprite.data[at + 2]!, alpha]);
    }
  }
}

function wallCells(
  cv: Canvas,
  side: WallSide,
  x: number,
  y: number,
  w: number,
  d: number,
  color: Rgba,
  fill: number,
): void {
  const [m0, m1] = [x * 32, (x + w) * 32];
  for (let m = m0; m < m1; m++) {
    const lx = wallColumn(side, m);
    const top = levelTop(lx) + y * LEVEL_H;
    const bottom = top + d * LEVEL_H;
    for (let ly = top; ly < bottom; ly++) {
      if (m === m0 || m === m1 - 1 || ly === top || ly === bottom - 1) cv.set(lx, ly, color);
      else if (fill > 0) cv.blend(lx, ly, [color[0], color[1], color[2], fill]);
    }
  }
}

function floorCells(cv: Canvas, x: number, y: number, w: number, d: number, color: Rgba, fill: number): void {
  eachFootprintPixel(x, y, w, d, (lx, ly, a, b) => {
    if (a < 2 || b < 2 || a >= 64 * w - 2 || b >= 64 * d - 2) cv.set(lx, ly, color);
    else if (fill > 0) cv.blend(lx, ly, [color[0], color[1], color[2], fill]);
  });
}

function outlineItem(cv: Canvas, item: SceneItem, color: Rgba): void {
  if (item.layer === "wall") wallCells(cv, sideOf(item), item.x, item.y, item.w, item.d, color, 0);
  else floorCells(cv, item.x, item.y, item.w, item.d, color, 0);
}

function drawCursor(cv: Canvas, c: Cursor): void {
  const color = c.ok ? OK : BAD;
  if (c.wall) wallCells(cv, c.wall, c.x, c.y, c.w, c.d, color, 70);
  else floorCells(cv, c.x, c.y, c.w, c.d, color, 70);
}
