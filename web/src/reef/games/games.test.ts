import { describe, expect, it } from "vitest";
import { chessCells } from "./Board";
import { gameEn, gameTr } from "../../i18n/reef-games";
describe("room games", () => {
  it("expands authoritative FEN for accessible board squares", () => {
    const cells = chessCells("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    expect(cells).toHaveLength(64);
    expect(cells[0]).toBe("r");
    expect(cells[60]).toBe("K");
    expect(cells.filter((c) => c === ".")).toHaveLength(32);
  });
  it("keeps the deferred EN and TR catalogs complete", () => {
    expect(Object.keys(gameEn).sort()).toEqual(Object.keys(gameTr).sort());
    for (const key of Object.keys(gameEn) as (keyof typeof gameEn)[])
      expect(gameTr[key].match(/\{\w+\}/g) ?? []).toEqual(gameEn[key].match(/\{\w+\}/g) ?? []);
  });
});

import { economyKey } from "./economy";
it("refreshes the actual player, house, wallet, inventory and pet query families", () => {
  for (const key of [
    ["player", "wallet"],
    ["player", "pets"],
    ["economy", "overview"],
    ["/api/house"],
    ["/api/me"],
    ["inventory"],
  ])
    expect(economyKey(key)).toBe(true);
  expect(economyKey(["reef-games", "house:1:0"])).toBe(false);
});
