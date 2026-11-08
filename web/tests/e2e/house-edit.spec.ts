import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const HOST = "770000000000000001";
test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "house", "seed_demo_house", "--", "--ignored"],
    { cwd: fileURLToPath(new URL("../../..", import.meta.url)), stdio: "inherit" },
  );
});
test("browser house editor: inventory, drag, turn, shared undo, redo and stale room", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await devLogin(page, "770000000000000003", []);
  await page.goto("/me/house");
  await page.getByRole("button", { name: "Decorate house", exact: true }).click();
  await expect(page.getByRole("region", { name: "House editor" })).toBeVisible();
  await page.getByLabel("Search inventory").fill("Plainwhorl");
  await page.locator(".house-stock button").first().click();
  const placedResponse = page.waitForResponse(
    (r) => r.url().includes("/ops") && r.request().postDataJSON()?.op.type === "place",
  );
  await page.getByRole("button", { name: "Place", exact: true }).click();
  const placed = await (await placedResponse).json();
  const id = placed.placements[0].id;
  const pieces = page.getByLabel("Select a placed piece");
  await pieces.selectOption(id);
  const turnResponse = page.waitForResponse(
    (r) => r.url().includes("/ops") && r.request().postDataJSON()?.op.type === "flip",
  );
  await page.getByRole("button", { name: "Turn / flip" }).click();
  expect((await turnResponse).ok()).toBeTruthy();
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByRole("button", { name: "Redo", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(page.getByRole("button", { name: "Move selected piece" })).toBeEnabled();
  await page.getByRole("button", { name: "Move selected piece" }).click();
  const canvas = page.locator(".room-canvas").first();
  const box = await canvas.boundingBox();
  const nativeWidth = await canvas.evaluate((el) => (el as HTMLCanvasElement).width);
  const scale = box!.width / nativeWidth;
  // Empty 8×8 room with grid: origin (284,121); tile (3,2) centre is (316,217).
  const movedResponse = page.waitForResponse(
    (r) => r.url().includes("/ops") && r.request().postDataJSON()?.op.type === "move",
  );
  await page.mouse.move(box!.x + 284 * scale, box!.y + 137 * scale);
  await page.mouse.down();
  await page.mouse.move(box!.x + 316 * scale, box!.y + 217 * scale, { steps: 12 });
  await page.mouse.up();
  const moved = await movedResponse;
  expect(moved.ok(), await moved.text()).toBeTruthy();
  const newId = (await moved.json()).placements[0].id;
  await expect(pieces).toHaveValue(newId);
  await page.route(
    "**/api/house/rooms/0/ops",
    async (route) => {
      await route.fulfill({
        status: 409,
        contentType: "application/json",
        body: JSON.stringify({
          code: "room_changed",
          placements: [],
          fingerprint: "changed",
          error: { code: "room_changed", message: "This room changed somewhere else (Discord?), reloaded." },
        }),
      });
    },
    { times: 1 },
  );
  await page.getByRole("button", { name: "Store", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "This room changed somewhere else" }),
  ).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "house-editor-desktop", true);
});
test("phone house is centered, pannable view mode without overflow", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await devLogin(page, HOST, []);
  await page.goto("/me/house");
  await expect(page.locator(".room-canvas").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Decorate house", exact: true })).toHaveCount(0);
  await expect(page.getByText("Drag to explore. Use + and − to zoom.")).toBeVisible();
  expect(await page.locator(".room-viewport").evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
  await page.getByRole("button", { name: "Zoom in", exact: true }).click();
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "house-editor-phone", true);
});

test("14×12 crowded room keeps pointer previews within a frame budget", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await devLogin(page, HOST, []);
  const { readFileSync } = await import("node:fs");
  const fixture = JSON.parse(
    readFileSync(
      fileURLToPath(new URL("../../src/iso/fixtures/crowded_14x12.json", import.meta.url)),
      "utf8",
    ),
  );
  const room = { ...fixture.room, fingerprint: "performance-fixture", undo: [] };
  await page.route("**/api/house/rooms/0", (route) => route.fulfill({ json: room }));
  await page.goto("/me/house");
  await page.getByRole("button", { name: "Decorate house", exact: true }).click();
  const pieces = page.getByLabel("Select a placed piece");
  await pieces.selectOption(room.placements[0].id);
  await page.getByRole("button", { name: "Move selected piece" }).click();
  const samples = await page.evaluate(async () => {
    const viewport = document.querySelector<HTMLElement>(".room-viewport")!;
    const rect = viewport.getBoundingClientRect();
    const frames: number[] = [];
    let previous = performance.now();
    for (let i = 0; i < 120; i++) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const now = performance.now();
      if (i > 10) frames.push(now - previous);
      previous = now;
      viewport.dispatchEvent(
        new PointerEvent("pointermove", {
          bubbles: true,
          clientX: rect.x + 100 + i * 3,
          clientY: rect.y + 200 + (i % 20),
          pointerType: "mouse",
        }),
      );
    }
    return frames;
  });
  const sorted = samples.sort((a, b) => a - b);
  // 60 Hz is ~16.7 ms; allow scheduling noise but catch repeatedly dropped frames.
  expect(sorted[Math.floor(sorted.length * 0.95)]).toBeLessThan(25);
});
