import { describe, expect, it } from "vitest";

import type { ModuleEntry } from "./api";
import { badge, changeCount, effective, patchFor, problem, withMode } from "./modules";

const pets: ModuleEntry = {
  key: "pets",
  label: "Pets",
  blurb: "",
  group: "home",
  server_only: false,
  enabled: true,
  channel_mode: "all",
  channels: [],
};
const welcome: ModuleEntry = { ...pets, key: "welcome", label: "Welcome cards", server_only: true };

describe("module drafts", () => {
  it("change nothing until something differs from what is saved", () => {
    expect(patchFor(pets, undefined)).toBeNull();
    expect(patchFor(pets, { enabled: true })).toBeNull();
    expect(patchFor(pets, { channel_mode: "all", channels: [] })).toBeNull();
  });

  it("send only the switch when only the switch moved", () => {
    expect(patchFor(pets, { enabled: false })).toEqual({ enabled: false });
  });

  it("send the mode with its channels when the rule moved", () => {
    expect(patchFor(pets, { channel_mode: "only", channels: ["7"] })).toEqual({
      channel_mode: "only",
      channels: ["7"],
    });
    const limited = { ...pets, channel_mode: "except", channels: ["1", "2"] } as ModuleEntry;
    expect(patchFor(limited, { channels: ["2", "1"] })).toBeNull();
    expect(patchFor(limited, { channels: ["1"] })).toEqual({ channel_mode: "except", channels: ["1"] });
  });

  it("clear the list when going back to all channels", () => {
    const limited = { ...pets, channel_mode: "only", channels: ["1"] } as ModuleEntry;
    expect(patchFor(limited, { channel_mode: "all" })).toEqual({ channel_mode: "all", channels: [] });
    expect(effective(limited, { channel_mode: "all" }).channels).toEqual([]);
  });

  it("leave a server-wide module's channels alone", () => {
    expect(patchFor(welcome, { channel_mode: "only", channels: ["1"] })).toBeNull();
    expect(patchFor(welcome, { enabled: false })).toEqual({ enabled: false });
  });

  it("count the switch and the rule separately", () => {
    const drafts = { pets: { enabled: false, channel_mode: "only" as const, channels: ["1"] }, welcome: {} };
    expect(changeCount([pets, welcome], drafts)).toBe(2);
    expect(changeCount([pets, welcome], {})).toBe(0);
  });

  it("need a channel before a list can be saved", () => {
    expect(problem(pets, { channel_mode: "only" }, 50)).toBe(
      "Pick at least one channel, or choose all channels.",
    );
    expect(problem(pets, { channel_mode: "only", channels: ["1"] }, 50)).toBeNull();
    expect(problem(pets, { channel_mode: "except", channels: ["1", "2", "3"] }, 2)).toContain("at most 2");
    expect(problem(pets, undefined, 50)).toBeNull();
  });

  it("keep the picked channels when switching between only and except", () => {
    const first = withMode(pets, undefined, "only");
    const second = withMode(pets, { ...first, channels: ["5"] }, "except");
    expect(second).toEqual({ channel_mode: "except", channels: ["5"] });
  });

  it("badge the state in a few words", () => {
    expect(badge(true, "all", 0)).toEqual({ text: "ON", tone: "mint" });
    expect(badge(false, "only", 3)).toEqual({ text: "OFF", tone: "dim" });
    expect(badge(true, "only", 1).text).toBe("ONLY IN 1 CHANNEL");
    expect(badge(true, "except", 2).text).toBe("OFF IN 2 CHANNELS");
  });
});
