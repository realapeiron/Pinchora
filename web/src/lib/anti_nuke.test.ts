import { describe, expect, it } from "vitest";
import { draftProblem, validUserId, words, type AntiNukeConfig } from "./anti_nuke";
describe("anti-nuke editor guardrails", () => {
  it("keeps Discord snowflakes exact and refuses self trust", () => {
    expect(validUserId("9223372036854775807", "1")).toBe(true);
    for (const id of ["1", "0", "01", "-1", "2.5", "1e10", "9223372036854775808"])
      expect(validUserId(id, "1")).toBe(false);
  });
  it("validates limits and requires a quarantine role", () => {
    const config = {
      limits: { ban: { count: 5, window_secs: 60 } },
      response: "alert",
      quarantine_role_id: null,
    } as unknown as AntiNukeConfig;
    expect(draftProblem(config)).toBe(false);
    expect(draftProblem({ ...config, response: "quarantine" })).toBe(true);
    expect(draftProblem({ ...config, limits: { ban: { count: 0, window_secs: 60 } } })).toBe(true);
    expect(draftProblem({ ...config, limits: { ban: { count: 5, window_secs: 301 } } })).toBe(true);
  });
  it("uses translated text without converting user IDs to numbers", () => {
    expect(words({ used: "{count}/{max} kullanılıyor" })("used", { count: 2, max: 50 })).toBe(
      "2/50 kullanılıyor",
    );
  });
});
