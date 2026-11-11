import { expect, test } from "vitest";
import type { Feed, FeedList } from "../api/feeds";
import { feedSummary, feedsSummary } from "./feedsSummary";

const feed = { kind: "youtube", channel_id: "1", enabled: true, health: { interval_minutes: 5 } } as Feed;
const list = { kinds: [{ key: "youtube", label: "YouTube" }] } as FeedList;
test("feed summaries show destination, count and off state", () => {
  expect(feedsSummary([])).toBe("No feeds yet");
  expect(feedsSummary([feed])).toBe("1 feed");
  expect(feedSummary(feed, list)).toBe("YouTube · Channel selected · 5 min");
  expect(feedSummary({ ...feed, enabled: false }, list)).toBe("Off");
});
