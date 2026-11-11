import { expect, test } from "vitest";
import type { GuildInfo } from "../../../lib/api";
import { trackersSummary, watchSummary } from "./trackersSummary";

test("tracker rows show selected channels and watched servers", () => {
  const info = { channels: [{ id: "1", name: "news", type: 0 }] } as GuildInfo;
  expect(
    trackersSummary(
      {
        patch_channels: { lol: "1" },
        mc_watch: [{ host: "play.example.com", channel_id: "1" }],
        mc_news_channel_id: "1",
      },
      info,
    ),
  ).toEqual({ lol: "#news", valorant: "Off", mc: "Off", news: "#news", watch: "1 server watched" });
  expect(trackersSummary({ patch_channels: {}, mc_watch: [], mc_news_channel_id: null }).watch).toBe(
    "No servers watched",
  );
  expect(watchSummary("1", info)).toBe("#news");
});
