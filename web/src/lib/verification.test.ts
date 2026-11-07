import { describe, expect, it } from "vitest";
import { idList, words } from "./verification";
describe("Safety settings", () => {
  it("formats counts in Turkish without changing identifier strings", () =>
    expect(
      words({ used: "{count}/{max} · {id}" }, "tr")("used", {
        count: 2500,
        max: 10000,
        id: "123456789012345678",
      }),
    ).toBe("2.500/10.000 · 123456789012345678"));
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
