import { describe, it, expect } from "vitest";
import { policyChanged, type ModPolicy } from "./moderationDesk";
const policy: ModPolicy = {
  report_channel: null,
  mute_role: null,
  jail_role: null,
  appeals: false,
  booster_role: null,
  booster_personal: false,
  autopurge: [],
  conditions: [],
};
describe("moderation policy draft", () => {
  it("tracks nested cleanup changes without mutating the saved policy", () => {
    const draft = structuredClone(policy);
    draft.autopurge.push({ channel: "123", age_secs: 3600, bots_only: true });
    expect(policyChanged(policy, draft)).toBe(true);
    expect(policy.autopurge).toEqual([]);
    expect(policyChanged(policy, structuredClone(policy))).toBe(false);
  });
});
