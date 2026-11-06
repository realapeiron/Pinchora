import { describe, expect, it } from "vitest";

import {
  type CardDesign,
  type TextLayer,
  CARD,
  commit,
  coverScale,
  fillPlaceholders,
  history,
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
