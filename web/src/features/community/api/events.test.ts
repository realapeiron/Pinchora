import { describe, expect, it } from "vitest";
import { fromZonedInput, zonedInput } from "./events";

describe("event time zones", () => {
  it("renders the server clock regardless of the browser clock", () => {
    expect(zonedInput("2026-11-12T17:00:00Z", "Europe/Istanbul")).toBe("2026-11-12T20:00");
    expect(fromZonedInput("2026-11-12T20:00", "Europe/Istanbul")).toBe("2026-11-12T17:00:00.000Z");
  });
  it("uses the new UTC offset after the daylight saving change", () => {
    expect(fromZonedInput("2026-03-29T20:00", "Europe/London")).toBe("2026-03-29T19:00:00.000Z");
  });
  it("rejects a local time skipped by daylight saving", () => {
    expect(() => fromZonedInput("2026-03-29T01:30", "Europe/London")).toThrow("does not exist");
  });
});

it("rejects ambiguous autumn clocks including half-hour DST", () => {
  expect(() => fromZonedInput("2026-10-25T01:30", "Europe/London")).toThrow("ambiguous");
  expect(() => fromZonedInput("2026-04-05T01:45", "Australia/Lord_Howe")).toThrow("ambiguous");
});
