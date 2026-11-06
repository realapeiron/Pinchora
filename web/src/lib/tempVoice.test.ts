import { describe, expect, it } from "vitest";

import { FALLBACK_NAME, MAX_NAME, ago, roomName, templateProblem } from "./tempVoice";

describe("room names", () => {
  it("name a room after its owner and number it", () => {
    expect(roomName("{user}'s room", "Mira", 1)).toBe("Mira's room");
    expect(roomName("Room {n} · {user}", "Mira", 3)).toBe("Room 3 · Mira");
  });

  it("do not expand a name that looks like a placeholder", () => {
    expect(roomName("{user}", "{n}", 7)).toBe("{n}");
  });

  it("collapse whitespace, stay within Discord's limit and are never empty", () => {
    expect(roomName("{user}", "line\nbreak\tand  gaps", 1)).toBe("line break and gaps");
    expect(roomName("{user}", "x".repeat(300), 1)).toHaveLength(MAX_NAME);
    expect([...roomName("{user}", "🦀".repeat(300), 1)]).toHaveLength(MAX_NAME);
    expect(roomName("   ", "Mira", 1)).toBe(FALLBACK_NAME);
  });
});

describe("name templates", () => {
  it("accept the two words and plain text", () => {
    expect(templateProblem("{user}'s room")).toBeNull();
    expect(templateProblem("{user} {n}")).toBeNull();
    expect(templateProblem("Lounge")).toBeNull();
  });

  it("refuse an empty one, a long one and words nobody knows", () => {
    expect(templateProblem("")).toMatch(/Give the rooms a name/);
    expect(templateProblem("   ")).toMatch(/Give the rooms a name/);
    expect(templateProblem("x".repeat(81))).toMatch(/at most 80/);
    expect(templateProblem("Hello {nick}")).toMatch(/\{nick\} isn't one of the words/);
    expect(templateProblem("Hello {user")).toMatch(/without its/);
    expect(templateProblem("{{user}")).toMatch(/without its/);
  });
});

describe("how long ago", () => {
  const now = Date.parse("2026-11-06T12:00:00Z");
  it("says it the way a person would", () => {
    expect(ago("2026-11-06T11:59:40Z", now)).toBe("just now");
    expect(ago("2026-11-06T11:48:00Z", now)).toBe("12 min ago");
    expect(ago("2026-11-06T09:00:00Z", now)).toBe("3 h ago");
    expect(ago("2026-11-03T09:00:00Z", now)).toBe("3 d ago");
    expect(ago("2026-11-06T13:00:00Z", now)).toBe("just now");
  });
});
