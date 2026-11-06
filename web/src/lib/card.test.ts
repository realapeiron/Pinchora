import { describe, expect, it } from "vitest";

import {
  type CardDesign,
  type Layer,
  type TextLayer,
  CARD,
  commit,
  costShare,
  coverScale,
  designCost,
  filledLength,
  fillPlaceholders,
  fitDesign,
  fitLayer,
  heavyMessage,
  history,
  layerCost,
  maxTextSize,
  newLayer,
  ordinal,
  push,
  redo,
  replace,
  resizeBox,
  resizeText,
  snapMove,
  textHeight,
  tidy,
  undo,
  unknownPlaceholders,
} from "./card";

const canvas: CardDesign = {
  version: 1,
  width: 1100,
  height: 500,
  background: { type: "transparent" },
  dim: 0,
  layers: [],
};

describe("the model", () => {
  it("measures text boxes and fills placeholders as the bot does", () => {
    expect(textHeight(64, 1)).toBe(83);
    expect(textHeight(22, 3)).toBe(86);
    const fill = { user: "Captain Pinch", username: "captain.pinch", server: "Pinchora Reef", members: 1234 };
    expect(fillPlaceholders("{user} ({username}) joined {server}: #{members}, the {ordinal}", fill)).toBe(
      "Captain Pinch (captain.pinch) joined Pinchora Reef: #1,234, the 1,234th",
    );
    expect(fillPlaceholders("{user} {nope}", { ...fill, user: "{server}" })).toBe("{server} {nope}");
    expect(fillPlaceholders("#{members} {ordinal}", { ...fill, members: null })).toBe("#? newest");
    expect([1, 2, 3, 4, 11, 12, 13, 101, 1112].map(ordinal)).toEqual([
      "1st",
      "2nd",
      "3rd",
      "4th",
      "11th",
      "12th",
      "13th",
      "101st",
      "1,112th",
    ]);
    expect(unknownPlaceholders("{user} {count} {server} {x_y}")).toEqual(["{count}", "{x_y}"]);
  });

  it("makes new layers with fresh ids in the middle of the canvas", () => {
    const text = newLayer(canvas, "text");
    const second = newLayer({ ...canvas, layers: [text] }, "text");
    expect(text.id).toBe("text-1");
    expect(second.id).toBe("text-2");
    expect(text.x + text.w / 2).toBeCloseTo(550, 0);
    const avatar = newLayer(canvas, "avatar");
    expect(avatar.w).toBe(avatar.h);
    const pet = newLayer(canvas, "sprite", { id: "p_drowsepatch_cat", w: 36, h: 26 });
    expect(pet.w / 36).toBe(pet.h / 26);
    expect(Number.isInteger(pet.w / 36)).toBe(true);
    expect(CARD.placeholders).toContain("{ordinal}");
  });

  it("covers the canvas with an atlas picture scaled whole, as the renderer does", () => {
    expect(coverScale(240, 135, 1100, 500)).toEqual({ k: 5, x: -50, y: -87, w: 1200, h: 675 });
    expect(coverScale(240, 135, 300, 150)).toEqual({ k: 2, x: -90, y: -60, w: 480, h: 270 });
  });
});

