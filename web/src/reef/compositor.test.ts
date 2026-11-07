import { anchor } from "../iso/render";
import { expect, it } from "vitest";
import { blank, fingerprint } from "../iso/image";
import { fixtureContent, fixtureJson, sceneOf, type SceneFile } from "../iso/testing";
import { Compositor, actorItem, composeReference, prepareRoom, standingOrder } from "./compositor";
import cases from "./fixtures/order.json";
const content = fixtureContent();
for (const name of fixtureJson<{ scenes: string[] }>("index.json").scenes)
  it(`cached layers preserve room.rs fixture ${name}`, async () => {
    const f = fixtureJson<SceneFile>(`${name}.json`);
    const scene = await sceneOf(f, content);
    expect(fingerprint(composeReference(prepareRoom(scene)))).toBe(f.hash);
  });
for (const c of cases.cases)
  it(c.name, () => {
    const floor = {
      ...actorItem(),
      x: c.furniture[0]!,
      y: c.furniture[1]!,
      w: c.furniture[2]!,
      d: c.furniture[3]!,
      layer: "floor" as const,
    };
    expect(standingOrder([floor, actorItem(c.actor[0], c.actor[1])])).toEqual(c.order);
  });
it("overlap rank and roster order remain stable", () => {
  const floor = { ...actorItem(), layer: "floor" as const };
  const surface = { ...actorItem(), layer: "surface" as const };
  expect(standingOrder([actorItem(), surface, floor, actorItem()])).toEqual([2, 1, 0, 3]);
});

it("alpha picking chooses the exposed object, ignores transparent holes and rejects off-scene pixels", () => {
  const lower = blank(3, 3);
  for (let i = 0; i < lower.data.length; i += 4) lower.data.set([100, 100, 100, 255], i);
  const upper = blank(3, 3);
  upper.data.set([200, 200, 200, 255], 0);
  const floor = { ...actorItem(), layer: "floor" as const, sprite: lower };
  const scene = {
    width: 8,
    depth: 8,
    wallpaper: blank(1, 1),
    floor: blank(1, 1),
    items: [floor, { ...floor, sprite: upper }],
    cursor: null,
    grid: false,
  };
  const room = prepareRoom(scene);
  const [x, y] = anchor(floor, lower);
  // Picking is CPU-only; invoke the production method without allocating DOM bitmaps.
  const pick = (px: number, py: number) => Compositor.prototype.pick.call({ room } as Compositor, px, py);
  expect(pick(x + room.origin[0], y + room.origin[1])).toBe(1);
  expect(pick(x + room.origin[0] + 1, y + room.origin[1] + 1)).toBe(0);
  expect(pick(-1, -1)).toBeNull();
});
