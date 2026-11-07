import { expect, it } from "vitest";
import { Camera, Gesture } from "./camera";
import { tileTop } from "../iso/projection";
it("picks the same native pixels at fractional DPR, after pan and zoom", () => {
  for (const dpr of [1, 1.25, 1.5, 2, 2.625, 3]) {
    const c = new Camera(dpr);
    c.center(390, 500, [300, 200]);
    c.pan(31.2, -23.8);
    c.zoom(4, [195, 250]);
    for (let y = 0; y < 12; y++)
      for (let x = 0; x < 14; x++) {
        const [px, py] = tileTop(x, y);
        expect(
          c.tile(
            ((px + 300) * c.scale + c.x) / dpr,
            ((py + 16 + 120) * c.scale + c.y) / dpr,
            [300, 120],
            14,
            12,
          ),
        ).toEqual([x, y]);
      }
    expect(Number.isInteger(c.x) && Number.isInteger(c.y) && Number.isInteger(c.scale)).toBe(true);
  }
});
it("preserves -1 edge convention", () => {
  const c = new Camera();
  expect(c.tile(0, 0, [0, 0], 8, 8)).toBeNull();
  expect(c.tile(0, 1, [0, 0], 8, 8)).toEqual([0, 0]);
});
it("separates taps, drags, pinch, and cancellation", () => {
  const g = new Gesture();
  g.down(1, [0, 0]);
  g.move(1, [3, 2]);
  expect(g.up(1)).toBe(true);
  g.down(1, [0, 0]);
  g.move(1, [20, 0]);
  g.move(1, [0, 0]);
  expect(g.up(1)).toBe(false);
  g.down(1, [0, 0]);
  g.down(2, [20, 0]);
  g.up(2);
  expect(g.up(1)).toBe(false);
  g.down(1, [0, 0]);
  g.cancel();
  expect(g.up(1)).toBe(false);
});
it("fits the whole room at the largest supported integer physical scale", () => {
  for (const [width, height, dpr] of [
    [390, 760, 2.625],
    [1016, 720, 1],
    [1280, 800, 2],
  ]) {
    const c = new Camera(dpr);
    const room: [number, number] = [860, 559];
    c.fit(width!, height!, room);
    expect(c.scale).toBe(
      Math.min(4, Math.floor(Math.min(((width! - 48) * dpr!) / 860, ((height! - 48) * dpr!) / 559))),
    );
    for (let frame = 0; frame < 160; frame++) c.follow(width!, height!, [46, 268], room, 16, false, 0);
    expect(c.x).toBeGreaterThanOrEqual(Math.floor(24 * dpr!));
    expect(c.x + room[0] * c.scale).toBeLessThanOrEqual(Math.ceil((width! - 24) * dpr!));
    expect(c.y).toBeGreaterThanOrEqual(Math.floor(24 * dpr!));
    expect(c.y + room[1] * c.scale).toBeLessThanOrEqual(Math.ceil((height! - 24) * dpr!));
    expect(Number.isInteger(c.x) && Number.isInteger(c.y)).toBe(true);
  }
});
it("tracks a zoomed player outside the dead zone and jumps with reduced motion", () => {
  const c = new Camera();
  c.zoom(4, [0, 0]);
  c.center(400, 600, [200, 200]);
  const start = [c.x, c.y];
  c.follow(400, 600, [203, 202], [860, 559], 16, false);
  expect([c.x, c.y]).toEqual(start);
  c.follow(400, 600, [250, 230], [860, 559], 16, false);
  expect(c.x).toBeLessThan(start[0]!);
  expect(c.x).toBeGreaterThan(200 - 250 * 4 + 24);
  c.follow(400, 600, [250, 230], [860, 559], 16, true, 0);
  expect([c.x, c.y]).toEqual([200 - 250 * 4, 300 - 230 * 4]);
});
it("clamps zoom to 1–4 and retains the native pixel under its anchor", () => {
  const c = new Camera(2.625);
  c.fit(390, 760, [860, 559]);
  const before = c.scene(180, 310);
  c.zoom(99, [180, 310]);
  expect(c.scale).toBe(4);
  const after = c.scene(180, 310);
  expect(Math.abs(after[0] - before[0])).toBeLessThanOrEqual(1);
  expect(Math.abs(after[1] - before[1])).toBeLessThanOrEqual(1);
  c.zoom(-99, [180, 310]);
  expect(c.scale).toBe(1);
});

it("explicit centring can place the player at the viewport centre even when the whole room fits", () => {
  const c = new Camera(2.625);
  c.fit(390, 760, [860, 559]);
  c.follow(390, 760, [46, 268], [860, 559], 16, true, 0, false);
  expect(c.x).toBe(Math.round((390 * 2.625) / 2 - 46));
  expect(c.y).toBe(Math.round((760 * 2.625) / 2 - 268));
});
