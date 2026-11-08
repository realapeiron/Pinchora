// Real API, Redis actors and two authenticated browsers. The lead runs Chromium.
import { randomBytes } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test as base, expect, type Page } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";
const test = base.extend<{ reef: { HOST: string; GUEST: string; GUILD: string } }>({
  // eslint-disable-next-line no-empty-pattern -- Playwright requires destructuring fixture dependencies.
  reef: async ({}, use) => {
    // Account join budgets live in the server, not in the database seed. Isolate
    // tests/retries without bypassing the six-joins-per-minute production limit.
    // End guild IDs in 0: the Discord mock treats IDs ending in 9 as bot-absent.
    const guild = 770000000000000000n + BigInt(randomBytes(6).readUIntBE(0, 6)) * 10n;
    const GUILD = String(guild),
      HOST = String(guild + 1n),
      GUEST = String(guild + 2n);
    execFileSync(
      "cargo",
      ["test", "-p", "pinchora-web", "--test", "reef", "seed_reef_wave7", "--", "--ignored"],
      {
        cwd: fileURLToPath(new URL("../../..", import.meta.url)),
        stdio: "inherit",
        env: { ...process.env, CARGO_BUILD_JOBS: "4", PINCHORA_E2E_REEF_GUILD_ID: GUILD },
      },
    );
    await use({ HOST, GUEST, GUILD });
  },
});
test.setTimeout(90_000);
async function write(page: Page, path: string, method: string, data: unknown) {
  const me = await (await page.request.get("/api/me")).json();
  return page.request.fetch(path, { method, headers: { "X-CSRF-Token": me.csrf }, data });
}
async function ready(page: Page) {
  await expect(page.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
}
for (const phone of [false, true])
  test(`two browsers decorate and reconcile ${phone ? "phone" : "desktop"}`, async ({
    browser,
    baseURL,
    reef: { HOST, GUEST },
  }) => {
    const a = await browser.newContext({
      baseURL,
      viewport: phone ? { width: 390, height: 844 } : { width: 1280, height: 800 },
      hasTouch: phone,
      isMobile: phone,
    });
    const b = await browser.newContext({ baseURL });
    try {
      const host = await a.newPage(),
        guest = await b.newPage();
      await devLogin(host, HOST, [], "Reef host");
      await devLogin(guest, GUEST, [], "Reef guest");
      await host.goto("/me/house?play=1");
      await guest.goto(`/house/${HOST}?play=1`);
      await ready(host);
      await ready(guest);
      await host.getByRole("button", { name: "Decorate", exact: true }).click();
      // The wrapping label also contains option text; use the select's accessible name.
      await host
        .getByRole("combobox", { name: "Inventory", exact: true })
        .selectOption("pebblerest_stool__sand");
      if (phone) {
        const canvas = host.locator(".reef-canvas");
        const sheet = host.getByRole("complementary", { name: "Decorate", exact: true });
        await expect(host.getByRole("region", { name: "Room chat", exact: true })).toBeHidden();
        await expect(canvas).toHaveAttribute("data-art-ready", "true");
        await expect(canvas).toHaveAttribute("data-camera-settled", "true");
        const canvasBox = (await canvas.boundingBox())!;
        const sheetBox = (await sheet.boundingBox())!;
        expect(canvasBox.y + canvasBox.height).toBeLessThanOrEqual(sheetBox.y + 1);
        expect(await sheet.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
        // A phone camera follows the player: tile (1, 1) can be offscreen (x=419 at 390px).
        // Tap an unobscured tile centre, then verify both coordinates rather than assuming a camera.
        const maxX = Number(await host.getByLabel("Position X", { exact: true }).getAttribute("max"));
        const maxY = Number(
          await host.getByLabel("Position Y / wall level", { exact: true }).getAttribute("max"),
        );
        const point = await canvas.evaluate(
          (el, { maxX, maxY }) => {
            const c = el as HTMLCanvasElement;
            const rect = c.getBoundingClientRect();
            const [ox, oy] = c.dataset.origin!.split(",").map(Number);
            const [cx, cy] = c.dataset.camera!.split(",").map(Number);
            const scale = Number(c.dataset.scale),
              dpr = devicePixelRatio;
            for (let tileX = 1; tileX <= maxX; tileX++) {
              for (let tileY = 1; tileY <= maxY; tileY++) {
                const x = (cx! + (ox! + 32 * (tileX - tileY)) * scale) / dpr;
                const y = (cy! + (oy! + 16 * (tileX + tileY + 1)) * scale) / dpr;
                if (
                  x >= 16 &&
                  x < rect.width - 16 &&
                  y >= 16 &&
                  y < rect.height - 16 &&
                  document.elementFromPoint(rect.left + x, rect.top + y) === c
                )
                  return { x, y, tileX, tileY };
              }
            }
            return null;
          },
          { maxX, maxY },
        );
        expect(point, "The phone editor must leave a floor tile available to tap").not.toBeNull();
        await canvas.tap({ position: { x: point!.x, y: point!.y } });
        await expect(host.getByLabel("Position X", { exact: true })).toHaveValue(String(point!.tileX));
        await expect(host.getByLabel("Position Y / wall level", { exact: true })).toHaveValue(
          String(point!.tileY),
        );
      } else {
        await host.getByLabel("Position X", { exact: true }).fill("2");
        await host.getByLabel("Position Y / wall level", { exact: true }).fill("2");
        await host.locator(".reef-canvas").focus();
        await host.keyboard.press("ArrowRight");
        await expect(host.getByLabel("Position X", { exact: true })).toHaveValue("3");
      }
      await host.getByRole("button", { name: "Confirm placement", exact: true }).click();
      await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-placements", "1");
      await expect(host.getByRole("button", { name: "Undo", exact: true })).toBeEnabled();
      await host.getByRole("button", { name: "Undo", exact: true }).click();
      await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-placements", "0");
      await expect(host.getByRole("heading", { level: 1 })).toHaveCount(1);
      expect(await overflow(host)).toBeLessThanOrEqual(1);
      await shot(host, `reef7-decorate-${phone ? "phone" : "desktop"}`);
      await host.getByRole("button", { name: "Done", exact: true }).click();
      await expect(host.locator(".reef-canvas")).toBeFocused();
      await expect(host.getByRole("region", { name: "Room chat", exact: true })).toBeVisible();
    } finally {
      await a.close();
      await b.close();
    }
  });
test("two browsers ring, accept, visit, appreciate and sign in-world", async ({
  browser,
  baseURL,
  reef: { HOST, GUEST },
}) => {
  const a = await browser.newContext({ baseURL }),
    b = await browser.newContext({ baseURL });
  try {
    const host = await a.newPage(),
      guest = await b.newPage();
    await devLogin(host, HOST, [], "Reef host");
    await devLogin(guest, GUEST, [], "Reef guest");
    expect(
      (
        await write(host, "/api/reef/preferences", "PUT", {
          presence: true,
          visits: "doorbell",
          doorbell: true,
        })
      ).ok(),
    ).toBeTruthy();
    await host.goto("/me/house?play=1");
    await guest.goto("/me/house?play=1");
    await ready(host);
    await ready(guest);
    for (const p of [host, guest])
      await p.getByRole("button", { name: "Social spaces", exact: true }).click();
    await guest.getByRole("button", { name: "Ring doorbell", exact: true }).click();
    await host.getByRole("button", { name: "Accept", exact: true }).click();
    await guest.getByRole("link", { name: "Visit", exact: true }).first().click();
    await ready(guest);
    await expect(host.locator(".reef-canvas")).toHaveAttribute("data-players", "2");
    await guest.getByRole("button", { name: "Social spaces", exact: true }).click();
    await guest.getByRole("button", { name: "Appreciate", exact: true }).click();
    await guest.getByLabel("Leave a note", { exact: true }).fill("A lovely reef room");
    await guest.getByRole("button", { name: "Sign guestbook", exact: true }).click();
    await expect(guest.getByText("A lovely reef room", { exact: true })).toBeVisible();
    expect((await write(host, `/api/reef/blocks/${GUEST}`, "PUT", {})).ok()).toBeTruthy();
    await expect(guest.locator(".reef-canvas")).not.toHaveAttribute("data-network", "connected");
  } finally {
    await a.close();
    await b.close();
  }
});
test("two guild members enter the same plaza and RSVP to a scheduled gathering", async ({
  browser,
  baseURL,
  reef: { HOST, GUEST, GUILD },
}) => {
  const a = await browser.newContext({ baseURL }),
    b = await browser.newContext({ baseURL });
  try {
    const host = await a.newPage(),
      guest = await b.newPage();
    const guild = { id: GUILD, name: "Wave 7 Reef", owner: true, permissions: "32" };
    await devLogin(host, HOST, [guild], "Reef host");
    await devLogin(guest, GUEST, [{ ...guild, owner: false, permissions: "0" }], "Reef guest");
    const url = `/me/house?play=1&scope=plaza&guild=${GUILD}&instance=0`;
    await host.goto(url);
    await guest.goto(url);
    await ready(host);
    await ready(guest);
    for (const p of [host, guest])
      await expect(p.locator(".reef-canvas")).toHaveAttribute("data-players", "2");
    await expect(guest.getByRole("button", { name: "Decorate", exact: true })).toHaveCount(0);
    const starts = new Date(Date.now() + 3600000),
      ends = new Date(Date.now() + 7200000);
    const response = await write(host, `/api/reef/guilds/${GUILD}/gatherings`, "POST", {
      title: "Reef room tour",
      starts_at: starts.toISOString(),
      ends_at: ends.toISOString(),
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    await guest.getByRole("button", { name: "Social spaces", exact: true }).click();
    const row = guest.getByRole("listitem").filter({ hasText: "Reef room tour" }).last();
    await row.getByRole("button", { name: "RSVP", exact: true }).click();
    await expect(guest.getByText("RSVP confirmed", { exact: true })).toBeVisible();
    await row.getByRole("link", { name: "Join gathering", exact: true }).click();
    await ready(guest);
    await shot(guest, "reef7-plaza-gathering");
  } finally {
    await a.close();
    await b.close();
  }
});

test("social sheet wraps long names at 390 px and supports keyboard open/close", async ({
  page,
  reef: { HOST, GUEST },
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await devLogin(page, HOST, [], "Reef host");
  await page.route("**/api/reef/social", (route) =>
    route.fulfill({
      json: {
        people: [
          { id: GUEST, name: "W".repeat(32), partner: true, online: true, visitable: true, doorbell: true },
        ],
      },
    }),
  );
  await page.route("**/api/reef/doorbells", (route) =>
    route.fulfill({ json: { items: [{ id: "1", host_id: HOST, guest_id: GUEST, status: "pending" }] } }),
  );
  await page.goto("/me/house?play=1");
  await ready(page);
  const toggle = page.getByRole("button", { name: "Social spaces", exact: true });
  await toggle.focus();
  await page.keyboard.press("Enter");
  const sheet = page.getByRole("complementary", { name: "Social spaces", exact: true });
  await expect(sheet.getByText("W".repeat(32), { exact: false })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
  expect(await sheet.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await sheet.getByRole("button", { name: "Close", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(sheet).toHaveCount(0);
  await expect(page.locator(".reef-canvas")).toBeFocused();
});
