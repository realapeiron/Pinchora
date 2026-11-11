import { expect, test } from "vitest";
import type { Settings } from "../../../lib/api";
import type { LevelingPolicy } from "../api/levelingPolicy";
import { levelingSummary, memberXpSummary } from "./levelingSummary";

const settings = {
  leveling_enabled: true,
  levelup_mode: "dm",
  voice_xp_enabled: true,
  voice_xp_amount: 12,
} as Settings;
const policy = { message_min: 15, message_max: 25 } as LevelingPolicy;

test("Leveling summaries follow live modes, amounts and list counts", () => {
  expect(
    levelingSummary(settings, { multipliers: 2, levelRoles: 3, repRoles: 1, voiceRoles: 4 }, policy),
  ).toMatchObject({
    chat: "Level-up destination: Direct message",
    voice: "12 XP per minute",
    multipliers: "2 multipliers",
    levelRoles: "3 level roles",
    repRoles: "1 reputation roles",
    voiceRoles: "4 voice roles",
    policy: "15–25 message XP",
  });
});
test("Leveling off and empty summaries", () => {
  expect(levelingSummary({ ...settings, leveling_enabled: false, voice_xp_enabled: false })).toMatchObject({
    chat: "Off",
    voice: "Off",
    multipliers: "0 multipliers",
    levelRoles: "0 level roles",
    repRoles: "0 reputation roles",
    voiceRoles: "0 voice roles",
    import: "No import yet",
  });
});
test("Member XP summary follows selection", () => {
  expect(memberXpSummary()).toBe("No member selected");
  expect(memberXpSummary("Moon Crab")).toBe("Moon Crab");
});
