import { expect, test } from "vitest";
import type { Settings } from "../api/playerEconomy";
import { economySummary } from "./economySummary";

const settings = {
  pool_goal: 10000,
  pool_total: 200,
  shop_min_level: 3,
  shop_min_days: 7,
  shop_enabled: true,
  revision: 0,
};
test("Economy summary reflects pool, store rules and balance preview", () => {
  expect(economySummary(settings, 50, 4)).toEqual({
    pool: "200 of 10,000 shells",
    store: "Level 3 · 7 days",
    balances: "4 members · keep 50%",
  });
});
test("Economy off and empty preview", () => {
  expect(economySummary({ ...settings, shop_enabled: false } as Settings)).toEqual({
    pool: "200 of 10,000 shells",
    store: "Off",
    balances: "Keep 100%",
  });
});
