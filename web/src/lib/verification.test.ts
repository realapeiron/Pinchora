import { describe, expect, it } from "vitest";
import { idList, words } from "./verification";
describe("Safety settings", () => {
  it("preserves snowflake precision and removes duplicate whitelist IDs", () =>
    expect(idList("123456789012345678, 123456789012345678\n987654321098765432")).toEqual([
      "123456789012345678",
      "987654321098765432",
    ]));
  it("interpolates translated copy without interpreting markup", () =>
    expect(words({ used: "{count}/{max} kullanıldı" })("used", { count: 3, max: 50 })).toBe(
      "3/50 kullanıldı",
    ));
});
