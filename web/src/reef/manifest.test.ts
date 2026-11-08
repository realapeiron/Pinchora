import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { beforeAll, expect, it } from "vitest";
import { decodePng } from "../iso/png";
import { crop, type Img } from "../iso/image";
import { validateManifest, type AssetManifest } from "./manifest";
let manifest: AssetManifest;
const dir = new URL("../../", import.meta.url);
beforeAll(() => {
  execFileSync("python3", [new URL("scripts/build-reef.py", dir).pathname]);
  manifest = JSON.parse(readFileSync(new URL("public/assets/reef/manifest.json", dir), "utf8"));
}, 120000);
it("validates all original sprites and 24 complete animation sheets", () => {
  expect(validateManifest(manifest)).toBe(manifest);
  expect(Object.keys(manifest.looks)).toHaveLength(24);
  expect(Object.keys(manifest.sprites).length).toBeGreaterThanOrEqual(952);
});
it("every repacked crop is RGBA-identical to the source atlas", async () => {
  const atlas = await decodePng(new Uint8Array(readFileSync(new URL("../assets/sprites/atlas.png", dir))));
  const index: Record<string, { x: number; y: number; w: number; h: number }> = JSON.parse(
    readFileSync(new URL("../assets/sprites/atlas.json", dir), "utf8"),
  );
  const pages = new Map<string, Img>();
  for (const [name, e] of Object.entries(manifest.sprites)) {
    let page = pages.get(e.page);
    if (!page) {
      page = await decodePng(
        new Uint8Array(readFileSync(new URL(`public${manifest.pages[e.page]!.url}`, dir))),
      );
      pages.set(e.page, page);
    }
    const source = index[name]!;
    expect(
      Buffer.from(crop(page, ...e.rect).data).equals(
        Buffer.from(crop(atlas, source.x, source.y, source.w, source.h).data),
      ),
      name,
    ).toBe(true);
  }
}, 120000);
it("rejects malformed crops, versions and missing directions", () => {
  const m = structuredClone(manifest);
  m.sprites[Object.keys(m.sprites)[0]!]!.rect[0] = -1;
  expect(() => validateManifest(m)).toThrow("crop");
  const l = structuredClone(manifest);
  l.looks.bobbin!.directions.SW = "missing";
  expect(() => validateManifest(l)).toThrow("row");
});
it("every directional row preserves the supplied sheet pixels", async () => {
  const { readdirSync } = await import("node:fs");
  const sourceDir = new URL("../assets/reef/looks/", dir);
  for (const file of readdirSync(sourceDir).filter((n) => n.endsWith("_metadata.json"))) {
    const meta = JSON.parse(readFileSync(new URL(file, sourceDir), "utf8")) as {
      look: string;
      rows: string[];
    };
    const source = await decodePng(
      new Uint8Array(readFileSync(new URL(file.replace("_metadata.json", "_sheet.png"), sourceDir))),
    );
    const look = manifest.looks[meta.look]!;
    for (const [i, d] of ["SW", "SE", "NW", "NE"].entries()) {
      const page = manifest.pages[look.directions[d as keyof typeof look.directions]]!;
      const image = await decodePng(new Uint8Array(readFileSync(new URL(`public${page.url}`, dir))));
      expect(
        Buffer.from(image.data).equals(Buffer.from(crop(source, 0, i * 128, 768, 128).data)),
        `${meta.look}/${d}`,
      ).toBe(true);
    }
  }
}, 30000);
it("compact seat contacts preserve every source frame's animation anchors", async () => {
  const { readdirSync } = await import("node:fs");
  const sourceDir = new URL("../assets/reef/looks/", dir);
  let omitted = 0;
  for (const file of readdirSync(sourceDir).filter((n) => n.endsWith("_metadata.json"))) {
    const source = JSON.parse(readFileSync(new URL(file, sourceDir), "utf8"));
    for (const frame of manifest.looks[source.look]!.frames) {
      const original = source.frames.find(
        (f: { direction: string; pose: string }) => f.direction === frame.direction && f.pose === frame.pose,
      );
      expect(frame.seat_contact ?? null).toEqual(original.seat_contact);
      expect(frame.foot_anchor).toEqual(original.foot_anchor);
      expect(frame.accessory_anchors).toEqual(original.accessory_anchors);
      expect(frame.hands).toEqual(original.hands);
      if (original.seat_contact === null) {
        expect(frame).not.toHaveProperty("seat_contact");
        omitted++;
      }
    }
  }
  expect(omitted).toBeGreaterThan(100);
});