describe("moving and resizing", () => {
  const box = { x: 100, y: 100, w: 200, h: 100 };

  it("snaps to the canvas middle and other layers' edges, within the threshold only", () => {
    const centred = snapMove({ ...box, x: 447 }, [], canvas, 6);
    expect(centred.box.x).toBe(450);
    expect(centred.guides).toEqual([{ axis: "x", at: 550 }]);
    const edge = snapMove({ ...box, y: 297 }, [{ x: 600, y: 400, w: 50, h: 50 }], canvas, 6);
    expect(edge.box.y).toBe(300);
    expect(edge.guides).toContainEqual({ axis: "y", at: 400 });
    const free = snapMove({ ...box, x: 420, y: 120 }, [], canvas, 6);
    expect(free.box).toEqual({ ...box, x: 420, y: 120 });
    expect(free.guides).toEqual([]);
  });

  it("resizes from any handle, keeping the opposite side put", () => {
    expect(resizeBox(box, "se", 50, 20, { kind: "free" })).toEqual({ x: 100, y: 100, w: 250, h: 120 });
    expect(resizeBox(box, "nw", 50, 20, { kind: "free" })).toEqual({ x: 150, y: 120, w: 150, h: 80 });
    expect(resizeBox(box, "w", -30, 99, { kind: "free" })).toEqual({ x: 70, y: 100, w: 230, h: 100 });
    expect(resizeBox(box, "se", -500, -500, { kind: "free" }, 8)).toEqual({ x: 100, y: 100, w: 8, h: 8 });
  });

  it("keeps avatars square and stickers in whole pixel steps", () => {
    const square = { x: 0, y: 0, w: 100, h: 100 };
    const grown = resizeBox(square, "nw", -40, -10, { kind: "square" });
    expect(grown).toEqual({ x: -40, y: -40, w: 140, h: 140 });
    const sprite = { x: 0, y: 0, w: 72, h: 52 };
    const stepped = resizeBox(sprite, "se", 40, 0, { kind: "aspect", unit: { w: 36, h: 26 } });
    expect(stepped).toEqual({ x: 0, y: 0, w: 108, h: 78 });
    // An avatar shrunk below four times its border keeps a border the API takes.
    const avatar = { ...newLayer(canvas, "avatar"), border_width: 12 };
    const small = tidy({ ...avatar, ...resizeBox(avatar, "se", -170, -170, { kind: "square" }) });
    expect(small.type === "avatar" && [small.w, small.border_width]).toEqual([30, 7]);
  });

  it("widens text from the sides and scales its font from the corners", () => {
    const text = newLayer(canvas, "text") as TextLayer;
    const wider = resizeText(text, "e", 100, 50);
    expect([wider.w, wider.size, wider.h]).toEqual([text.w + 100, text.size, text.h]);
    const bigger = resizeText(text, "se", text.w / 2, 0);
    expect(bigger.size).toBe(60);
    expect(bigger.h).toBe(textHeight(60, 1));
    expect(bigger.x).toBe(text.x);
    const huge = resizeText(text, "se", 10_000, 0);
    expect(huge.size).toBe(CARD.text_size[1]);
  });
});

describe("the undo history", () => {
  it("steps back and forth, and a drag is one step", () => {
    let h = history(1);
    h = push(h, 2);
    h = push(h, 3);
    expect(undo(h).present).toBe(2);
    expect(redo(undo(h)).present).toBe(3);
    // A drag: replaced as it goes, committed once from where it began.
    let drag = replace(h, 4);
    drag = replace(drag, 5);
    drag = commit(drag, 3);
    expect(drag.present).toBe(5);
    expect(undo(drag).present).toBe(3);
    expect(undo(undo(drag)).present).toBe(2);
    // A new step drops what could be redone.
    expect(push(undo(h), 9).future).toEqual([]);
    expect(undo(history(1))).toEqual(history(1));
  });
});

