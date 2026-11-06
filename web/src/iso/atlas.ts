// The sprite sheet the bot embeds (`assets/sprites/atlas.png` + `atlas.json`), served at
// `/assets/atlas.png` and `/assets/atlas.json`. Sprites are cut out of it on first use.

import { type Img, crop } from "./image";
import { decodePng } from "./png";

interface Entry {
  x: number;
  y: number;
  w: number;
  h: number;
}

export class Atlas {
  private readonly sheet: Img;
  private readonly index: Map<string, Entry>;
  private readonly cut = new Map<string, Img>();

  constructor(sheet: Img, index: Record<string, Entry>) {
    this.sheet = sheet;
    // As the bot's `SpriteStore`: an entry that reaches outside the sheet is ignored.
    this.index = new Map(
      Object.entries(index).filter(([, e]) => e.x + e.w <= sheet.width && e.y + e.h <= sheet.height),
    );
  }

  /** The sprite `name` as drawn, or `null` while it is not drawn yet. */
  get(name: string): Img | null {
    const hit = this.cut.get(name);
    if (hit) return hit;
    const e = this.index.get(name);
    if (!e) return null;
    const sprite = crop(this.sheet, e.x, e.y, e.w, e.h);
    this.cut.set(name, sprite);
    return sprite;
  }

  has(name: string): boolean {
    return this.index.has(name);
  }
}

/** Reads both files of the atlas, by the URLs `GET /api/house/content` gives (they carry the version). */
export async function loadAtlas(pngUrl: string, jsonUrl: string): Promise<Atlas> {
  const [png, index] = await Promise.all([
    fetch(pngUrl, { credentials: "same-origin" }).then((r) => {
      if (!r.ok) throw new Error(`atlas.png: ${r.status}`);
      return r.arrayBuffer();
    }),
    fetch(jsonUrl, { credentials: "same-origin" }).then((r) => {
      if (!r.ok) throw new Error(`atlas.json: ${r.status}`);
      return r.json() as Promise<Record<string, Entry>>;
    }),
  ]);
  return new Atlas(await decodePng(new Uint8Array(png)), index);
}
