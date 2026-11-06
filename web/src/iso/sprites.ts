// Sprite helpers of `pinchora-render/src/sprites.rs`: the palette swap for variants, the stand-ins drawn
// until real art exists, and the sleeping pet. They run in f32 maths like the Rust, so a stand-in looks
// the same in both places.

import {
  type Img,
  type Rgba,
  blank,
  clone,
  crop,
  flipHorizontal,
  overlay,
  rotate270,
  rotate90,
} from "./image";
import { LEVEL_H } from "./projection";
import type { Ramp } from "./content";
import type { SceneLayer } from "./scene";

const f32 = Math.fround;

const OUTLINE: Rgba = [0x1b, 0x1d, 0x2b, 255];

/** Wallpaper and floor textures are square tiles of this size. */
const TEX = 16;

/** The coral ramp every base sprite uses for its main colour; variants replace it. */
export const BASE_RAMP: Ramp = [
  [0x7a, 0x2a, 0x3a],
  [0xc2, 0x41, 0x3f],
  [0xff, 0x6b, 0x4a],
  [0xff, 0x9e, 0x7a],
  [0xff, 0xd2, 0xc0],
];

/** FNV-1a: stable across runs, so placeholders never change look. */
export function hash(s: string): bigint {
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(s)) {
    h = BigInt.asUintN(64, (h ^ BigInt(b)) * 0x100000001b3n);
  }
  return h;
}

/** Swaps the base ramp for `ramp`; every other colour is kept. */
export function recolor(sprite: Img, ramp: Ramp): Img {
  const out = clone(sprite);
  const d = out.data;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] === 0) continue;
    const at = BASE_RAMP.findIndex((c) => c[0] === d[i] && c[1] === d[i + 1] && c[2] === d[i + 2]);
    const to = ramp[at];
    if (to) {
      d[i] = to[0];
      d[i + 1] = to[1];
      d[i + 2] = to[2];
    }
  }
  return out;
}

/** `shade`: each channel times `f` (an f32), cut to a byte, opaque. */
export function shade(c: readonly number[], f: number): [number, number, number, number] {
  const k = f32(f);
  const one = (v: number) => Math.trunc(Math.min(f32(v * k), 255));
  return [one(c[0]!), one(c[1]!), one(c[2]!), 255];
}

function put(img: Img, x: number, y: number, c: Rgba): void {
  const at = (y * img.width + x) * 4;
  img.data[at] = c[0];
  img.data[at + 1] = c[1];
  img.data[at + 2] = c[2];
  img.data[at + 3] = c[3];
}

/**
 * A stand-in for furniture art, drawn in the base ramp so variants recolor it like real art: an
 * isometric box for floor and surface items, a framed panel for wall items and a striped flat mat for
 * rugs (rugs and wall art are flat; the renderer projects them).
 */
export function placeholderItem(
  id: string,
  w: number,
  d: number,
  layer: SceneLayer,
  isoWidth: number | null,
): Img {
  if (layer === "rug") {
    const [pw, ph] = [w * 32, d * 32];
    const img = blank(pw, ph);
    for (let y = 0; y < ph; y++) {
      for (let x = 0; x < pw; x++) {
        if (x === 0 || y === 0 || x === pw - 1 || y === ph - 1) put(img, x, y, OUTLINE);
        else if ((Math.floor(x / 6) + Math.floor(y / 6)) % 2 === 0) put(img, x, y, shade(BASE_RAMP[3]!, 1));
        else put(img, x, y, shade(BASE_RAMP[2]!, 1));
      }
    }
    return img;
  }
  if (layer === "wall") {
    const [pw, ph] = [w * 32 - 4, d * LEVEL_H - 4];
    const img = blank(pw, ph);
    for (let y = 0; y < ph; y++) {
      for (let x = 0; x < pw; x++) {
        const edge = Math.min(x, y, pw - 1 - x, ph - 1 - y);
        put(img, x, y, edge === 0 ? OUTLINE : edge <= 2 ? shade(BASE_RAMP[1]!, 1) : shade(BASE_RAMP[3]!, 1));
      }
    }
    return img;
  }
  const full = (w + d) * 32;
  const wanted = isoWidth ?? (layer === "surface" ? 24 : full);
  const sw = Math.min(Math.max(wanted, 8), full);
  const k = f32(sw / full);
  const h = hash(id);
  const tall = layer === "surface" ? 10 + Number(h % 8n) : 18 + Number(h % 34n);
  const height = Math.trunc(f32(f32(tall) * Math.max(k, 0.5)));
  return isoBox(w, d, sw, height);
}

