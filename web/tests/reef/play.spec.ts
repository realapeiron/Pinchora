import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import type { Catalog } from "../../src/iso/content";
import type { RoomJson } from "../../src/iso/scene";
const catalog: Catalog = JSON.parse(
  readFileSync(new URL("../../src/iso/fixtures/content.json", import.meta.url), "utf8"),
);
// Mirror the production furniture projection, not the smaller renderer-fixture catalogue.
const fullFurniture = JSON.parse(
  readFileSync(new URL("../../../content/furniture.json", import.meta.url), "utf8"),
) as Catalog["furniture"][number][];
catalog.furniture = fullFurniture.map((f) => ({
  id: f.id,
  name: f.name,
  category: f.category,
  layer: f.layer,
  footprint: f.footprint,
  rarity: f.rarity,
  iso_width: f.iso_width ?? null,
  surface_height: f.surface_height ?? null,
  tags: f.tags,
  variants: f.variants,
}));
const item = (id: string) => {
  const base = catalog.furniture.find((f) => f.id === id)!;
  return base.variants.length ? `${id}__${base.variants[0]}` : id;
};
const room: RoomJson = {
  idx: 0,
  room_type: "living",
  width: 14,
  depth: 12,
  wallpaper: "creamcalm_plaster",
  floor: "sandsettle_boards",
  seed: 0,
  pets: [],
  characters: ["bobbin.1.11.1.2"],
  placements: [
    { id: "seat", item_id: item("pebblerest_stool"), x: 2, y: 10, flipped: false },
    { id: "game", item_id: item("blinkslot_cabinet"), x: 5, y: 9, flipped: false },
  ],
};
test.beforeEach(async ({ page }) => {
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const body =
      path === "/api/me"
        ? {
            user: { id: "123456789012345678", username: "Reef player", global_name: null, avatar_url: "" },
            csrf: "test",
            capabilities: { message_content: false },
          }
        : path === "/api/house/content"
          ? catalog
          : path === "/api/house/rooms/0"
            ? room
            : path === "/api/house"
              ? {
                  scope: "own",
                  owner: { id: "123456789012345678", name: "Reef player", you: true },
                  house: { name: "Home", tier: 3, tier_name: "Reef", likes: 0, locked: false },
                  width: 14,
                  depth: 12,
                  rooms: [
                    {
                      idx: 0,
                      room_type: "living",
                      name: "Living",
                      wallpaper: room.wallpaper,
                      floor: room.floor,
                      items: 2,
                    },
                  ],
                }
              : null;
    await route.fulfill({
      status: body ? 200 : 404,
      contentType: "application/json",
      body: JSON.stringify(body),
    });
  });
  await page.route("**/assets/fonts/**", (route) => {
    const name = new URL(route.request().url()).pathname.split("/").pop()!;
    return route.fulfill({
      status: 200,
      contentType: "font/ttf",
      body: readFileSync(new URL(`../../../assets/fonts/${name}`, import.meta.url)),
    });
  });
});
async function openControls(page: Page) {
  const toggle = page.getByRole("button", { name: "Controls", exact: true });
  if ((await toggle.getAttribute("aria-expanded")) === "false") await toggle.click();
}
async function roomReady(page: Page) {
  const canvas = page.locator(".reef-canvas");
  await expect(canvas).toHaveAttribute("data-ready", "true");
  await expect(canvas).toHaveAttribute("data-art-ready", "true");
  await expect(canvas).toHaveAttribute("data-camera-settled", "true");
  await page.evaluate(() => document.fonts.ready);
  return canvas;
}
async function assertViewport(page: Page) {
  const header = (await page.locator(".top-bar").boundingBox())!;
  const canvas = (await page.locator(".reef-canvas").boundingBox())!;
  expect(canvas.y).toBeGreaterThanOrEqual(header.y + header.height - 1);
  expect(canvas.y + canvas.height).toBeCloseTo(page.viewportSize()!.height, 0);
  expect(canvas.width).toBeGreaterThanOrEqual(page.viewportSize()!.width < 768 ? 389 : 950);
  const overflow = await page.evaluate(() => ({
    x: window.scrollX,
    y: window.scrollY,
    width: document.documentElement.scrollWidth - innerWidth,
    height: document.documentElement.scrollHeight - innerHeight,
  }));
  expect(overflow).toEqual({ x: 0, y: 0, width: 0, height: 0 });
  await expect(page.locator(".reef-name-tag")).toBeInViewport({ ratio: 0.95 });
}

