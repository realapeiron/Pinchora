import { describe, expect, it } from "vitest";

import {
  type PanelDraft,
  bodyOf,
  customEmojiUrl,
  draftOf,
  editsOf,
  emptyDraft,
  footerOf,
  joinSpan,
  newKey,
  problemWithDelay,
  problemWithDraft,
  problemWithExpiry,
  problemWithRules,
  reactionProblems,
  spanText,
  splitSpan,
} from "./roles";

const row = (role_id: string, label: string, extra: Partial<PanelDraft["roles"][number]> = {}) => ({
  key: newKey(),
  role_id,
  label,
  emoji: null,
  description: null,
  temp_secs: null,
  ...extra,
});

describe("times", () => {
  it("splits a time into its biggest whole unit and back", () => {
    expect(splitSpan(0)).toEqual({ amount: 0, unit: "h" });
    expect(splitSpan(7_200)).toEqual({ amount: 2, unit: "h" });
    expect(splitSpan(172_800)).toEqual({ amount: 2, unit: "d" });
    expect(splitSpan(90)).toEqual({ amount: 2, unit: "m" });
    expect(joinSpan(2, "h")).toBe(7_200);
    expect(joinSpan(-3, "d")).toBe(0);
  });

  it("writes a time like the bot does", () => {
    expect(spanText(0)).toBe("none");
    expect(spanText(2_700)).toBe("45m");
    expect(spanText(129_600)).toBe("1d 12h");
    expect(spanText(90_061)).toBe("1d 1h 1m 1s");
  });

  it("holds delays and expiries to the command's bounds", () => {
    expect(problemWithDelay(0)).toBeNull();
    expect(problemWithDelay(29)).toMatch(/30 seconds/);
    expect(problemWithDelay(30)).toBeNull();
    expect(problemWithDelay(31 * 86_400)).toMatch(/30 days/);
    expect(problemWithExpiry(0)).toBeNull();
    expect(problemWithExpiry(59)).toMatch(/a minute/);
    expect(problemWithExpiry(365 * 86_400)).toBeNull();
    expect(problemWithExpiry(366 * 86_400)).toMatch(/a year/);
  });
});

describe("panel rules", () => {
  it("refuses limits that make no sense, with the command's sentences", () => {
    expect(problemWithRules(false, 0, 0)).toBeNull();
    expect(problemWithRules(true, 1, 0)).toBeNull();
    expect(problemWithRules(false, 3, 2)).toBe("The fewest roles can't be more than the most.");
    expect(problemWithRules(true, 2, 0)).toBe("One at a time can ask members to keep at most one role.");
    expect(problemWithRules(false, 0, 26)).toBe("A limit is 0 to 25.");
    expect(problemWithRules(false, 5, 0)).toBeNull();
  });

  it("words the footer of each style", () => {
    expect(footerOf({ style: "buttons", exclusive: false, max_picks: 0 })).toBe(
      "Click a button to get a role; click it again to drop it",
    );
    expect(footerOf({ style: "select", exclusive: true, max_picks: 3 })).toBe(
      "Pick your roles from the menu · one at a time",
    );
    expect(footerOf({ style: "reactions", exclusive: false, max_picks: 2 })).toMatch(/· up to 2 roles$/);
  });
});

describe("the editor's draft", () => {
  it("is empty until somebody types, and counts what differs", () => {
    const saved = emptyDraft();
    expect(editsOf(saved, emptyDraft())).toBe(0);
    const now = { ...emptyDraft(), title: "Teams", exclusive: true, roles: [row("1", "Red")] };
    expect(editsOf(saved, now)).toBe(3);
    expect(editsOf(now, { ...now, roles: now.roles.map((r) => ({ ...r, key: newKey() })) })).toBe(0);
  });

  it("round-trips through the body the API takes", () => {
    const panel = {
      id: "7",
      title: " Teams ",
      description: null,
      style: "select" as const,
      exclusive: true,
      required_role: "9",
      min_picks: 1,
      max_picks: 0,
      channel_id: null,
      message_id: null,
      roles: [{ role_id: "1", label: "Red", emoji: "🔴", description: "Plays red", temp_secs: 3_600 }],
    };
    const body = bodyOf(draftOf(panel));
    expect(body.title).toBe("Teams");
    expect(body.description).toBeNull();
    expect(body.roles).toEqual([
      { role_id: "1", label: "Red", emoji: "🔴", description: "Plays red", temp_secs: 3_600 },
    ]);
    expect(body.required_role).toBe("9");
  });

  it("finds the first thing wrong before asking the server", () => {
    const ok = { ...emptyDraft(), title: "Teams", roles: [row("1", "Red"), row("2", "Blue")] };
    expect(problemWithDraft(ok)).toBeNull();
    expect(problemWithDraft({ ...ok, title: "  " })?.field).toBe("title");
    expect(problemWithDraft({ ...ok, description: "x".repeat(501) })?.field).toBe("description");
    expect(problemWithDraft({ ...ok, min_picks: 3, max_picks: 2 })?.field).toBe("rules");
    expect(problemWithDraft({ ...ok, roles: [row("", "Nobody")] })?.message).toMatch(/Pick a role/);
    expect(problemWithDraft({ ...ok, roles: [row("1", "Red"), row("1", "Again")] })?.message).toMatch(
      /twice/,
    );
    expect(problemWithDraft({ ...ok, roles: [row("1", " ")] })?.message).toMatch(/label/);
    expect(problemWithDraft({ ...ok, roles: [row("1", "Red", { temp_secs: 10 })] })?.message).toMatch(
      /minute/,
    );
    const many = Array.from({ length: 26 }, (_, i) => row(String(i + 1), `Role ${i}`));
    expect(problemWithDraft({ ...ok, roles: many })?.message).toMatch(/at most 25/);
  });

  it("says what a reactions panel still needs", () => {
    const base = { ...emptyDraft(), title: "Pings", style: "reactions" as const };
    expect(reactionProblems({ ...base, roles: [row("1", "Bell", { emoji: "🔔" })] })).toEqual([]);
    expect(reactionProblems({ ...base, roles: [row("1", "Bell")] })).toEqual(["Bell needs an emoji."]);
    expect(
      reactionProblems({
        ...base,
        roles: [row("1", "Bell", { emoji: "❤️" }), row("2", "Heart", { emoji: "❤" })],
      }),
    ).toEqual(["Heart shares its emoji."]);
    expect(reactionProblems({ ...base, style: "buttons", roles: [row("1", "Bell")] })).toEqual([]);
  });

  it("finds a custom emoji's picture", () => {
    expect(customEmojiUrl("<:blob:123456789>")).toBe(
      "https://cdn.discordapp.com/emojis/123456789.png?size=48",
    );
    expect(customEmojiUrl("<a:party:42>")).toBe("https://cdn.discordapp.com/emojis/42.gif?size=48");
    expect(customEmojiUrl("🎮")).toBeNull();
    expect(customEmojiUrl(null)).toBeNull();
  });
});
