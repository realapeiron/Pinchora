import { describe, expect, it } from "vitest";
import { trackersDraft, type TrackersData } from "./trackers";

const data: TrackersData = {
  patch_channels: { lol: "7001", valorant: "7002" },
  mc_watch: [{ host: "play.example.com:25565", channel_id: "7001", message_id: "9001" }],
  mc_news_channel_id: null,
  language: "en",
  strings: {},
};

describe("tracker editable settings", () => {
  it("excludes server-owned fields, including nested watch message IDs", () => {
    expect(trackersDraft(data)).toEqual({
      patch_channels: { lol: "7001", valorant: "7002" },
      mc_watch: [{ host: "play.example.com:25565", channel_id: "7001" }],
      mc_news_channel_id: null,
    });
  });

  it("ignores channel insertion order and background message updates when detecting edits", () => {
    const refreshed = {
      ...data,
      patch_channels: { valorant: "7002", lol: "7001" },
      mc_watch: data.mc_watch.map((watch) => ({ ...watch, message_id: "9002" })),
    };
    expect(JSON.stringify(trackersDraft(refreshed))).toBe(JSON.stringify(trackersDraft(data)));
    expect(trackersDraft({ ...data, mc_news_channel_id: "7002" })).not.toEqual(trackersDraft(data));
  });
});
