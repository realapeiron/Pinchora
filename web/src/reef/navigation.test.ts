import { describe, expect, it } from "vitest";
import fixtures from "./fixtures/navigation.json";
import layouts from "./fixtures/navigation-layout.json";
import { approach, classify, findPath, free, navigation, spawn, type Grid, type Target } from "./navigation";
import { Player } from "./player";
import { fixtureContent, fixtureJson, type SceneFile } from "../iso/testing";
import type { Point } from "./manifest";
const grid = (width = 4, depth = 4, blocked: number[] = []): Grid => ({
  width,
  depth,
  blocked: new Set(blocked),
});
const seat: Target = {
  id: "seat",
  label: "Stool",
  kind: "seat",
  approaches: [
    [1, 2],
    [2, 1],
  ],
  anchor: [1, 1],
  height: 20,
  direction: "SW",
};
describe("portable A* contract", () => {
  for (const f of layouts.cases)
    it(f.name, () => {
      const content = fixtureContent();
      const base = fixtureJson<SceneFile>("empty_8x8.json").room;
      const { grid: g, targets } = navigation(content, {
        ...base,
        width: 6,
        depth: 6,
        placements: f.placements,
      });
      expect([...g.blocked].sort((a, b) => a - b)).toEqual(
        f.blocked.map(([x, y]) => y! * 6 + x!).sort((a, b) => a - b),
      );
      expect(targets.map(({ label: _label, ...target }) => target)).toEqual(f.targets);
    });

  for (const f of fixtures.cases)
    it(f.name, () => {
      const g = grid(f.size[0], f.size[1]);
      for (const [x, y] of f.blocked) g.blocked.add(y! * g.width + x!);
      expect(findPath(g, f.start as Point, f.goal as Point)).toEqual(f.path);
    });
  it("chooses a deterministic free entry and handles a full room", () => {
    expect(spawn(grid())).toEqual([0, 3]);
    expect(spawn(grid(2, 2, [0, 1, 2, 3]))).toBeNull();
  });
  it("chooses a reachable approach rather than the closest blocked one", () => {
    const g = grid(4, 4, [5, 9]);
    expect(approach(g, [0, 0], seat)?.at(-1)).toEqual([2, 1]);
  });
  it("conservatively blocks floor and surface; rugs remain walkable", () => {
    const c = fixtureContent();
    for (const b of c.catalog.furniture) {
      if (b.layer === "rug") expect(classify(b).kind).toBe("walkable");
      if (b.layer === "surface") expect(classify(b).blocked).toBeNull();
    }
  });
  it("rotates seat anchors and approach cells", () => {
    const c = fixtureContent();
    const file = fixtureJson<SceneFile>("empty_8x8.json");
    const b = c.catalog.furniture.find((b) => b.id === "pebblerest_stool")!;
    const id = b.variants.length ? `${b.id}__${b.variants[0]}` : b.id;
    const nav = navigation(c, {
      ...file.room,
      placements: [{ id: "1", item_id: id, x: 2, y: 3, flipped: true }],
    });
    expect(free(nav.grid, [2, 3])).toBe(false);
    expect(nav.targets[0]?.direction).toBe("SE");
    expect(nav.targets[0]?.approaches[0]).toEqual([3, 3]);
  });
});
describe("local player state", () => {
  it("walks at three tiles/sec, seats and releases to the approach tile", () => {
    const p = new Player(grid(4, 4, [5]));
    expect(p.move(seat.anchor, seat)).toBe(true);
    for (let i = 0; i < 30; i++) p.update(100, false);
    expect(p.state).toBe("sit");
    expect(p.target?.id).toBe("seat");
    expect(p.move([0, 3])).toBe(true);
    expect(p.target).toBeNull();
    for (let i = 0; i < 30; i++) p.update(100, false);
    expect(p.tile).toEqual([0, 3]);
    expect(p.state).toBe("idle");
  });
  it("does not abandon a valid path for an unreachable replacement", () => {
    const p = new Player(grid(4, 4, [5]));
    p.move([3, 3]);
    p.update(100, false);
    expect(p.move([1, 1])).toBe(false);
    for (let i = 0; i < 20; i++) p.update(100, false);
    expect(p.tile).toEqual([3, 3]);
  });
  it("replans through the next tile without cutting a corner", () => {
    const p = new Player(grid());
    p.move([3, 3]);
    p.update(100, false);
    p.move([0, 0]);
    p.update(100, false);
    expect(p.position?.[1]).toBe(3);
    for (let i = 0; i < 40; i++) p.update(100, false);
    expect(p.tile).toEqual([0, 0]);
  });
  it("reduced motion advances collision tiles without interpolation and use ends", () => {
    const p = new Player(grid());
    p.move([1, 3]);
    p.update(100, true);
    expect(p.position).toEqual([0, 3]);
    for (let i = 0; i < 4; i++) p.update(100, true);
    expect(p.tile).toEqual([1, 3]);
    p.move([1, 3], { ...seat, kind: "usable", approaches: [[1, 3]] });
    expect(p.state).toBe("use");
    p.update(1000, true);
    expect(p.state).toBe("idle");
  });
});

it("faces a usable object from its actual approach tile", () => {
  const p = new Player(grid());
  p.move([0, 2], { ...seat, kind: "usable", anchor: [0, 2], approaches: [[0, 3]] });
  expect(p.state).toBe("use");
  expect(p.direction).toBe("NE");
});
