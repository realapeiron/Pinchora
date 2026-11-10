import { describe, expect, it } from "vitest";
import type { ModuleEntry, Settings } from "../../../lib/api";
import type { GuildStats } from "../api/stats";
import { attention, catalogFilter, chartModel, setupSteps, statCards } from "./overviewModel";

const modules = [
  { key: "welcome", group: "server", label: "Welcome", blurb: "Greet people", enabled: true },
  { key: "goodbye", group: "server", label: "Goodbye", blurb: "Say bye", enabled: true },
  { key: "log", group: "server", label: "Log", blurb: "Keep records", enabled: true },
  { key: "leveling", group: "players", label: "Leveling", blurb: "Chat XP", enabled: true },
  { key: "stats", group: "server", label: "Stats", blurb: "Count activity", enabled: false },
] as ModuleEntry[];
const settings = {
  welcome_channel_id: "42",
  goodbye_channel_id: null,
  log_channel_id: null,
  leveling_enabled: true,
} as Settings;
const stats = {
  days: 7,
  module_on: true,
  totals: { messages: 120, joins: 9, leaves: 2, commands: 5, active_members: 21, voice_minutes: 0 },
  previous: { messages: 100, joins: 4, leaves: 1, commands: 10, active_members: null, voice_minutes: 0 },
  daily: [{ day: "2026-11-10", messages: 120, joins: 9, leaves: 2, commands: 5, voice_minutes: 0 }],
} as GuildStats;

describe("overview data model", () => {
  it("makes four real stats cards with previous-period changes", () => {
    expect(statCards(stats)).toEqual([
      { key: "messages", value: "120", change: 20 },
      { key: "members", value: "9 / 2", change: null },
      { key: "commands", value: "5", change: -50 },
      { key: "active", value: "21", change: null },
    ]);
    expect(statCards({ ...stats, previous: null })?.every((card) => card.change === null)).toBe(true);
  });
  it("keeps past activity when stats are off, and leaves uncounted periods empty", () => {
    expect(statCards({ ...stats, module_on: false })).toEqual(statCards(stats));
    expect(
      statCards({
        ...stats,
        module_on: false,
        totals: {
          ...stats.totals,
          messages: 0,
          joins: 0,
          leaves: 0,
          commands: 0,
          active_members: null,
        },
        daily: stats.daily.map((day) => ({ ...day, messages: 0, joins: 0, leaves: 0, commands: 0 })),
      }),
    ).toBeNull();
    expect(statCards(undefined)).toBeNull();
  });
  it("only reports computable attention signals", () => {
    expect(attention(settings, modules, null).map((signal) => signal.key)).toEqual([
      "goodbye",
      "log",
      "stats",
    ]);
    expect(attention(settings, modules, 0).map((signal) => signal.key)).toEqual([
      "goodbye",
      "log",
      "stats",
      "roles",
    ]);
    expect(attention(settings, modules, 0).find((signal) => signal.key === "stats")?.page).toBe("modules");
    expect(
      attention(
        { ...settings, goodbye_channel_id: "43", log_channel_id: "44" },
        modules.map((module) => ({ ...module, enabled: true })),
        1,
      ),
    ).toEqual([]);
  });
  it("derives setup progress and remaining steps from settings and modules", () => {
    const steps = setupSteps(settings, modules);
    expect(steps.map((step) => [step.key, step.done])).toEqual([
      ["welcome", true],
      ["goodbye", false],
      ["log", false],
      ["leveling", true],
      ["stats", false],
    ]);
    expect(steps.filter((step) => step.done)).toHaveLength(2);
    expect(steps.find((step) => step.key === "stats")?.page).toBe("modules");
  });
  it("filters the catalog by category, switched-on state and label or blurb", () => {
    expect(catalogFilter(modules, "server", "count", false).map((module) => module.key)).toEqual(["stats"]);
    expect(catalogFilter(modules, "all", "chat xp", true).map((module) => module.key)).toEqual(["leveling"]);
    expect(catalogFilter(modules, "server", "count", true)).toEqual([]);
  });
  it("uses a nice chart scale and labels each real day and count", () => {
    const model = chartModel(stats.daily);
    expect(model.max).toBe(200);
    expect(model.ticks).toEqual([0, 50, 100, 150, 200]);
    expect(model.bars[0]).toMatchObject({ day: "2026-11-10", value: 120, accessible: "Nov 10: 120" });
  });
});
