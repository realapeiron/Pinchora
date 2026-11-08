// Real API, Redis actors and two authenticated browsers. The lead runs Chromium.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test, expect, type Page } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";
const HOST = "770000000000000181",
  GUEST = "770000000000000182",
  GUILD = "770000000000000180";
test.setTimeout(90_000);
test.beforeEach(() =>
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "reef", "seed_reef_wave7", "--", "--ignored"],
    {
      cwd: fileURLToPath(new URL("../../..", import.meta.url)),
      stdio: "inherit",
      env: { ...process.env, CARGO_BUILD_JOBS: "4" },
    },
  ),
);
async function write(page: Page, path: string, method: string, data: unknown) {
  const me = await (await page.request.get("/api/me")).json();
  return page.request.fetch(path, { method, headers: { "X-CSRF-Token": me.csrf }, data });
}
async function ready(page: Page) {
  await expect(page.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
}
for (const phone of [false, true])
  test(`two browsers decorate and reconcile ${phone ? "phone" : "desktop"}`, async ({ browser, baseURL }) => {
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
      await host.getByLabel("Inventory", { exact: true }).selectOption("pebblerest_stool__sand");
      if (phone) {
        const point = await host.locator(".reef-canvas").evaluate((el) => {
          const c = el as HTMLCanvasElement;
          const [ox, oy] = c.dataset.origin!.split(",").map(Number);
          const [cx, cy] = c.dataset.camera!.split(",").map(Number);
          const scale = Number(c.dataset.scale),
            dpr = devicePixelRatio;
          return { x: (cx! + ox! * scale) / dpr, y: (cy! + (oy! + 48) * scale) / dpr };
        });
        await host.locator(".reef-canvas").tap({ position: point });
        await expect(host.getByLabel("Position X", { exact: true })).toHaveValue("1");
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
      expect(await overflow(host)).toBeLessThanOrEqual(1);
      await shot(host, `reef7-decorate-${phone ? "phone" : "desktop"}`);
    } finally {
      await a.close();
      await b.close();
    }
  });
test("two browsers ring, accept, visit, appreciate and sign in-world", async ({ browser, baseURL }) => {
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
