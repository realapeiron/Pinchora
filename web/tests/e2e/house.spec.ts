// The read-only house page in a real browser: a furnished house drawn by the TypeScript renderer, checked
// against the picture the bot draws, a visit to another player's house, a locked door, and the phone layout.
// The house comes from `cargo test -p pinchora-web --test house seed_demo_house -- --ignored`, run first.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

// The ids the seed uses.
const HOST = "770000000000000001";
const LOCKED = "770000000000000002";
const EMPTY = "770000000000000003";

test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "house", "seed_demo_house", "--", "--ignored"],
    {
      cwd: fileURLToPath(new URL("../../..", import.meta.url)),
      stdio: "inherit",
    },
  );
});

/** Waits until the room is drawn: the canvas has pixels that are not the empty one. */
async function drawn(page: Page) {
  await expect(page.locator(".room-canvas").first()).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const canvas = document.querySelector<HTMLCanvasElement>(".room-canvas");
        const ctx = canvas?.getContext("2d");
        if (!canvas || !ctx) return 0;
        const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
        let opaque = 0;
        for (let i = 3; i < data.length; i += 4) if (data[i] === 255) opaque++;
        return opaque;
      }),
    )
    .toBeGreaterThan(1000);
}

/** The piece under a point on the room, by name, found by sweeping the canvas. */
async function pieceAt(page: Page, fx: number, fy: number) {
  const box = await page.locator(".room-canvas").first().boundingBox();
  if (!box) throw new Error("no canvas");
  await page.mouse.move(box.x + box.width * fx, box.y + box.height * fy);
  return page.locator(".house-info-name");
}

test("my house: rooms, hover names, the grid, and identical to the bot's own picture", async ({ page }) => {
  await devLogin(page, HOST, []);
  await page.goto("/me/house");
  await expect(page.getByRole("heading", { name: "MY HOUSE", exact: true })).toBeVisible();
  await expect(page.getByText("Reef House · 3 rooms · 12 likes")).toBeVisible();
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(3);
  await drawn(page);
  await expect(page.getByRole("img", { name: /21 items/ })).toBeVisible();
  await shot(page, "house-desktop", true);

  // The browser's room next to the bot's own picture of it, for people (and agents) to compare by eye.
  const room = await (await page.request.get("/api/house/rooms/0")).json();
  const botNative = await page.request.get(`/api/house/rooms/0/render.png?scale=1&seed=${room.seed}`);
  const botBig = await page.request.get(`/api/house/rooms/0/render.png?scale=2&seed=${room.seed}`);
  const mine = await page.evaluate(
    () => document.querySelector<HTMLCanvasElement>(".room-canvas")?.toDataURL("image/png") ?? "",
  );
  mkdirSync("test-results/screens", { recursive: true });
  writeFileSync("test-results/screens/house-bot-1x.png", await botNative.body());
  writeFileSync("test-results/screens/house-bot-2x.png", await botBig.body());
  writeFileSync("test-results/screens/house-browser-1x.png", Buffer.from(mine.split(",")[1] ?? "", "base64"));

  // The piano stands in the left of the room.
  const name = await pieceAt(page, 0.3, 0.37);
  await expect(name).toHaveText("Humskein Upright (Plum)");
  await shot(page, "house-hover", true);

  // What is in the room, by name.
  await expect(page.getByText("Plainwhorl Chair (Sand)")).toBeVisible();
  await expect(page.getByText("Character: captain_crab")).toBeVisible();
  await expect(page.getByText("×2").first()).toBeVisible();

  // The tile grid adds the coordinates along the front edges, and still comes out the same as the bot's.
  await page.getByRole("button", { name: "Tile grid" }).click();
  await drawn(page);
  await shot(page, "house-grid");
  await page.getByRole("button", { name: "Tile grid" }).click();

  // The browser's picture is the bot's picture, pixel for pixel: the page checks it itself.
  await page.getByRole("button", { name: "Compare with Discord" }).click();
  await expect(page.getByRole("status").filter({ hasText: "identical to this view" })).toBeVisible();
  await shot(page, "house-compare");
  await page.getByRole("button", { name: "Back to this view" }).click();

  // The second room.
  await tabs.nth(1).click();
  await expect(page).toHaveURL(/room=1/);
  await expect(page.getByText("Patchpillow Bed (Cream)")).toBeVisible();
  await drawn(page);
  await shot(page, "house-room2");

  // Zoom keeps whole pixels: the canvas is shown at a whole multiple of its own size.
  await page.getByRole("button", { name: "Zoom in" }).click();
  const sizes = await page.evaluate(() => {
    const canvas = document.querySelector<HTMLCanvasElement>(".room-canvas");
    const rect = canvas?.getBoundingClientRect();
    return { w: canvas?.width ?? 0, shown: rect?.width ?? 0 };
  });
  expect(sizes.shown / sizes.w).toBe(Math.round(sizes.shown / sizes.w));
});

test("visiting: another player's house, a locked door, and an empty room", async ({ page }) => {
  await devLogin(page, EMPTY, []);
  await page.goto(`/house/${HOST}`);
  await expect(page.getByRole("heading", { name: "CAPTAIN CRAB'S HOUSE", exact: true })).toBeVisible();
  await drawn(page);
  await shot(page, "house-visit");

  await page.goto(`/house/${LOCKED}`);
  await expect(page.getByText("Shy Shrimp has locked their house.")).toBeVisible();
  await shot(page, "house-locked");

  await page.goto("/me/house");
  await drawn(page);
  await expect(page.getByText("Nothing yet.")).toBeVisible();
  await shot(page, "house-empty");
});

test("the house on a phone: no sideways page scroll, the room pans inside its frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await devLogin(page, HOST, []);
  await page.goto("/me/house");
  await drawn(page);
  await shot(page, "house-phone", true);
  const wide = await page.evaluate(() =>
    [...document.querySelectorAll("body *")]
      .filter((el) => el.getBoundingClientRect().right > window.innerWidth + 1)
      .filter((el) => !el.closest(".room-viewport, .house-tabs"))
      .map((el) => `${el.tagName}.${el.className}@${Math.round(el.getBoundingClientRect().right)}`)
      .slice(0, 6),
  );
  expect(await overflow(page), wide.join(", ")).toBeLessThanOrEqual(0);
  const frame = await page.locator(".room-viewport").evaluate((el) => ({
    scrollable: el.scrollWidth > el.clientWidth,
    width: el.clientWidth,
  }));
  expect(frame.width).toBeLessThanOrEqual(390);
  expect(frame.scrollable).toBe(true);
});