/** An outlined isometric box with a `w`×`d` footprint, `sw` pixels wide and `height` tall. */
function isoBox(w: number, d: number, sw: number, height: number): Img {
  const k = f32(sw / ((w + d) * 32));
  const sh = Math.trunc(f32(f32(16 * (w + d)) * k)) + height + 1;
  const img = blank(sw, sh);
  const [wf, df, hf] = [f32(w), f32(d), f32(height / k)];
  for (let py = 0; py < sh; py++) {
    for (let px = 0; px < sw; px++) {
      // Back to footprint units, with the left corner at x = 0.
      const u = f32(px / k);
      const v = f32(py / k);
      const a = f32(f32(u - f32(32 * df)) + f32(2 * v));
      const b = f32(f32(f32(2 * v) - u) + f32(32 * df));
      const top = a >= 0 && b >= 0 && a < f32(64 * wf) && b < f32(64 * df);
      const edge =
        u < f32(32 * wf)
          ? f32(f32(16 * df) + f32(u / 2))
          : f32(f32(16 * f32(wf + df)) - f32(f32(u - f32(32 * wf)) / 2));
      const body = v >= edge && v < f32(edge + hf);
      if (top) put(img, px, py, shade(BASE_RAMP[3]!, 1));
      else if (body && u < f32(32 * wf)) put(img, px, py, shade(BASE_RAMP[2]!, 1));
      else if (body) put(img, px, py, shade(BASE_RAMP[1]!, 1));
    }
  }
  // Outline every pixel that borders the background.
  const solid = clone(img);
  for (let y = 0; y < sh; y++) {
    for (let x = 0; x < sw; x++) {
      if (img.data[(y * sw + x) * 4 + 3] === 0) continue;
      const open = (dx: number, dy: number) => {
        const [nx, ny] = [x + dx, y + dy];
        return nx < 0 || ny < 0 || nx >= sw || ny >= sh || solid.data[(ny * sw + nx) * 4 + 3] === 0;
      };
      if (open(-1, 0) || open(1, 0) || open(0, -1) || open(0, 1)) put(img, x, y, OUTLINE);
    }
  }
  return img;
}

/** A 16×16 stand-in tile: planks for floors, stripes for wallpapers. */
export function placeholderTile(id: string, wall: boolean): Img {
  const palette = [
    [0xb9, 0x7f, 0x4a],
    [0x8f, 0xa0, 0xd8],
    [0xe6, 0xd3, 0xc3],
    [0x5b, 0xbf, 0x6a],
    [0xff, 0x9e, 0x7a],
    [0x9b, 0x6b, 0xc4],
  ];
  const base = palette[Number(hash(id) % BigInt(palette.length))]!;
  const img = blank(TEX, TEX);
  for (let y = 0; y < TEX; y++) {
    for (let x = 0; x < TEX; x++) {
      let f: number;
      if (wall) f = x % 8 < 4 ? 1 : 0.9;
      else f = y % 8 === 7 || (x + Math.floor(y / 8) * 8) % 16 === 0 ? 0.7 : 1;
      put(img, x, y, shade(base, f));
    }
  }
  return img;
}

const PET_ROWS = [
  "....####....",
  "..#oooooo#..",
  ".#oooooooo#.",
  "#oo##oo##oo#",
  "#oo#wo#woo.#",
  "#oooooooooo#",
  "#oooo##oooo#",
  ".#oooooooo#.",
  "..##.##.##..",
];

