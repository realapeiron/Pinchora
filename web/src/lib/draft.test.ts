import { describe, expect, it } from "vitest";

import { changedFields, patchOf, safeUndoPatch, undoPatch, unsavedLabel } from "./draft";

const saved = { leveling_enabled: true, voice_xp_enabled: false, levelup_mode: "channel" };

it("save undo restores only the previous saved values", () => {
  const patch = patchOf(saved, { leveling_enabled: false, levelup_mode: "dm" });
  expect(patch).toEqual({ leveling_enabled: false, levelup_mode: "dm" });
  expect(undoPatch(saved, patch)).toEqual({ leveling_enabled: true, levelup_mode: "channel" });
});

it("undo restores fields that still hold this save's values", () => {
  expect(
    safeUndoPatch(
      { leveling_enabled: false, levelup_mode: "dm" },
      { leveling_enabled: false, levelup_mode: "dm" },
      { leveling_enabled: true, levelup_mode: "channel" },
    ),
  ).toEqual({ patch: { leveling_enabled: true, levelup_mode: "channel" }, changedElsewhere: false });
});

it("undo leaves newer saved values alone, including when every field changed", () => {
  const written = { leveling_enabled: false, levelup_mode: "dm" };
  const previous = { leveling_enabled: true, levelup_mode: "channel" };
  expect(safeUndoPatch({ leveling_enabled: true, levelup_mode: "dm" }, written, previous)).toEqual({
    patch: { levelup_mode: "channel" },
    changedElsewhere: true,
  });
  expect(safeUndoPatch({ leveling_enabled: true, levelup_mode: "channel" }, written, previous)).toEqual({
    patch: {},
    changedElsewhere: true,
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
