import { deflateSync } from "node:zlib";

import { describe, expect, it } from "vitest";

import { decodePng } from "./png";

/** A PNG built by hand: the signature, a header, one data chunk and the end. */
function png(
  width: number,
  height: number,
  colorType: number,
  rows: number[][],
  extra: [string, number[]][] = [],
) {
  const crc = new Uint8Array(4); // the reader does not check checksums
  const chunk = (type: string, body: number[]) => {
    const out = new Uint8Array(12 + body.length);
    new DataView(out.buffer).setUint32(0, body.length);
    out.set(
      [...type].map((c) => c.charCodeAt(0)),
      4,
    );
    out.set(body, 8);
    out.set(crc, 8 + body.length);
    return out;
  };
  const ihdr = new Uint8Array(13);
  const view = new DataView(ihdr.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  ihdr.set([8, colorType, 0, 0, 0], 8);
  const data = deflateSync(Uint8Array.from(rows.flat()));
  const parts = [
    Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", [...ihdr]),
    ...extra.map(([type, body]) => chunk(type, body)),
    chunk("IDAT", [...data]),
    chunk("IEND", []),
  ];
  return Uint8Array.from(parts.flatMap((p) => [...p]));
}

describe("decodePng", () => {
  it("reads RGBA rows with every filter", async () => {
    const [width, height, bpp] = [3, 10, 4];
    // Ten rows of made-up bytes; each row is filtered the way a PNG writer would, cycling through the five filters.
    const raw = Array.from({ length: height }, (_, y) =>
      Array.from({ length: width * bpp }, (_, i) => (y * 37 + i * 91 + (i % 3) * 53) % 256),
    );
    const paeth = (a: number, b: number, c: number) => {
      const p = a + b - c;
      const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
      return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
    };
    const rows = raw.map((row, y) => {
      const type = y % 5;
      const out = row.map((v, i) => {
        const left = i >= bpp ? row[i - bpp]! : 0;
        const up = y > 0 ? raw[y - 1]![i]! : 0;
        const corner = y > 0 && i >= bpp ? raw[y - 1]![i - bpp]! : 0;
        const guess = [0, left, up, (left + up) >> 1, paeth(left, up, corner)][type]!;
        return (v - guess + 256) % 256;
      });
      return [type, ...out];
    });
    const img = await decodePng(png(width, height, 6, rows));
    expect([img.width, img.height]).toEqual([width, height]);
    expect([...img.data]).toEqual(raw.flat());
  });

  it("reads palette images with see-through entries", async () => {
    const img = await decodePng(
      png(
        2,
        1,
        3,
        [[0, 0, 1]],
        [
          ["PLTE", [255, 0, 0, 0, 255, 0]],
          ["tRNS", [0]],
        ],
      ),
    );
    expect([...img.data]).toEqual([255, 0, 0, 0, 0, 255, 0, 255]);
  });

  it("reads RGB images as opaque", async () => {
    const img = await decodePng(png(1, 1, 2, [[0, 1, 2, 3]]));
    expect([...img.data]).toEqual([1, 2, 3, 255]);
  });

  it("refuses what it cannot read", async () => {
    await expect(decodePng(Uint8Array.from([1, 2, 3, 4, 5, 6, 7, 8, 9]))).rejects.toThrow(/signature/);
  });
});
