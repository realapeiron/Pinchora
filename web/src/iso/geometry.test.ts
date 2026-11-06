// The geometry the renderer stands on: the projection, the draw order and the placement footprints,
// with the same checks `room.rs` and `house.rs` make in their own tests.

import { describe, expect, it } from "vitest";

import { Content } from "./content";
import { drawOrder } from "./order";
import { floorTop, plane, screenToTile, wallColumn } from "./projection";
import { footprint, surfaceLift } from "./rules";
import type { SceneItem } from "./scene";
import { blank } from "./image";
import { fixtureContent } from "./testing";

function piece(x: number, y: number, w: number, d: number, layer: SceneItem["layer"]): SceneItem {
  return {
    sprite: blank(8, 8),
    x,
    y,
    w,
    d,
    layer,
    flipped: false,
    selected: false,
    lift: 0,
    info: { kind: "furniture", label: "", detail: null, placement: null },
  };
}

describe("projection", () => {
  it("walls meet the floor: every column starts its floor right below its wall", () => {
    for (let lx = -64; lx < 64; lx++) {
      const top = floorTop(lx);
      const [a, b] = plane(lx, top);
      expect(a >= 0 && b >= 0, `column ${lx} starts on the floor`).toBe(true);
      const [a2, b2] = plane(lx, top - 1);
      expect(a2 < 0 || b2 < 0, `column ${lx} has floor above its wall`).toBe(true);
    }
  });

  it("counts wall pixels away from the corner on both sides", () => {
    expect([0, 1, 31].map((m) => wallColumn("right", m))).toEqual([0, 1, 31]);
    expect([0, 1, 31].map((m) => wallColumn("left", m))).toEqual([-1, -2, -32]);
  });

  it("finds the floor tile under a screen point", () => {
    // Tile (x, y) has its top corner at (32·(x−y), 16·(x+y)); its middle is 16 rows below.
    for (const [x, y] of [
      [0, 0],
      [3, 0],
      [0, 5],
      [4, 4],
      [7, 2],
    ] as const) {
      expect(screenToTile(32 * (x - y), 16 * (x + y) + 16, 8, 8)).toEqual([x, y]);
    }
    expect(screenToTile(0, -20, 8, 8)).toBeNull();
    expect(screenToTile(400, 100, 8, 8)).toBeNull();
  });
});

describe("draw order", () => {
  it("draws nearer things later", () => {
    const items = [
      piece(2, 2, 1, 1, "floor"),
      piece(0, 0, 2, 2, "floor"),
      piece(0, 0, 1, 1, "surface"),
      piece(4, 0, 1, 3, "floor"),
      piece(3, 3, 1, 1, "figure"),
    ];
    const order = drawOrder(items, [0, 1, 2, 3, 4]);
    const pos = (i: number) => order.indexOf(i);
    expect(pos(1), "back table before the chair in front").toBeLessThan(pos(0));
    expect(pos(1), "table before the lamp on it").toBeLessThan(pos(2));
    expect(pos(0), "chair before the pet in front of it").toBeLessThan(pos(4));
    expect(order).toHaveLength(5);
  });
});

describe("catalogue", () => {
  const content: Content = fixtureContent();

  it("resolves plain, variant and soulbound ids as the bot does", () => {
    const sofa = content.item("plainwhorl_chair__sand");
    expect(sofa?.variant).toBe("sand");
    expect(content.item("plainwhorl_chair__sand~bound")?.variant).toBe("sand");
    expect(content.itemName(sofa!)).toBe("Plainwhorl Chair (Sand)");
    // A design that comes in colours needs one; a colour it does not come in is unknown.
    expect(content.item("plainwhorl_chair")).toBeNull();
    expect(content.item("plainwhorl_chair__ultraviolet")).toBeNull();
    expect(content.item("no_such_thing")).toBeNull();
  });

  it("flips a floor item a quarter turn and leaves wall items alone", () => {
    const chair = footprint(content, "crumbrail_bench__sand", 6, 7, false);
    const turned = footprint(content, "crumbrail_bench__sand", 6, 7, true);
    expect(chair?.layer).toBe("floor");
    expect([turned?.rect.w, turned?.rect.d]).toEqual([chair?.rect.d, chair?.rect.w]);
    const window = footprint(content, "pebblepane_window__sand", 1, 0, false);
    const flipped = footprint(content, "pebblepane_window__sand", 1, 0, true);
    expect(window?.rect).toEqual(flipped?.rect);
  });

  it("lifts a small thing by the table under it", () => {
    const placed = [
      { item_id: "crumbcorner_table__moss", x: 3, y: 4, flipped: false },
      { item_id: "pagepair_books__moss", x: 3, y: 4, flipped: false },
    ];
    expect(surfaceLift(content, placed, 3, 4)).toBeGreaterThan(0);
    expect(surfaceLift(content, placed, 0, 0)).toBe(0);
    expect(surfaceLift(content, [], 3, 4)).toBe(0);
  });
});