/** A small round critter with eyes, tinted per species, until real pet art exists. */
export function placeholderPet(id: string): Img {
  const colors = [
    [0xff, 0x6b, 0x4a],
    [0x06, 0xd6, 0xa0],
    [0xff, 0xd1, 0x66],
    [0x9b, 0x6b, 0xc4],
    [0x3a, 0x9b, 0xdc],
    [0xff, 0x8f, 0xb1],
  ];
  const c = colors[Number(hash(id) % BigInt(colors.length))]!;
  const small = blank(12, PET_ROWS.length);
  PET_ROWS.forEach((row, y) => {
    [...row].forEach((ch, x) => {
      if (ch === "#") put(small, x, y, OUTLINE);
      else if (ch === "o") put(small, x, y, shade(c, 1));
      else if (ch === "w") put(small, x, y, [0xff, 0xf1, 0xe8, 255]);
    });
  });
  return scaleBy(small, 3);
}

function scaleBy(src: Img, factor: number): Img {
  const out = blank(src.width * factor, src.height * factor);
  for (let y = 0; y < out.height; y++) {
    for (let x = 0; x < out.width; x++) {
      const from = (Math.floor(y / factor) * src.width + Math.floor(x / factor)) * 4;
      out.data.set(src.data.subarray(from, from + 4), (y * out.width + x) * 4);
    }
  }
  return out;
}

/** Pet art drawn for the old 16 px tiles is doubled so it matches the 64 px rooms. */
export function petSprite(drawn: Img | null, species: string): Img {
  const sprite = drawn ?? placeholderPet(species);
  return sprite.width < 28 ? scaleBy(sprite, 2) : sprite;
}

/** A small "z", and a bigger one: the snore drawn above a sleeping pet. */
const ZZ = [
  ["#####", "...#.", "..#..", ".#...", "#####"],
  ["#######", ".....#.", "....#..", "...#...", "..#....", ".#.....", "#######"],
];
/** Rows above the pet the "z z" gets. */
const ZZ_ROOM = 17;

/**
 * `sprite` as a pet asleep, facing left or right: an upright one (taller than wide) lies on its side, a
 * wide one flattens to two thirds of its height, both a little dimmed and trimmed to their pixels so they
 * rest on the floor, with a pixel "z z" rising above.
 */
export function sleepingPet(sprite: Img, left: boolean): Img {
  const upright = sprite.height >= sprite.width;
  const turned = upright ? (left ? rotate270(sprite) : rotate90(sprite)) : clone(sprite);
  let [x0, y0, x1, y1] = [turned.width, turned.height, -1, -1];
  for (let y = 0; y < turned.height; y++) {
    for (let x = 0; x < turned.width; x++) {
      if (turned.data[(y * turned.width + x) * 4 + 3]! > 0) {
        [x0, y0, x1, y1] = [Math.min(x0, x), Math.min(y0, y), Math.max(x1, x), Math.max(y1, y)];
      }
    }
  }
  let body = x1 < x0 ? turned : crop(turned, x0, y0, x1 - x0 + 1, y1 - y0 + 1);
  if (!upright) {
    body = flatten(body);
    if (left) body = flipHorizontal(body);
  }
  closeEyes(body);
  for (let i = 0; i < body.data.length; i += 4) {
    if (body.data[i + 3]! > 0) {
      for (let c = 0; c < 3; c++) body.data[i + c] = Math.floor((body.data[i + c]! * 7) / 8);
    }
  }
  const width = Math.max(body.width, 24);
  const out = blank(width, body.height + ZZ_ROOM);
  overlay(out, body, Math.floor((width - body.width) / 2), ZZ_ROOM);
  // The small one low by the head, the big one high and away from it: the snore leaves the head, so it
  // is mirrored with the way the pet lies. The art faces left, which turning an upright pet to the left
  // keeps (its top is its head) and mirroring a wide one to the left undoes.
  const middle = Math.floor(width / 2);
  const headLeft = upright === left;
  const places: [number, number][] = headLeft
    ? [
        [Math.max(middle - 9, 0), 10],
        [middle + 1, 1],
      ]
    : [
        [middle + 4, 10],
        [Math.max(middle - 8, 0), 1],
      ];
  drawGlyph(out, ZZ[0]!, places[0]![0], places[0]![1]);
  drawGlyph(out, ZZ[1]!, places[1]![0], places[1]![1]);
  return out;
}

