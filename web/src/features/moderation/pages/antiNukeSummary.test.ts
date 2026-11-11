import { expect, test } from "vitest";
import type { AntiNukeConfig, AntiNukeData } from "../api/anti_nuke";
import { antiNukeSummary } from "./antiNukeSummary";

const config = {
  enabled: false,
  response: "ban",
  limits: {},
  trusted_users: [],
  vip_roles: [],
} as unknown as AntiNukeConfig;
const data = { events: [] } as unknown as AntiNukeData;
const words = (key: string) => (key === "response.ban" ? "Ban" : key);

test("Anti-nuke summary handles off and empty lists", () => {
  expect(antiNukeSummary(config, data, words)).toEqual({
    protection: "Off",
    autoRestore: "Off",
    revertVip: "Off",
    removeBots: "Off",
    ownerOnly: "Off",
    limits: "0 action limits",
    trusted: "No trusted members",
    vip: "No VIP roles",
    preview: "Preview only",
    events: "No recent events",
  });
});

test("Anti-nuke summary follows the current protection and lists", () => {
  const value = {
    ...config,
    enabled: true,
    auto_restore: true,
    revert_vip_bans: true,
    remove_bots: true,
    owner_only_config: true,
    limits: { kick: { count: 2, window_secs: 5 } },
    trusted_users: ["1"],
    vip_roles: ["2", "3"],
  };
  expect(antiNukeSummary(value, { ...data, events: [{}, {}] } as AntiNukeData, words)).toMatchObject({
    protection: "Ban",
    autoRestore: "On",
    revertVip: "On",
    removeBots: "On",
    ownerOnly: "On",
    limits: "1 action limits",
    trusted: "1 trusted members",
    vip: "2 VIP roles",
    events: "2 recent events",
  });
});
