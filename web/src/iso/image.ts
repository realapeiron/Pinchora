// Pixel buffers and the handful of image operations the room renderer uses, written to give the same
// pixels as the `image` crate (0.25) does for the same calls. No canvas is involved, so the same code
// runs in the browser and under vitest.

/** An RGBA image, not premultiplied. */
export interface Img {
  width: number;
  height: number;
  data: Uint8ClampedArray<ArrayBuffer>;
}

export type Rgba = readonly [number, number, number, number];

const f32 = Math.fround;

export function blank(width: number, height: number): Img {
  return { width, height, data: new Uint8ClampedArray(width * height * 4) };
}

export function clone(src: Img): Img {
  return { width: src.width, height: src.height, data: src.data.slice() };
}

/** `image::imageops::crop_imm(..).to_image()`: the rectangle at (x, y), which must lie inside `src`. */
export function crop(src: Img, x: number, y: number, w: number, h: number): Img {
  const out = blank(w, h);
  for (let row = 0; row < h; row++) {
    const from = ((y + row) * src.width + x) * 4;
    out.data.set(src.data.subarray(from, from + w * 4), row * w * 4);
  }
  return out;
}

/** `imageops::flip_horizontal`. */
export function flipHorizontal(src: Img): Img {
  const { width, height } = src;
  const out = blank(width, height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = (y * width + x) * 4;
      const to = (y * width + (width - 1 - x)) * 4;
      out.data.set(src.data.subarray(from, from + 4), to);
    }
  }
  return out;
}

/** `imageops::rotate90`: a quarter turn clockwise. */
export function rotate90(src: Img): Img {
  const { width, height } = src;
  const out = blank(height, width);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = (y * width + x) * 4;
      const to = (x * height + (height - 1 - y)) * 4;
      out.data.set(src.data.subarray(from, from + 4), to);
    }
  }
  return out;
}

/** `imageops::rotate270`: a quarter turn counter-clockwise. */
export function rotate270(src: Img): Img {
  const { width, height } = src;
  const out = blank(height, width);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = (y * width + x) * 4;
      const to = ((width - 1 - x) * height + y) * 4;
      out.data.set(src.data.subarray(from, from + 4), to);
    }
  }
  return out;
}

/** `imageops::resize(.., Nearest)` to a whole multiple of the size. */
export function scaleNearest(src: Img, factorX: number, factorY: number): Img {
  const out = blank(src.width * factorX, src.height * factorY);
  for (let y = 0; y < out.height; y++) {
    const sy = Math.floor(y / factorY);
    for (let x = 0; x < out.width; x++) {
      const from = (sy * src.width + Math.floor(x / factorX)) * 4;
      out.data.set(src.data.subarray(from, from + 4), (y * out.width + x) * 4);
    }
  }
  return out;
}

/**
 * `Rgba::blend`: lays one pixel over the pixel at `at` the way the `image` crate does, in f32 maths
 * with the result cut to bytes (not rounded), so even see-through pixels land on the same value.
 */
export function blendOver(
  d: Uint8ClampedArray,
  at: number,
  r: number,
  g: number,
  b: number,
  a: number,
): void {
  if (a === 0) return;
  if (a === 255) {
    d[at] = r;
    d[at + 1] = g;
    d[at + 2] = b;
    d[at + 3] = 255;
    return;
  }
  const bgR = f32(d[at]! / 255);
  const bgG = f32(d[at + 1]! / 255);
  const bgB = f32(d[at + 2]! / 255);
  const bgA = f32(d[at + 3]! / 255);
  const fgR = f32(r / 255);
  const fgG = f32(g / 255);
  const fgB = f32(b / 255);
  const fgA = f32(a / 255);
  const alpha = f32(f32(bgA + fgA) - f32(bgA * fgA));
  if (alpha === 0) return;
  const keep = f32(1 - fgA);
  const out = (bg: number, fg: number) => {
    const premultiplied = f32(f32(fg * fgA) + f32(f32(bg * bgA) * keep));
    return Math.trunc(f32(255 * f32(premultiplied / alpha)));
  };
  d[at] = out(bgR, fgR);
  d[at + 1] = out(bgG, fgG);
  d[at + 2] = out(bgB, fgB);
  d[at + 3] = Math.trunc(f32(255 * alpha));
}

/** `imageops::overlay`: `top` laid over `bottom` with its corner at (x, y), clipped to `bottom`. */
export function overlay(bottom: Img, top: Img, x: number, y: number): void {
  if (x > bottom.width || y > bottom.height || x + top.width <= 0 || y + top.height <= 0) return;
  const x0 = Math.min(Math.max(x, 0), bottom.width);
  const y0 = Math.min(Math.max(y, 0), bottom.height);
  const x1 = Math.min(Math.max(x + top.width, 0), bottom.width);
  const y1 = Math.min(Math.max(y + top.height, 0), bottom.height);
  const tx0 = Math.min(Math.max(-x, 0), top.width);
  const ty0 = Math.min(Math.max(-y, 0), top.height);
  for (let by = y0; by < y1; by++) {
    for (let bx = x0; bx < x1; bx++) {
      const from = ((ty0 + by - y0) * top.width + tx0 + bx - x0) * 4;
      blendOver(
        bottom.data,
        (by * bottom.width + bx) * 4,
        top.data[from]!,
        top.data[from + 1]!,
        top.data[from + 2]!,
        top.data[from + 3]!,
      );
    }
  }
}

/** FNV-1a over the pixels, with the colour of see-through pixels ignored. Matches the Rust fixture writer. */
export function fingerprint(img: Img): string {
  let hash = 0x811c9dc5;
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const clear = d[i + 3] === 0;
    for (let c = 0; c < 4; c++) {
      hash = Math.imul(hash ^ (clear ? 0 : d[i + c]!), 0x01000193) >>> 0;
    }
  }
  return hash.toString(16).padStart(8, "0");
}