describe("what keeps a design cheap to draw", () => {
  const avatar = (design: CardDesign) => newLayer(design, "avatar");
  const text = (design: CardDesign, body: string) => ({
    ...(newLayer(design, "text") as TextLayer),
    text: body,
  });

  it("counts a text's length as the API does", () => {
    for (const sample of CARD.filled_samples) {
      expect(filledLength(sample.text), JSON.stringify(sample.text)).toBe(sample.length);
    }
    expect(filledLength("😀李{user}")).toBe(2 + CARD.filled_max["{user}"]!);
  });

  it("charges shapes their area, avatars five times it and text by its characters", () => {
    const shape = newLayer(canvas, "shape");
    expect(layerCost(shape)).toBe(shape.w * shape.h);
    const face = avatar(canvas);
    expect(layerCost(face)).toBe(face.w * face.h * CARD.avatar_cost_factor);
    expect(layerCost(text(canvas, "Hi"))).toBe(2 * CARD.text_char_cost);
    // A hidden layer costs nothing.
    expect(designCost([shape, { ...face, hidden: true }])).toBe(layerCost(shape));
  });

  it("rounds the share up, so 100 or less is a design the API takes", () => {
    const long = text(canvas, "x".repeat(200));
    const rest = CARD.layer_cost_max - layerCost(long);
    const filler = { ...newLayer(canvas, "shape"), w: 1, h: rest } as Layer;
    expect(costShare([long, filler])).toBe(100);
    expect(costShare([long, { ...filler, h: rest + 1 }])).toBe(101);
    expect(costShare([])).toBe(0);
    expect(heavyMessage([long, { ...filler, h: rest + 1 }])).toBe(
      "The visible layers are too heavy to draw quickly together (101% of what a card may use). Shrink, remove or hide some; avatars and long texts weigh the most.",
    );
  });

  it("keeps every layer inside the canvas, and says nothing when it already is", () => {
    const small = { ...canvas, width: 600, height: 150 };
    const big = { ...newLayer(canvas, "shape"), w: 900, h: 450 } as Layer;
    const fitted = fitLayer(big, small);
    expect([fitted.w, fitted.h]).toEqual([600, 150]);
    const face = { ...avatar(canvas), w: 400, h: 400 } as Layer;
    expect([fitLayer(face, small).w, fitLayer(face, small).h]).toEqual([150, 150]);
    const title = { ...(newLayer(canvas, "text") as TextLayer), size: 200, w: 1000, h: textHeight(200, 1) };
    const squeezed = fitLayer(title, small) as TextLayer;
    expect(squeezed.w).toBe(600);
    expect(squeezed.size).toBe(maxTextSize(1, 150));
    expect(squeezed.size).toBe(115);
    expect(textHeight(squeezed.size, 1)).toBeLessThanOrEqual(150);
    expect(squeezed.h).toBe(textHeight(squeezed.size, 1));
    // Nothing to do: the very same objects come back, so no undo step is made for it.
    const fits = { ...canvas, layers: [newLayer(canvas, "shape"), avatar(canvas)] };
    expect(fitDesign(fits)).toBe(fits);
    expect(
      fitDesign({ ...fits, width: 300, height: 150 }).layers.every((l) => l.w <= 300 && l.h <= 150),
    ).toBe(true);
  });

  it("stops a resize at the canvas edge, avatars at their own cap and stickers on whole steps", () => {
    const box = { x: 0, y: 0, w: 200, h: 100 };
    expect(resizeBox(box, "se", 5000, 5000, { kind: "free" }, 8, { w: 1100, h: 500 })).toEqual({
      x: 0,
      y: 0,
      w: 1100,
      h: 500,
    });
    const square = { x: 10, y: 10, w: 100, h: 100 };
    expect(resizeBox(square, "nw", -5000, -5000, { kind: "square" }, 8, { w: 500, h: 500 })).toEqual({
      x: -390,
      y: -390,
      w: 500,
      h: 500,
    });
    const sprite = { x: 0, y: 0, w: 72, h: 52 };
    const stepped = resizeBox(sprite, "se", 5000, 0, { kind: "aspect", unit: { w: 36, h: 26 } }, 8, {
      w: 400,
      h: 300,
    });
    expect(stepped).toEqual({ x: 0, y: 0, w: 396, h: 286 });
    // A font cannot grow past the canvas height.
    const title = newLayer(canvas, "text") as TextLayer;
    const grown = resizeText(title, "se", 100_000, 0, { width: 1100, height: 200 });
    expect(textHeight(grown.size, 1)).toBeLessThanOrEqual(200);
    expect(grown.w).toBeLessThanOrEqual(1100);
  });
});
