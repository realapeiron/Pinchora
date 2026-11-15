import { describe, it, expect } from "vitest";
import { policyChanges, type ModPolicy } from "./moderationDesk";
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
    expect(policyChanges(policy, draft)).toBe(1);
    expect(policy.autopurge).toEqual([]);
    expect(policyChanges(policy, structuredClone(policy))).toBe(0);
  });
  it("counts each changed field once", () => {
    const draft = { ...structuredClone(policy), appeals: true, report_channel: "5" };
    draft.conditions.push({ role: "7", requires: ["8"], all: true, voice_channel: null });
    draft.conditions.push({ role: "9", requires: [], all: false, voice_channel: null });
    expect(policyChanges(policy, draft)).toBe(3);
  });
});
