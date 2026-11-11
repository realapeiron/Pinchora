import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { GuildInfo } from "../../../lib/api";
import { newBoard, newVote, type StarboardData } from "../api/starboard";
import { boardSummary, starboardSummary, voteSummary } from "./starboardSummary";

const info = { channels: [{ id: "1", name: "best", type: 0 }] } as GuildInfo;
const empty: StarboardData = {
  boards: [],
  votes: [],
  message_content: false,
  limits: { boards: 3, threshold: 50 },
};

test("starboard list rows summarize empty and populated lists", async () => {
  await setLocale("en");
  expect(starboardSummary(empty)).toEqual({ boards: "No boards yet", votes: "No vote channels yet." });
  expect(
    starboardSummary({
      ...empty,
      boards: [{ ...newBoard(), channel_id: "1" }],
      votes: [{ ...newVote(), channel_id: "1" }],
    }),
  ).toEqual({ boards: "1 board", votes: "1 vote channel" });
});

test("board and vote editor rows summarize draft, exclusions and off cases", async () => {
  await setLocale("en");
  expect(boardSummary(newBoard(), info)).toMatchObject({
    details: "No channel",
    exclusions: "0 included · 0 excluded",
    options: "On",
    weights: "No weighted roles",
  });
  expect(
    boardSummary(
      {
        ...newBoard(),
        channel_id: "1",
        threshold: 2,
        only_channels: ["1"],
        except_channels: ["2"],
        enabled: false,
        role_weights: { "3": 2 },
      },
      info,
    ),
  ).toMatchObject({
    details: "#best · 2 stars",
    exclusions: "1 included · 1 excluded",
    options: "Off",
    weights: "1 weighted role",
  });
  expect(voteSummary(newVote(), info)).toBe("No channel");
  expect(voteSummary({ ...newVote(), channel_id: "1" }, info)).toContain("#best");
});
