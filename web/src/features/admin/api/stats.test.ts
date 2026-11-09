import { describe, expect, it } from "vitest";

import { change, compact, counterName, dayLabel, heatStep, hourLabel, minutes, niceMax, peak } from "./stats";

describe("stats numbers", () => {
  it("rounds a scale up to 1, 2, 4, 6 or 8 times a power of ten, as the cards do", () => {
    expect([0, 1, 3, 5, 9, 11, 200, 201, 371].map(niceMax)).toEqual([1, 1, 4, 6, 10, 20, 200, 400, 400]);
  });

  it("says the change from the period before, or nothing when there was none", () => {
    expect(change(120, 100)).toBe(20);
    expect(change(50, 100)).toBe(-50);
    expect(change(5, 0)).toBeNull();
    expect(change(5, null)).toBeNull();
  });

  it("shades busier hours brighter and leaves empty ones out", () => {
    expect(heatStep(0, 10)).toBeNull();
    expect(heatStep(1, 100)).toBe(0);
    expect(heatStep(50, 100)).toBe(2);
    expect(heatStep(100, 100)).toBe(4);
  });

  it("writes numbers and minutes the way the cards write them", () => {
    expect(compact(999)).toBe("999");
    expect(compact(1_234)).toBe("1.2K");
    expect(compact(48_000)).toBe("48K");
    expect(compact(1_300_000)).toBe("1.3M");
    expect(minutes(45)).toBe("45m");
    expect(minutes(200)).toBe("3h 20m");
    expect(minutes(120)).toBe("2h");
    expect(minutes(72_240)).toBe("1,204h");
  });

  it("reads days and hours as UTC", () => {
    expect(dayLabel("2026-11-06")).toBe("Nov 6");
    expect(dayLabel("2026-01-31", true)).toBe("Jan 31, 2026");
    expect(hourLabel(23)).toBe("23:00–00:00 UTC");
  });

  it("finds the busiest hour", () => {
    const grid = Array.from({ length: 7 }, () => Array.from({ length: 24 }, () => 0));
    expect(peak(grid)).toBeNull();
    grid[4]![20] = 9;
    grid[1]![3] = 2;
    expect(peak(grid)).toEqual({ weekday: 4, hour: 20, messages: 9 });
  });

  it("fills a counter's name like the bot", () => {
    expect(counterName("👥 Members: {count}", 12_345)).toBe("👥 Members: 12,345");
  });
});