test("cold play, walk and seated screenshots; keyboard, actions and budgets", async ({ page }, testInfo) => {
  const requests: string[] = [];
  page.on("request", (r) => requests.push(r.url()));
  await page.goto("/me/house?play=1");
  const canvas = await roomReady(page);
  const phone = testInfo.project.name === "phone";
  await expect(page.getByRole("button", { name: "Controls", exact: true })).toHaveAttribute(
    "aria-expanded",
    String(!phone),
  );
  await assertViewport(page);
  const fit = await canvas.evaluate((node: HTMLCanvasElement) => {
    const [x, y] = node.dataset.camera!.split(",").map(Number);
    const [width, height] = node.dataset.roomSize!.split(",").map(Number);
    const scale = Number(node.dataset.scale);
    return {
      x,
      y,
      right: x! + width! * scale,
      bottom: y! + height! * scale,
      width: node.width,
      height: node.height,
      scale,
    };
  });
  expect(fit.x).toBeGreaterThanOrEqual(23 * (phone ? 2.625 : 1));
  expect(fit.y).toBeGreaterThanOrEqual(23 * (phone ? 2.625 : 1));
  expect(fit.right).toBeLessThanOrEqual(fit.width - 23 * (phone ? 2.625 : 1));
  expect(fit.bottom).toBeLessThanOrEqual(fit.height - 23 * (phone ? 2.625 : 1));
  expect(fit.scale).toBe(1);
  await page.screenshot({ path: `test-results/reef/play-${testInfo.project.name}-open.png`, scale: "css" });
  expect(requests.some((u) => /\/atlas\.(png|json)/.test(u))).toBe(false);
  const budget = await page.evaluate(() =>
    performance
      .getEntriesByType("resource")
      .reduce((sum, r) => sum + (r as PerformanceResourceTiming).encodedBodySize, 0),
  );
  console.log(`REEF ${testInfo.project.name} cold response bodies to ready: ${budget} bytes`);
  expect(budget).toBeLessThanOrEqual(2_000_000);

  await canvas.focus();
  await page.keyboard.press("ArrowRight");
  await expect(canvas).toHaveAttribute("data-tile", "1,11");
  await expect(canvas).toHaveAttribute("data-state", "idle");
  await expect(canvas).toHaveAttribute("data-camera-settled", "true");
  await assertViewport(page);
  await page.screenshot({ path: `test-results/reef/play-${testInfo.project.name}-walk.png`, scale: "css" });

  await openControls(page);
  await page.locator("#reef-controls").getByRole("button", { name: "Sit", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-state", "sit");
  if (phone) await page.getByRole("button", { name: "Close controls" }).click();
  await expect(canvas).toHaveAttribute("data-camera-settled", "true");
  await assertViewport(page);
  await page.screenshot({ path: `test-results/reef/play-${testInfo.project.name}-seated.png`, scale: "css" });

  await openControls(page);
  await page.getByRole("button", { name: "Use", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-state", "use");
  await expect(canvas).toHaveAttribute("data-state", "idle");
  await page.getByLabel("Tile X").fill("2");
  await page.getByLabel("Tile Y").fill("10");
  await page.getByRole("button", { name: "Walk to tile" }).click();
  await expect(page.getByRole("status").filter({ hasText: "unreachable" })).toBeVisible();
  await expect(canvas).toHaveAttribute("data-frame-p95", /.+/);
  const p95 = Number(await canvas.getAttribute("data-frame-p95"));
  const cache = Number(await canvas.getAttribute("data-cache-bytes"));
  console.log(`REEF ${testInfo.project.name} frame CPU p95: ${p95} ms; cache: ${cache} bytes`);
  expect(p95).toBeLessThanOrEqual(16.7);
  expect(cache).toBeLessThanOrEqual(16 * 1024 * 1024);
});

test("drawer returns focus, zoom remains integer and reduced motion centres instantly", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/me/house?play=1");
  const canvas = await roomReady(page);
  await openControls(page);
  await page.getByRole("button", { name: "Close controls" }).click();
  const toggle = page.getByRole("button", { name: "Controls", exact: true });
  await expect(toggle).toBeFocused();
  await toggle.click();
  await expect(page.getByRole("button", { name: "Close controls" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(toggle).toBeFocused();
  await expect(page.locator("#reef-controls")).toBeHidden();
  for (let i = 0; i < 5; i++) await page.getByRole("button", { name: "Zoom in", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-scale", "4");
  await canvas.focus();
  await page.keyboard.press("ArrowUp");
  await expect(canvas).toHaveAttribute("data-tile", "0,10");
  await expect(canvas).toHaveAttribute("data-state", "idle");
  await page.getByRole("button", { name: "Centre on me" }).click();
  const positions = await canvas.evaluate(async (node) => {
    await new Promise(requestAnimationFrame);
    const first = node.getAttribute("data-camera");
    await new Promise(requestAnimationFrame);
    return [first, node.getAttribute("data-camera")];
  });
  expect(positions[0]).toBe(positions[1]);
  await expect(page.locator(".reef-name-tag")).toBeInViewport();
  for (let i = 0; i < 5; i++) await page.getByRole("button", { name: "Zoom out", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-scale", "1");
  await page.getByRole("button", { name: "View", exact: true }).click();
  await expect(page.locator(".house-playing")).toHaveCount(0);
  expect(await page.evaluate(() => document.body.style.overflow)).not.toBe("hidden");
});

test("touch drag and pinch never walk and pinch uses integer steps", async ({ page, context }, testInfo) => {
  test.skip(testInfo.project.name !== "phone", "Touch gestures use the phone project.");
  await page.goto("/me/house?play=1");
  const canvas = await roomReady(page);
  const start = await canvas.getAttribute("data-tile");
  const box = (await canvas.boundingBox())!;
  const cdp = await context.newCDPSession(page);
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: box.x + 100, y: box.y + 260, id: 1 }],
  });
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchMove",
    touchPoints: [{ x: box.x + 190, y: box.y + 290, id: 1 }],
  });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await expect(canvas).toHaveAttribute("data-tile", start!);
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [
      { x: box.x + 130, y: box.y + 260, id: 1 },
      { x: box.x + 220, y: box.y + 260, id: 2 },
    ],
  });
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchMove",
    touchPoints: [
      { x: box.x + 70, y: box.y + 260, id: 1 },
      { x: box.x + 280, y: box.y + 260, id: 2 },
    ],
  });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await expect(canvas).toHaveAttribute("data-scale", /^[234]$/);
  await expect(canvas).toHaveAttribute("data-tile", start!);
});

