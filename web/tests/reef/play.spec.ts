import { expect, test } from "@playwright/test";
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
test("cold play, keyboard walking, seat, use and unreachable tile", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (r) => requests.push(r.url()));
  await page.goto("/me/house?play=1");
  const canvas = page.locator(".reef-canvas");
  await expect(canvas).toHaveAttribute("data-ready", "true");
  expect(requests.some((u) => /\/atlas\.(png|json)/.test(u))).toBe(false);
  const budget = await page.evaluate(() =>
    performance
      .getEntriesByType("resource")
      .reduce((sum, r) => sum + (r as PerformanceResourceTiming).encodedBodySize, 0),
  );
  console.log(`REEF cold response bodies to ready: ${budget} bytes`);
  expect(budget).toBeLessThanOrEqual(2_000_000);
  await canvas.focus();
  await page.keyboard.press("ArrowRight");
  await expect(canvas).toHaveAttribute("data-tile", "1,11");
  await page.getByText("Furniture and actions").click();
  await page.getByRole("button", { name: "Sit", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-state", "sit");
  await page.getByRole("button", { name: "Use", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-state", "use");
  await expect(canvas).toHaveAttribute("data-state", "idle");
  await page.getByLabel("Tile X").fill("2");
  await page.getByLabel("Tile Y").fill("10");
  await page.getByRole("button", { name: "Walk to tile" }).click();
  await expect(page.getByRole("status").filter({ hasText: "unreachable" })).toBeVisible();
  await expect(canvas).toHaveAttribute("data-frame-p95", /.+/);
  console.log(`REEF frame CPU p95: ${await canvas.getAttribute("data-frame-p95")} ms`);
  await page.screenshot({ path: "test-results/reef/play-phone.png", fullPage: true });
});
test("touch drag cannot walk; reduced motion keyboard still moves", async ({ page, context }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/me/house?play=1");
  const canvas = page.locator(".reef-canvas");
  await expect(canvas).toHaveAttribute("data-ready", "true");
  const start = await canvas.getAttribute("data-tile");
  await canvas.scrollIntoViewIfNeeded();
  const box = (await canvas.boundingBox())!;
  const cdp = await context.newCDPSession(page);
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: box.x + 100, y: box.y + 100, id: 1 }],
  });
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchMove",
    touchPoints: [{ x: box.x + 190, y: box.y + 130, id: 1 }],
  });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await expect(canvas).toHaveAttribute("data-tile", start!);
  await canvas.focus();
  await page.keyboard.press("ArrowUp");
  await expect(canvas).toHaveAttribute("data-tile", "0,10");
});
