// Parity with the Rust renderer: every scene the Rust tests wrote is drawn here and compared with the PNG
// they drew, pixel for pixel. `cargo test -p pinchora-commands house_fixtures_match` is the other half.

import { describe, expect, it } from "vitest";

import { fingerprint } from "./image";
import { renderRoom } from "./render";
import { type SceneFile, fixtureContent, fixtureJson, fixturePng, sceneOf } from "./testing";

/** Pixels allowed to differ from the bot's picture; the port is exact, so none. */
const MAX_DIFFERING_PIXELS = 0;

const index = fixtureJson<{ scenes: string[] }>("index.json");

describe("scenes drawn by the bot", () => {
  const content = fixtureContent();
  it("lists the scenes", () => {
    expect(index.scenes.length).toBeGreaterThanOrEqual(6);
  });
  for (const name of index.scenes) {
    it(`${name} matches the bot's picture`, async () => {
      const file = fixtureJson<SceneFile>(`${name}.json`);
      const expected = await fixturePng(file.png);
      const { image } = renderRoom(await sceneOf(file, content));
      expect([image.width, image.height]).toEqual([expected.width, expected.height]);
      expect(file.size).toEqual([expected.width, expected.height]);
      let differing = 0;
      let first: string | null = null;
      for (let i = 0; i < expected.data.length; i += 4) {
        let same = true;
        for (let c = 0; c < 4; c++) same &&= image.data[i + c] === expected.data[i + c];
        if (same) continue;
        differing++;
        if (first === null) {
          const p = i / 4;
          const [x, y] = [p % image.width, Math.floor(p / image.width)];
          const at = (d: Uint8ClampedArray) => `[${d[i]}, ${d[i + 1]}, ${d[i + 2]}, ${d[i + 3]}]`;
          first = `(${x}, ${y}) has ${at(image.data)}, the bot drew ${at(expected.data)}`;
        }
      }
      expect(differing, first ?? "").toBeLessThanOrEqual(MAX_DIFFERING_PIXELS);
      expect(fingerprint(image)).toBe(file.hash);
    });
  }
});
