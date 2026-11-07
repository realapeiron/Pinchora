import { expect, it } from "vitest";
import { actorItem, standingOrder } from "./compositor";
import { anchor } from "../iso/render";
for (const count of [16, 32])
  it(`frame ordering CPU budget: 60 furnishings + ${count} moving figures`, () => {
    const items = Array.from({ length: 60 }, (_, i) => ({
      ...actorItem(i % 10, Math.floor(i / 10)),
      layer: "floor" as const,
    }));
    const actors = Array.from({ length: count }, (_, i) => actorItem(i % 14, 7 + Math.floor(i / 14)));
    const all = [...items, ...actors];
    const times: number[] = [];
    for (let frame = 0; frame < 600; frame++) {
      for (let i = 0; i < count; i++) actors[i]!.x = (i % 14) + Math.sin(frame / 60) * 0.4;
      const start = performance.now();
      const order = standingOrder(all);
      for (const i of order) anchor(all[i]!, all[i]!.sprite);
      if (frame > 60) times.push(performance.now() - start);
    }
    times.sort((a, b) => a - b);
    const p95 = times[Math.floor(times.length * 0.95)]!;
    console.log(`Reef ordering CPU only: 60 + ${count}: p95 ${p95.toFixed(3)} ms (not browser FPS)`);
    expect(p95).toBeLessThan(16.7);
  });
