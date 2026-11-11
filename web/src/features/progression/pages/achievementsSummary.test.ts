import { expect, test } from "vitest";
import type { Achievement, AchievementDraft } from "../api/achievements";
import { achievementSummary, achievementsSummary } from "./achievementsSummary";

const item: AchievementDraft = {
  name: "First post",
  description: "",
  icon: "star",
  kind: "messages",
  target: 10,
  reward_role_id: null,
  reward_points: 0,
  announce_channel_id: null,
  secret: false,
  enabled: true,
};
test("achievement rows summarise target, announcement and off state", () => {
  expect(achievementsSummary([])).toBe("No achievements yet");
  expect(achievementSummary(item)).toContain("messages · 10 · No announcement channel");
  expect(achievementSummary({ ...item, enabled: false })).toContain("Off · 10");
  expect(achievementsSummary([{ ...item, id: "1", module_off: false } as Achievement])).toBe("1 achievement");
});
