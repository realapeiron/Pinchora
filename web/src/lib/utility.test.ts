import { describe, expect, it } from "vitest";
import {
  TIMEZONE_NOTICE,
  firstRunUtc,
  newRecurrence,
  pauseReason,
  previewBody,
  stickyBody,
  validTimezone,
} from "./utility";

describe("utility editor payloads", () => {
  it("projects sticky drafts to the strict API write schema", () => {
    expect(
      stickyBody({
        channel_id: "1",
        content: "note",
        embed: null,
        min_gap_secs: 30,
        min_messages: 5,
        enabled: true,
        message_count: 100,
        pause_reason: "permissions_lost",
      }),
    ).toEqual({
      channel_id: "1",
      content: "note",
      embed: null,
      min_gap_secs: 30,
      min_messages: 5,
      enabled: true,
    });
  });
  it("projects preview drafts without message fields", () => {
    const draft = {
      content: "private",
      channel_id: "1",
      embed: null,
      schedule: { kind: "once" as const },
      tz: "Europe/Istanbul",
      next_run_at: "2027-01-01T00:00:00Z",
    };
    expect(Object.keys(previewBody(draft)).sort()).toEqual(["next_run_at", "schedule", "tz"]);
  });
  it("explains automatic DST behavior and uncertain delivery before retry", () => {
    expect(TIMEZONE_NOTICE).toContain("follow daylight saving");
    expect(pauseReason("delivery_uncertain")).toContain("Check the channel before resuming");
    expect(pauseReason("permissions_lost")).toContain("permissions");
  });
});

describe("schedule editor time boundaries", () => {
  it("follows DST in named zones and rejects fixed offsets", () => {
    expect(validTimezone("Europe/Berlin")).toBe(true);
    expect(validTimezone("Bogus/Zone")).toBe(false);
    expect(validTimezone("+03:00")).toBe(false);
    expect(newRecurrence("daily", "2026-03-29T07:00:00Z", "Europe/Berlin")).toEqual({
      kind: "daily",
      hour: 9,
      minute: 0,
    });
    expect(newRecurrence("daily", "2026-03-28T07:00:00Z", "Europe/Berlin")).toEqual({
      kind: "daily",
      hour: 8,
      minute: 0,
    });
  });
  it("allows clearing or partially entering the date without throwing", () => {
    expect(firstRunUtc("")).toBe("");
    expect(firstRunUtc("invalid")).toBe("");
    expect(firstRunUtc("2027-02-29T09:00")).toBe("");
    expect(firstRunUtc("2028-02-29T09:00")).toBe("2028-02-29T09:00:00.000Z");
  });
  it("seeds local recurrence across month and week boundaries using the saved offset", () => {
    expect(newRecurrence("weekly", "2026-01-31T23:30:00Z", "Europe/Istanbul")).toEqual({
      kind: "weekly",
      weekday: 0,
      hour: 2,
      minute: 30,
    });
    expect(newRecurrence("daily", "2028-03-01T01:00:00Z", "America/Sao_Paulo")).toEqual({
      kind: "daily",
      hour: 22,
      minute: 0,
    });
  });
});
