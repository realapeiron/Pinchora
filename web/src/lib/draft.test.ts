import { describe, expect, it } from "vitest";

import { changedFields, patchOf, unsavedLabel } from "./draft";

const saved = { leveling_enabled: true, voice_xp_enabled: false, levelup_mode: "channel" };

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