test.describe("Turkish context actions", () => {
  test.use({ locale: "tr-TR" });
  test("tapping a seat offers Otur before walking", async ({ page }) => {
    await page.goto("/me/house?play=1");
    const canvas = await roomReady(page);
    const start = await canvas.getAttribute("data-tile");
    // Centre of the fixture stool's cushion: native 64×73 sprite, top = tile y + 32 − 73.
    const point = await canvas.evaluate((node: HTMLCanvasElement) => {
      const [ox, oy] = node.dataset.origin!.split(",").map(Number);
      const [cx, cy] = node.dataset.camera!.split(",").map(Number);
      const scale = Number(node.dataset.scale);
      return {
        x: (cx! + (ox! + 32 * (2 - 10)) * scale) / devicePixelRatio,
        y: (cy! + (oy! + 16 * (2 + 10) - 25) * scale) / devicePixelRatio,
      };
    });
    await canvas.click({ position: point });
    const bubble = page.locator(".reef-context");
    await expect(bubble.getByRole("button", { name: "Otur", exact: true })).toBeVisible();
    await expect(canvas).toHaveAttribute("data-tile", start!);
    await bubble.getByRole("button", { name: "Otur", exact: true }).click();
    await expect(canvas).toHaveAttribute("data-state", "sit");
    await expect(bubble).toBeHidden();
    await expect(canvas).toBeFocused();
    await expect(page.getByRole("button", { name: "Erişilebilir kontroller", exact: true })).toBeVisible();
  });
});
