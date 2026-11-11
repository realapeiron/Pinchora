import { expect, test } from "vitest";
import type { Settings } from "../../../lib/api";
import { pointsSummary } from "./pointsSummary";

const settings = {
  points_enabled: true,
  points_min: 2,
  points_max: 6,
  points_emoji: "🐚",
  points_name: "shells",
} as Settings;
test("Points summary reflects the current currency and store count", () => {
  expect(pointsSummary(settings, 3)).toEqual({ earning: "2–6 🐚 shells", store: "3 items on sale" });
});
test("Points off and empty store", () => {
  expect(pointsSummary({ ...settings, points_enabled: false })).toEqual({
    earning: "Off",
    store: "0 items on sale",
  });
});
