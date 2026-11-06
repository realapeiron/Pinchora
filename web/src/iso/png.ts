// A small PNG reader for the sprite atlas and the test fixtures: 8-bit grey, RGB, palette and RGBA,
// no interlacing. It gives the exact bytes in the file (a canvas would round see-through pixels), and
// runs in Node as well as in the browser, so the tests exercise the code the page uses.

import type { Img } from "./image";

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

async function inflate(bytes: Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

/** Undoes the per-row filters in place; returns the rows as one buffer of `height * stride` bytes. */
function unfilter(raw: Uint8Array, height: number, stride: number, bpp: number): Uint8Array {
  const out = new Uint8Array(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)]!;
    const src = y * (stride + 1) + 1;
    const dst = y * stride;
    const up = dst - stride;
    for (let i = 0; i < stride; i++) {
      const x = raw[src + i]!;
      const left = i >= bpp ? out[dst + i - bpp]! : 0;
      const above = y > 0 ? out[up + i]! : 0;
      const corner = y > 0 && i >= bpp ? out[up + i - bpp]! : 0;
      let v: number;
      switch (filter) {
        case 0:
          v = x;
          break;
        case 1:
          v = x + left;
          break;
        case 2:
          v = x + above;
          break;
        case 3:
          v = x + ((left + above) >> 1);
          break;
        case 4:
          v = x + paeth(left, above, corner);
          break;
        default:
          throw new Error(`PNG: unknown row filter ${filter}`);
      }
      out[dst + i] = v & 255;
    }
  }
  return out;
}

export async function decodePng(file: Uint8Array): Promise<Img> {
  if (SIGNATURE.some((b, i) => file[i] !== b)) throw new Error("PNG: bad signature");
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  let width = 0;
  let height = 0;
  let colorType = -1;
  let palette: Uint8Array | null = null;
  let alpha: Uint8Array | null = null;
  const parts: Uint8Array[] = [];
  let at = 8;
  while (at + 8 <= file.length) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(file[at + 4]!, file[at + 5]!, file[at + 6]!, file[at + 7]!);
    const body = file.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = view.getUint32(at + 8);
      height = view.getUint32(at + 12);
      const depth = file[at + 16];
      colorType = file[at + 17]!;
      if (depth !== 8 || file[at + 20] !== 0) throw new Error("PNG: only 8-bit, non-interlaced images");
    } else if (type === "PLTE") palette = body;
    else if (type === "tRNS") alpha = body;
    else if (type === "IDAT") parts.push(body);
    else if (type === "IEND") break;
    at += 12 + length;
  }
  const channels = { 0: 1, 2: 3, 3: 1, 6: 4 }[colorType];
  if (channels === undefined) throw new Error(`PNG: unsupported colour type ${colorType}`);
  const joined = new Uint8Array(new ArrayBuffer(parts.reduce((n, p) => n + p.length, 0)));
  let offset = 0;
  for (const p of parts) {
    joined.set(p, offset);
    offset += p.length;
  }
  const rows = unfilter(await inflate(joined), height, width * channels, channels);
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0, n = width * height; i < n; i++) {
    const o = i * 4;
    const s = i * channels;
    switch (colorType) {
      case 6:
        data[o] = rows[s]!;
        data[o + 1] = rows[s + 1]!;
        data[o + 2] = rows[s + 2]!;
        data[o + 3] = rows[s + 3]!;
        break;
      case 2:
        data[o] = rows[s]!;
        data[o + 1] = rows[s + 1]!;
        data[o + 2] = rows[s + 2]!;
        data[o + 3] = 255;
        break;
      case 0:
        data[o] = data[o + 1] = data[o + 2] = rows[s]!;
        data[o + 3] = 255;
        break;
      default: {
        const index = rows[s]!;
        data[o] = palette?.[index * 3] ?? 0;
        data[o + 1] = palette?.[index * 3 + 1] ?? 0;
        data[o + 2] = palette?.[index * 3 + 2] ?? 0;
        data[o + 3] = alpha?.[index] ?? 255;
      }
    }
  }
  return { width, height, data };
}
