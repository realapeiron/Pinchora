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

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import Board from "./Board";
import type { Game } from "./types";
it("keeps every board square keyboard reachable during pending moves and spectating", () => {
  for (const kind of ["chess", "checkers"] as const) {
    for (const [busy, side] of [
      [true, 0],
      [false, null],
      [false, 1],
    ] as const) {
      const game: Game = {
        id: "1",
        room: "house:1:0",
        kind,
        revision: "2",
        host_id: "1",
        deadline: "",
        practice: true,
        side,
        answered: false,
        placement_id: null,
        players: [],
        board: { phase: "active", turn: 0, winner: null, reason: null },
      };
      const html = renderToStaticMarkup(createElement(Board, { game, busy, send: () => undefined }));
      const squares = html.match(/<button[^>]+data-square[^>]+>/g) ?? [];
      expect(squares).toHaveLength(kind === "chess" ? 64 : 32);
      for (const square of squares) {
        expect(square).toContain('aria-disabled="true"');
        expect(square).not.toMatch(/\sdisabled=/);
        expect(square).not.toContain('tabindex="-1"');
      }
    }
  }
});
