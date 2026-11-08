import { describe, it, expect } from "vitest";
import { provisionReady, seasonProgress, settingsDraft, type Economy, type EconomyPlayer } from "./economyV2";
describe("economy budgets and writable settings", () => {
  it("caps offline provisions and never pays fractional hours", () => {
    const p = { provision_at: "2026-11-01T00:00:00Z", provision_claimed: 0 } as EconomyPlayer;
    expect(provisionReady(p, Date.parse(p.provision_at!) + 3599999)).toBe(0);
    expect(provisionReady(p, Date.parse(p.provision_at!) + 100 * 3600000)).toBe(120);
    expect(
      provisionReady({ ...p, provision_claimed: 120 }, Date.parse(p.provision_at!) + 100 * 3600000),
    ).toBe(0);
  });
  it("hides old season progress and strips read-only fields from settings", () => {
    expect(seasonProgress({ season: 202612, player: { season: 202611, season_xp: 500 } } as Economy)).toBe(0);
    expect(
      settingsDraft({
        pool_total: 50,
        revision: 9,
        pool_goal: 10000,
        shop_enabled: true,
        shop_min_days: 2,
        shop_min_level: 5,
      }),
    ).toEqual({ pool_goal: 10000, shop_enabled: true, shop_min_days: 2, shop_min_level: 5 });
  });
});
