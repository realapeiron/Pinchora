import { describe, expect, it } from "vitest";

import { isDiscordId } from "./ids";

describe("ids", () => {
  it("takes only snowflakes", () => {
    expect(isDiscordId("1234567890123456789")).toBe(true);
    for (const bad of ["", "abc", "12a", "../1", "1 2", "-1", "123456789012345678901"]) {
      expect(isDiscordId(bad)).toBe(false);
    }
  });
});
