import { describe, expect, it } from "vitest";

import type { AuditEntry } from "./api";
import { describeChange, initials, timeAgo } from "./text";

const now = new Date("2026-11-06T12:00:00Z");

function entry(after: Record<string, unknown> | null): AuditEntry {
  return {
    id: "1",
    actor_id: "2",
    actor_name: "Ana",
    source: "web",
    action: "settings.patch",
    target: null,
    before: null,
    after,
    created_at: now.toISOString(),
  };
}

describe("text", () => {
  it("says how long ago", () => {
    expect(timeAgo("2026-11-06T11:59:30Z", now)).toBe("just now");
    expect(timeAgo("2026-11-06T11:55:00Z", now)).toBe("5 min ago");
    expect(timeAgo("2026-11-06T09:00:00Z", now)).toBe("3 h ago");
    expect(timeAgo("2026-11-04T12:00:00Z", now)).toBe("2 d ago");
    expect(timeAgo("2026-01-02T12:00:00Z", now)).toBe("2026-01-02");
  });

  it("describes changes", () => {
    expect(describeChange(entry({ leveling_enabled: false }))).toBe("turned leveling off");
    expect(describeChange(entry({ leveling_enabled: true, levelup_mode: "dm" }))).toBe(
      "turned leveling on, changed levelup mode",
    );
    expect(describeChange(entry(null))).toBe("made a change (settings.patch)");
  });

  it("makes initials", () => {
    expect(initials("Pinchora Reef")).toBe("PR");
    expect(initials("cafe")).toBe("CA");
    expect(initials("")).toBe("?");
  });
});
