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