/** The most pixels an eye can have: darker, bigger shapes inside a pet are markings. */
const EYE_MAX = 12;

/**
 * Closes the eyes of a sleeper: an eye is a small dark patch with body all around it (the dark pixels
 * that touch the outline or the edge of the picture are the outline), and a closed one is only its
 * bottom row, the rest painted over with the body beside it. A patch one row high is shut already.
 */
function closeEyes(img: Img): void {
  const [w, h] = [img.width, img.height];
  const alpha = (x: number, y: number) => img.data[(y * w + x) * 4 + 3]!;
  const dark = (x: number, y: number) => {
    if (x < 0 || y < 0 || x >= w || y >= h) return false;
    const at = (y * w + x) * 4;
    return img.data[at + 3] === 255 && Math.max(img.data[at]!, img.data[at + 1]!, img.data[at + 2]!) < 80;
  };
  const seen = new Array<boolean>(w * h).fill(false);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (seen[y * w + x] || !dark(x, y)) continue;
      // The patch this pixel is in, and whether it stays clear of the outline.
      const patch: [number, number][] = [];
      const stack: [number, number][] = [[x, y]];
      let inside = true;
      seen[y * w + x] = true;
      for (let from = stack.pop(); from; from = stack.pop()) {
        const [px, py] = from;
        patch.push([px, py]);
        for (let dx = -1; dx <= 1; dx++) {
          for (let dy = -1; dy <= 1; dy++) {
            const [nx, ny] = [px + dx, py + dy];
            if (nx < 0 || ny < 0 || nx >= w || ny >= h || alpha(nx, ny) < 255) {
              inside = false;
            } else if (dark(nx, ny) && !seen[ny * w + nx]) {
              seen[ny * w + nx] = true;
              stack.push([nx, ny]);
            }
          }
        }
      }
      let [top, bottom] = [patch[0]![1], patch[0]![1]];
      for (const [, py] of patch) [top, bottom] = [Math.min(top, py), Math.max(bottom, py)];
      if (!inside || patch.length > EYE_MAX || top === bottom) continue;
      for (const [px, py] of patch.filter((p) => p[1] < bottom)) {
        // The body above the patch, or failing that beside it.
        const near = (
          [
            [px, top - 1],
            [px - 1, py],
            [px + 1, py],
          ] as [number, number][]
        ).find(([fx, fy]) => fx >= 0 && fy >= 0 && fx < w && fy < h && !dark(fx, fy));
        if (near) {
          const [from, to] = [(near[1] * w + near[0]) * 4, (py * w + px) * 4];
          img.data.copyWithin(to, from, from + 4);
        }
      }
    }
  }
}

/** `sprite` without every third row: two thirds as tall, its pixels as sharp as before. */
function flatten(sprite: Img): Img {
  const kept: number[] = [];
  for (let y = 0; y < sprite.height; y++) if (y % 3 !== 2) kept.push(y);
  const out = blank(sprite.width, kept.length);
  kept.forEach((from, y) => {
    out.data.set(
      sprite.data.subarray(from * sprite.width * 4, (from + 1) * sprite.width * 4),
      y * sprite.width * 4,
    );
  });
  return out;
}

/** `glyph` (`#` pixels) in a soft blue with a dark outline, its top-left corner at (x, y). */
function drawGlyph(img: Img, glyph: string[], x: number, y: number): void {
  const lit = (gx: number, gy: number) => glyph[gy]?.[gx] === "#";
  const [w, h] = [glyph[0]!.length, glyph.length];
  for (let gy = -1; gy <= h; gy++) {
    for (let gx = -1; gx <= w; gx++) {
      let near = false;
      for (let dy = -1; dy <= 1 && !near; dy++)
        for (let dx = -1; dx <= 1; dx++) near ||= lit(gx + dx, gy + dy);
      const [px, py] = [x + gx, y + gy];
      if (!near || px < 0 || py < 0 || px >= img.width || py >= img.height) continue;
      put(img, px, py, lit(gx, gy) ? [0xcf, 0xd8, 0xff, 255] : OUTLINE);
    }
  }
}
