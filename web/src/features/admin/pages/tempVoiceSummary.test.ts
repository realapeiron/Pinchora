import { expect, test } from "vitest";
import type { Creator, TempVoice } from "../api/tempVoice";
import { creatorSummary, roomSummary, tempVoiceSummary } from "./tempVoiceSummary";

const creator = {
  channel_id: "1",
  name_template: "{user}'s room",
  default_limit: 5,
  rooms: 0,
  enabled: true,
} as Creator;
const data = { creators: [], rooms: [] } as unknown as TempVoice;
test("temporary voice summaries cover empty and off states", () => {
  expect(tempVoiceSummary(data).creators).toBe("No creator channels yet");
  expect(tempVoiceSummary(data).rooms).toBe("No rooms right now");
  expect(creatorSummary(creator)).toContain("5");
  expect(creatorSummary({ ...creator, enabled: false })).toBe("Off");
  expect(roomSummary({ owner_name: "Mira", members: 2, user_limit: 4 } as TempVoice["rooms"][number])).toBe(
    "Mira · 2/4 inside",
  );
});
