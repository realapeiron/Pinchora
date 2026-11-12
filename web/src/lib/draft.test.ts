import { describe, expect, it } from "vitest";

import { changedFields, patchOf, undoPatch, undoRequest, unsavedLabel } from "./draft";

const saved = { leveling_enabled: true, voice_xp_enabled: false, levelup_mode: "channel" };

it("save undo restores only the previous saved values", () => {
  const patch = patchOf(saved, { leveling_enabled: false, levelup_mode: "dm" });
  expect(patch).toEqual({ leveling_enabled: false, levelup_mode: "dm" });
  expect(undoPatch(saved, patch)).toEqual({ leveling_enabled: true, levelup_mode: "channel" });
});

it("undo sends the values written by the save as expectations", () => {
  expect(
    undoRequest(
      { leveling_enabled: false, levelup_mode: "dm" },
      { leveling_enabled: true, levelup_mode: "channel" },
    ),
  ).toEqual({
    leveling_enabled: true,
    levelup_mode: "channel",
    expect: { leveling_enabled: false, levelup_mode: "dm" },
  });
});

it("undo sends all saved fields for the server to compare atomically", () => {
  const written = { leveling_enabled: false, levelup_mode: "dm" };
  const previous = { leveling_enabled: true, levelup_mode: "channel" };
  expect(undoRequest(written, previous)).toEqual({
    leveling_enabled: true,
    levelup_mode: "channel",
    expect: written,
  });
  expect(undoRequest({ leveling_enabled: false }, { leveling_enabled: true })).toEqual({
    leveling_enabled: true,
    expect: { leveling_enabled: false },
  });
});

describe("draft", () => {
  it("counts only fields that really differ", () => {
    expect(changedFields(saved, {})).toEqual([]);
    expect(changedFields(saved, { leveling_enabled: true })).toEqual([]);
    expect(changedFields(saved, { leveling_enabled: false })).toEqual(["leveling_enabled"]);
    expect(changedFields(saved, { leveling_enabled: false, levelup_mode: "dm" })).toEqual([
      "leveling_enabled",
      "levelup_mode",
    ]);
  });

  it("builds a patch of the changes", () => {
    expect(patchOf(saved, { leveling_enabled: false, voice_xp_enabled: false })).toEqual({
      leveling_enabled: false,
    });
  });

  it("labels the save bar", () => {
    expect(unsavedLabel(1)).toBe("1 unsaved change");
    expect(unsavedLabel(3)).toBe("3 unsaved changes");
  });
});
