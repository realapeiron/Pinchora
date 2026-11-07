import { expect, it } from "vitest";
import { Footsteps } from "./effects";
it("reuses twelve particles and clears motion effects when reduced motion is enabled", () => {
  const dust = new Footsteps();
  const initial = [...dust.particles];
  for (let frame = 0; frame < 1000; frame++) dust.update(16, true, false, [frame / 60, 4]);
  expect(dust.particles).toHaveLength(12);
  dust.particles.forEach((p, i) => expect(p).toBe(initial[i]));
  expect(dust.particles.some((p) => p.age < 400)).toBe(true);
  dust.update(16, true, true, [10, 4]);
  expect(dust.particles.every((p) => p.age >= 400)).toBe(true);
});
