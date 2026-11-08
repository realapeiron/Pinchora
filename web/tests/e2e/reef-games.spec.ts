// Real server and two browser contexts. The lead runs Chromium.
import { randomBytes } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test as base, expect, type Page } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";
import type { Game } from "../../src/reef/games/types";
import type { Puzzle } from "../../src/reef/games/Picross";
const test = base.extend<{ players: { host: string; guest: string } }>({
  // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature.
  players: async ({}, use) => {
    const guild = 770000000000000000n + BigInt(randomBytes(6).readUIntBE(0, 6)) * 10n;
    execFileSync(
      "cargo",
      ["test", "-p", "pinchora-web", "--test", "reef", "seed_reef_wave7", "--", "--ignored"],
      {
        cwd: fileURLToPath(new URL("../../..", import.meta.url)),
        stdio: "inherit",
        env: { ...process.env, CARGO_BUILD_JOBS: "4", PINCHORA_E2E_REEF_GUILD_ID: String(guild) },
      },
    );
    await use({ host: String(guild + 1n), guest: String(guild + 2n) });
  },
});
test.setTimeout(600000);
async function room(page: Page, host: string) {
  await page.goto(`/house/${host}?play=1`);
  await expect(page.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
  await page.getByRole("button", { name: "Room activities", exact: true }).click();
}
async function write<T>(page: Page, path: string, data: unknown): Promise<T> {
  const me = await (await page.request.get("/api/me")).json();
  const response = await page.request.post(path, { headers: { "X-CSRF-Token": me.csrf }, data });
  expect(response.ok(), await response.text()).toBe(true);
  return response.json() as Promise<T>;
}
async function games(page: Page, host: string): Promise<Game[]> {
  return (await (await page.request.get(`/api/reef/rooms/house:${host}:0/games`)).json()).games;
}
async function select(page: Page, g: Game) {
  await page.getByRole("button", { name: new RegExp(` · ${g.id} · `) }).click();
}
for (const kind of ["checkers", "chess", "trivia"] as const)
  test(`two browsers complete ${kind}, spectators and reconnect`, async ({ browser, baseURL, players }) => {
    const a = await browser.newContext({ baseURL, viewport: { width: 390, height: 844 }, hasTouch: true });
    const b = await browser.newContext({ baseURL });
    try {
      const host = await a.newPage(),
        guest = await b.newPage();
      await devLogin(host, players.host, [], "Reef host");
      await devLogin(guest, players.guest, [], "Reef guest");
      await room(host, players.host);
      await room(guest, players.host);
      await host
        .getByRole("button", {
          name: kind === "checkers" ? "Pearl Checkers" : kind === "chess" ? "Reef Chess" : "Trivia",
          exact: true,
        })
        .click();
      await expect.poll(async () => (await games(host, players.host)).length).toBeGreaterThan(0);
      let g = (await games(host, players.host))[0]!;
      await select(guest, g);
      await expect(guest.getByText("Spectating", { exact: true }).first()).toBeVisible();
      await guest.getByRole("button", { name: "Join game", exact: true }).click();
      await expect.poll(async () => (await games(host, players.host))[0]!.players.length).toBe(2);
      g = (await games(host, players.host))[0]!;
      if (kind === "trivia") {
        await host.getByRole("button", { name: "Start round", exact: true }).click();
        await expect.poll(async () => (await games(host, players.host))[0]!.board.phase).toBe("active");
        g = (await games(host, players.host))[0]!;
        expect(g.board.correct).toBeUndefined();
        await host.getByRole("button", { name: g.board.options![0]!, exact: true }).click();
        await host.reload();
        await expect(host.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
        await host.getByRole("button", { name: "Room activities", exact: true }).click();
        await select(host, g);
        await expect(host.getByText("Answer locked. Waiting for the reveal.")).toBeVisible();
        await guest.getByRole("button", { name: g.board.options![1]!, exact: true }).click();
      } else {
        const chess = ["f2f3", "e7e5", "g2g4", "d8h4"];
        for (let ply = 0; ply < 400; ply++) {
          g = (await games(host, players.host))[0]!;
          if (g.board.phase === "done") break;
          const player = g.board.turn === 0 ? host : guest;
          const code = kind === "chess" ? chess[ply]! : g.board.legal![0]!.code;
          const menu = player.getByRole("combobox", { name: "Legal moves", exact: true });
          await expect(menu).toBeEnabled();
          await expect(menu.locator(`option[value="${code}"]`)).toHaveCount(1);
          if (ply === 0) {
            const move = g.board.legal!.find((m) => m.code === code)!;
            const source = player.locator(`[data-square="${move.from}"]`);
            const target = player.locator(`[data-square="${move.to}"]`);
            await source.focus();
            await player.keyboard.press("Enter");
            await target.focus();
            await player.keyboard.press("Space");
            await expect(target).toBeFocused();
          } else {
            await menu.selectOption(code);
          }
          await expect.poll(async () => (await games(host, players.host))[0]!.revision).not.toBe(g.revision);
        }
      }
      await expect.poll(async () => (await games(host, players.host))[0]!.board.phase).toBe("done");
      g = (await games(host, players.host))[0]!;
      expect(g.players.every((p) => p.paid === 0)).toBe(true);
      if (kind === "trivia") expect(g.board.correct).toBeDefined();
      if (kind === "chess") expect(g.board.winner).toBe(1);
      await expect(host.locator("h1")).toHaveCount(1);
      expect(await overflow(host)).toBeLessThanOrEqual(1);
      await shot(host, `reef-games-${kind}-phone`);
      await shot(guest, `reef-games-${kind}-desktop`);
    } finally {
      await a.close();
      await b.close();
    }
  });
// Solve public clues, not a private server solution or a content fixture.
function solve(p: Puzzle): boolean[] {
  const patterns = (clues: number[]) => {
    const out: boolean[][] = [];
    for (let mask = 0; mask < 1024; mask++) {
      const row = Array.from({ length: 10 }, (_, i) => !!(mask & (1 << i)));
      const runs: number[] = [];
      let n = 0;
      for (const v of [...row, false]) {
        if (v) n++;
        else if (n) {
          runs.push(n);
          n = 0;
        }
      }
      if (JSON.stringify(runs.length ? runs : [0]) === JSON.stringify(clues.length ? clues : [0]))
        out.push(row);
    }
    return out;
  };
  const rows = p.rows.map(patterns),
    cols = p.cols.map(patterns);
  const chosen: boolean[][] = [];
  const walk = (r: number): boolean => {
    if (r === 10) return true;
    for (const row of rows[r]!) {
      chosen[r] = row;
      if (
        cols.every((options, c) =>
          options.some((column) => chosen.slice(0, r + 1).every((line, i) => line[c] === column[i])),
        ) &&
        walk(r + 1)
      )
        return true;
    }
    return false;
  };
  if (!walk(0)) throw new Error("No solution from public clues");
  return chosen.flat();
}
test("two browsers resume and complete the same daily Picross", async ({ browser, baseURL, players }) => {
  const a = await browser.newContext({ baseURL }),
    b = await browser.newContext({ baseURL, viewport: { width: 390, height: 844 }, hasTouch: true });
  try {
    const first = await a.newPage(),
      second = await b.newPage();
    await devLogin(first, players.host, [], "Picross player");
    await room(first, players.host);
    await first.getByRole("button", { name: "Pixel Picross", exact: true }).click();
    const path = `/api/reef/rooms/house:${players.host}:0/picross`;
    let p = await write<Puzzle>(first, path, { mode: "daily" });
    const solution = solve(p);
    const cells = solution.flatMap((filled, cell) => (filled ? [cell] : []));
    const firstCell = cells.shift()!;
    await first.locator(`[data-cell="${firstCell}"]`).focus();
    await first.keyboard.press("Space");
    await expect(first.locator(`[data-cell="${firstCell}"]`)).toHaveAttribute("aria-pressed", "true");
    await expect(first.locator(`[data-cell="${firstCell}"]`)).toBeFocused();
    await first.keyboard.press("ArrowRight");
    await expect(first.locator(`[data-cell="${Math.min(99, firstCell + 1)}"]`)).toBeFocused();
    await first.goto("/me");
    await devLogin(second, players.host, [], "Picross player");
    await room(second, players.host);
    await second.getByRole("button", { name: "Pixel Picross", exact: true }).click();
    await expect(second.locator(`[data-cell="${firstCell}"]`)).toHaveAttribute("aria-pressed", "true");
    for (const cell of cells) {
      const button = second.locator(`[data-cell="${cell}"]`);
      await expect(button).toBeEnabled();
      await button.click();
      await expect(button).toHaveAttribute("aria-pressed", "true");
    }
    p = await write<Puzzle>(second, path, { mode: "daily" });
    expect(p.status).toBe("solved");
    expect(p.paid).toBe(0);
    await expect(second.getByText(/Puzzle solved!/)).toBeVisible();
    expect(await overflow(second)).toBeLessThanOrEqual(1);
    await shot(second, "reef-games-picross-phone");
    const resumed = await write<Puzzle>(first, path, { mode: "daily" });
    expect(resumed.grid).toBe(p.grid);
    expect(resumed.paid).toBe(p.paid);
  } finally {
    await a.close();
    await b.close();
  }
});
