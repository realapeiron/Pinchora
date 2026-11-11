import { expect, test } from "vitest";
import type { RobloxSettings } from "../api/roblox";
import { robloxSummary } from "./robloxSummary";

const settings: RobloxSettings = {
  enabled: true,
  require_verified: true,
  verified_role_id: "1",
  unverified_role_id: null,
  verify_channel_id: null,
  nickname_template: null,
};
test("Roblox summaries include off and empty states", () => {
  expect(robloxSummary(settings, []).binds).toBe("No role binds yet");
  expect(robloxSummary(settings, []).verification).toContain("No channel");
  expect(robloxSummary({ ...settings, enabled: false, require_verified: false }, []).verification).toBe(
    "Off",
  );
  expect(robloxSummary({ ...settings, enabled: false, require_verified: false }, []).newcomers).toBe("Off");
});
