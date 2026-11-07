import { expect, it } from "vitest";
import { blank } from "../iso/image";
import { composeAccessories, poseAt, type AccessoryLayer } from "./animation";
import { KEYS, deriveRamp, paintRamp, recolorCharacter, rgb } from "./palette";
import type { Frame } from "./manifest";
it("maps exact key colours to bot hand ramps, including the first four bottom shades", () => {
  const colors = [...KEYS.skin, ...KEYS.hair, ...KEYS.top, ...KEYS.bottom, 0x1b1d2b, 0xfff1e8];
  const img = blank(colors.length, 1);
  colors.forEach((c, i) => img.data.set([...rgb(c), 255], i * 4));
  const out = recolorCharacter(img, "bobbin.1.11.1.2");
  const expected = [
    ...paintRamp("1", "skin"),
    ...paintRamp("11", "hair"),
    ...paintRamp("1", "top"),
    ...paintRamp("2", "bottom").slice(0, 4),
    rgb(0x1b1d2b),
    rgb(0xfff1e8),
  ].flatMap((c) => [...c, 255]);
  expect([...out.data]).toEqual(expected);
  expect(img.data).not.toEqual(out.data);
});
it("custom ramps preserve a shadeable middle and transparency", () => {
  expect(deriveRamp(0xff6b4a, 5)[2]).toEqual([255, 107, 74]);
  const img = blank(1, 1);
  img.data.set([...rgb(KEYS.skin[0]!), 0]);
  expect(recolorCharacter(img, "bobbin.#123456.0.0.0").data).toEqual(img.data);
});
it("selects all four walking frames and honours reduced motion", () => {
  expect([0, 125, 250, 375].map((t) => poseAt("walk", t, false))).toEqual([
    "walk1",
    "walk2",
    "walk3",
    "walk4",
  ]);
  expect(poseAt("walk", 250, true)).toBe("idle");
  expect(poseAt("use", 250, false)).toBe("use2");
});
it("layers accessories by slot and direction using frame anchors", () => {
  const frame: Frame = {
    direction: "SW",
    pose: "idle",
    foot_anchor: [48, 120],
    seat_contact: null,
    accessory_anchors: { hat: [1, 1], head: [1, 1], torso: [1, 1] },
    hands: { screen_left: null, screen_right: [1, 1] },
  };
  const body = blank(3, 3);
  body.data.set([0, 255, 0, 255], 16);
  const sprite = blank(1, 1);
  sprite.data.set([255, 0, 0, 255]);
  const layer: AccessoryLayer = {
    id: "test",
    slot: "back",
    sprite,
    anchor: [1, 1],
    offset: [1, 1],
    clear: [],
  };
  expect([...composeAccessories(body, frame, [layer]).data.slice(16, 20)]).toEqual([0, 255, 0, 255]);
  expect([...composeAccessories(body, { ...frame, direction: "NW" }, [layer]).data.slice(16, 20)]).toEqual([
    255, 0, 0, 255,
  ]);
  expect([...composeAccessories(body, frame, [{ ...layer, slot: "head" }]).data.slice(16, 20)]).toEqual([
    255, 0, 0, 255,
  ]);
});

import paletteFixtures from "./fixtures/palette.json";
it("matches Rust derive_ramp fixtures for custom colours", () => {
  for (const f of paletteFixtures.cases) {
    expect(deriveRamp(f.rgb, 4), f.rgb.toString(16)).toEqual(f.four);
    expect(deriveRamp(f.rgb, 5), f.rgb.toString(16)).toEqual(f.five);
  }
});
it("preserves accessories beyond the cell with padded frames", () => {
  const body = blank(2, 2);
  const sprite = blank(1, 1);
  sprite.data.set([7, 8, 9, 255]);
  const frame: Frame = {
    direction: "SW",
    pose: "idle",
    foot_anchor: [48, 120],
    seat_contact: null,
    accessory_anchors: { hat: [0, 0], head: [0, 0], torso: [0, 0] },
    hands: { screen_left: null, screen_right: null },
  };
  const out = composeAccessories(
    body,
    frame,
    [{ id: "hat", slot: "head", sprite, offset: [-1, -1], anchor: [0, 0], clear: [] }],
    2,
  );
  expect([...out.data.slice((1 * out.width + 1) * 4, (1 * out.width + 1) * 4 + 4)]).toEqual([7, 8, 9, 255]);
});
