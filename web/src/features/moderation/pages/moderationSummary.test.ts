import { expect, test } from "vitest";
import type { GuildInfo } from "../../../lib/api";
import { moderationSummary } from "./moderationSummary";

test("Moderation summary follows the current channel, DM switch and ladder", () => {
  const info = { channels: [{ id: "123", name: "mod-log", type: 0 }] } as GuildInfo;
  expect(moderationSummary("123", true, 3, info)).toEqual({
    log: "#mod-log",
    dm: "On",
    ladder: "3 warn rungs",
  });
});
test("Moderation off and empty ladder", () => {
  expect(moderationSummary(null, false, 0)).toEqual({
    log: "Server log default",
    dm: "Off",
    ladder: "0 warn rungs",
  });
});
