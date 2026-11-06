// The stand-in art, the palette swap and the sleeping pet against the sprites the Rust code drew.

import { describe, expect, it } from "vitest";

import { type Img, blank, fingerprint } from "./image";
import { placeholderItem, placeholderPet, placeholderTile, recolor, sleepingPet } from "./sprites";
import type { Ramp } from "./content";
import type { SceneLayer } from "./scene";
import { fixtureJson } from "./testing";

type Case = { size: [number, number]; hash: string } & (
  | {
      kind: "placeholder_item";
      id: string;
      w: number;
      d: number;
      layer: SceneLayer;
      iso_width: number | null;
    }
  | { kind: "placeholder_tile"; id: string; wall: boolean }
  | { kind: "placeholder_pet"; id: string }
  | {
      kind: "sleeping_pet";
      frame: [number, number];
      at: [number, number];
      body: [number, number];
      left: boolean;
    }
  | { kind: "recolor"; name: string; ramp: Ramp; id: string; w: number; d: number; iso_width: number | null }
);

/** An orange block with an eye near its corner, so a turn or a mirror shows (the Rust test's `block`). */
function block(frame: [number, number], at: [number, number], body: [number, number]): Img {
  const img = blank(frame[0], frame[1]);
  const put = (x: number, y: number, c: number[]) => img.data.set(c, (y * img.width + x) * 4);
  for (let y = at[1]; y < at[1] + body[1]; y++)
    for (let x = at[0]; x < at[0] + body[0]; x++) put(x, y, [200, 90, 60, 255]);
  put(at[0] + 2, at[1] + 1, [255, 255, 255, 255]);
  return img;
}

const cases = fixtureJson<Case[]>("sprites.json");

function make(c: Case): Img {
  switch (c.kind) {
    case "placeholder_item":
      return placeholderItem(c.id, c.w, c.d, c.layer, c.iso_width);
    case "placeholder_tile":
      return placeholderTile(c.id, c.wall);
    case "placeholder_pet":
      return placeholderPet(c.id);
    case "sleeping_pet":
      return sleepingPet(block(c.frame, c.at, c.body), c.left);
    case "recolor":
      return recolor(placeholderItem(c.id, c.w, c.d, "floor", c.iso_width), c.ramp);
  }
}

describe("sprites the Rust code drew", () => {
  it("has cases for every kind", () => {
    expect(new Set(cases.map((c) => c.kind)).size).toBe(5);
  });
  for (const [i, c] of cases.entries()) {
    const title = `${c.kind} #${i} ${"id" in c ? c.id : ""}${"left" in c ? (c.left ? " left" : " right") : ""}${"name" in c ? ` ${c.name}` : ""}`;
    it(title, () => {
      const img = make(c);
      expect([img.width, img.height]).toEqual(c.size);
      expect(fingerprint(img)).toBe(c.hash);
    });
  }
});
