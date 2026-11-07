import { describe, expect, it } from "vitest";
import { setLocale } from "../i18n";

import { type Feed, ago, blankDraft, bodyOf, changedFields, draftOf, words, wordsProblem } from "./feeds";
import { feedWords } from "./feedsWords";

const t = feedWords({
  "at-most-max-words": "At most {max} words.",
  "word-is-longer-than-max-characters": "“{word}” is longer than {max} characters.",
  "not-yet": "not yet",
  "just-now": "just now",
  "count-min-ago": "{count} min ago",
  "count-h-ago": "{count} h ago",
  "count-days-ago": "{count} days ago",
});

const feed: Feed = {
  id: "7",
  kind: "rss",
  source: "https://reef.example/feed.xml",
  account: "The Reef Times",
  link: "https://reef.example/feed.xml",
  channel_id: "100",
  message: null,
  embed: true,
  ping_role_id: null,
  live_role_id: null,
  live_message_id: null,
  filter_include: ["live"],
  filter_exclude: [],
  enabled: true,
  created_at: "2026-11-06T10:00:00Z",
  health: {
    state: "ok",
    text: "Healthy.",
    last_checked_at: null,
    last_item_at: null,
    last_sent_at: null,
    next_poll_at: "2026-11-06T10:05:00Z",
    fail_count: 0,
    interval_minutes: 10,
  },
};

describe("filter words", () => {
  it("splits on commas, trims, lowers and drops repeats", () => {
    expect(words(" Live, trailer ,live,, ")).toEqual(["live", "trailer"]);
    expect(words("")).toEqual([]);
  });

  it("says when a list is too long or a word too long", () => {
    expect(wordsProblem("a, b", 10, 40, t)).toBeNull();
    expect(wordsProblem(Array.from({ length: 11 }, (_, i) => `w${i}`).join(","), 10, 40, t)).toBe(
      "At most 10 words.",
    );
    expect(wordsProblem("x".repeat(41), 10, 40, t)).toContain("longer than 40");
  });
});

describe("what a save sends", () => {
  it("sends everything, kind and source included, for a new feed", () => {
    const draft = {
      ...blankDraft("reddit"),
      source: " r/aww ",
      channel_id: "5",
      filter_exclude: "NSFW, spoiler",
    };
    expect(bodyOf(draft, null)).toEqual({
      kind: "reddit",
      source: "r/aww",
      channel_id: "5",
      message: null,
      embed: true,
      ping_role_id: null,
      live_role_id: null,
      filter_include: [],
      filter_exclude: ["nsfw", "spoiler"],
      enabled: true,
    });
    expect(blankDraft("youtube").embed).toBe(false);
  });

  it("sends only what changed for an edit, never the source", () => {
    const saved = draftOf(feed);
    expect(saved.filter_include).toBe("live");
    const draft = { ...saved, channel_id: "200", message: "   ", source: "elsewhere" };
    expect(changedFields(saved, draft)).toEqual(["source", "channel_id", "message"]);
    expect(bodyOf(draft, saved)).toEqual({ channel_id: "200", message: null });
    expect(bodyOf(saved, saved)).toEqual({});
  });
});

describe("ago", () => {
  const now = Date.parse("2026-11-06T12:00:00Z");
  it("reads times like a person would", () => {
    expect(ago(null, t, now)).toBe("not yet");
    expect(ago("2026-11-06T11:59:40Z", t, now)).toBe("just now");
    expect(ago("2026-11-06T11:57:00Z", t, now)).toBe("3 min ago");
    expect(ago("2026-11-06T09:00:00Z", t, now)).toBe("3 h ago");
    expect(ago("2026-11-02T12:00:00Z", t, now)).toBe("4 days ago");
  });
});

it("uses Turkish time and validation strings from the server", async () => {
  await setLocale("tr");
  const tr = feedWords({
    "count-min-ago": "{count} dk önce",
    "at-most-max-words": "En fazla {max} kelime.",
  });
  expect(ago("2026-11-06T11:57:00Z", tr, Date.parse("2026-11-06T12:00:00Z"))).toBe("3 dk önce");
  expect(wordsProblem("a, b, c", 2, 40, tr)).toBe("En fazla 2 kelime.");
  await setLocale("en");
});
