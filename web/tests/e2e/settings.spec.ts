// P2's settings pages in a real browser: every page on desktop and phone, the welcome card designer
// with the bot's real preview PNG, and the leveling lists (level roles, multipliers, the sync job).

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}4`;
const user = `${run}6`;

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

/**
 * The pixels of the welcome card the designer shows right now, as a data URL. (Read through a canvas:
 * the dashboard's CSP lets images, not fetches, load blob: URLs.)
 */
async function previewPixels(page: Page): Promise<string> {
  const frame = page.locator(".preview-frame");
  await expect(frame).not.toHaveClass(/is-busy/, { timeout: 15_000 });
  const image = page.locator(".preview-image");
  await expect(image).toBeVisible();
  return image.evaluate(async (img: HTMLImageElement) => {
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    canvas.getContext("2d")!.drawImage(img, 0, 0);
    return canvas.toDataURL("image/png");
  });
}

const PAGES = [
  { path: "", name: "overview", heading: "OVERVIEW" },
  { path: "/modules", name: "modules", heading: "MODULES" },
  { path: "/welcome", name: "welcome", heading: "WELCOME" },
  { path: "/welcome-image", name: "welcome-image", heading: "WELCOME IMAGE" },
  { path: "/leveling", name: "leveling", heading: "LEVELING" },
  { path: "/points", name: "points", heading: "POINTS" },
  { path: "/log", name: "server-log", heading: "SERVER LOG" },
  { path: "/community", name: "community", heading: "COMMUNITY" },
  { path: "/audit", name: "audit-log", heading: "AUDIT LOG" },
];

test("the welcome page shows the image the bot sends and opens the editor", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/welcome`);
  await expect(page.getByRole("heading", { name: "WELCOME", exact: true })).toBeVisible();
  const first = await previewPixels(page);
  expect(first.length).toBeGreaterThan(1000);
  expect(await page.locator(".preview-image").evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(
    1100,
  );
  await expect(page.getByText("The classic card, sent with the text, in one message.")).toBeVisible();
  await page.locator(".designer").scrollIntoViewIfNeeded();
  await shot(page, "welcome-designer");

  await page.getByLabel("Channel").first().selectOption({ label: "#welcome" });
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const api = await (await page.request.get(`/api/guilds/${guild}`)).json();
  expect(api.settings.welcome_channel_id).toBe("7010");

  await page.getByRole("link", { name: "Open the editor" }).click();
  await expect(page.getByRole("heading", { name: "WELCOME IMAGE", exact: true })).toBeVisible();
});

test("level roles refuse what the bot cannot give, and the sync runs", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/leveling`);
  const roles = page.getByLabel("Role", { exact: true }).first();
  await expect(roles.locator("option", { hasText: "@Admin" })).toBeDisabled();
  await expect(roles.locator("option", { hasText: "@Admin" })).toContainText("above my role");
  await expect(roles.locator("option", { hasText: "@Server Booster" })).toBeDisabled();

  await page.getByLabel("Level", { exact: true }).fill("3");
  await roles.selectOption({ label: "@Fan" });
  await page.getByRole("button", { name: "Add" }).first().click();
  const list = page.getByRole("list", { name: "Level roles" });
  await expect(list).toContainText("@Fan");
  await expect(list).toContainText("LV 3");

  await page.getByRole("button", { name: "Sync members" }).click();
  await expect(page.getByText("Checked 0 members and updated the level roles of 0.")).toBeVisible();

  // An XP multiplier for a channel, and a no-XP role.
  await page.getByLabel("Channel", { exact: true }).selectOption({ label: "#memes" });
  await page.getByLabel("Multiplier").fill("2");
  await page.getByRole("button", { name: "Set", exact: true }).click();
  const rules = page.getByRole("list", { name: "XP multipliers" });
  await expect(rules).toContainText("#memes");
  await expect(rules).toContainText("×2");

  await page.getByText("Direct message", { exact: true }).click();
  await expect(page.getByRole("radio", { name: /Direct message/ })).toBeChecked();
  await page.getByRole("switch", { name: "Voice XP" }).click();
  await shot(page, "leveling-edited", true);
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();

  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText("synced the level roles")).toBeVisible();
  await expect(page.getByText("set a level role")).toBeVisible();
  await page.getByText("set a level role").click();
  await expect(page.locator(".diff")).toContainText("level");
  await shot(page, "audit-log-open");
});

test("points and the store", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/points`);
  await page.getByLabel("Emoji").fill("coins");
  await expect(page.getByText("Use one emoji")).toBeVisible();
  await page.getByLabel("Emoji").fill("🐚");
  await page.getByLabel("Name").fill("shells");
  await page.getByLabel("Most per message").fill("6");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();

  await page.getByLabel("Role", { exact: true }).selectOption({ label: "@Regular" });
  await page.getByLabel(/^Price/).fill("250");
  await page.getByLabel("Stock").fill("5");
  await page.getByRole("button", { name: "Put on sale" }).click();
  const store = page.getByRole("list", { name: "Store" });
  await expect(store).toContainText("@Regular");
  await expect(store).toContainText("250");
  await expect(store).toContainText("5 left");
});

/** Waits until the pictures and message previews on the page have loaded. */
async function settled(page: Page, name: string) {
  if (name === "welcome" || name === "welcome-image") await previewPixels(page);
  await expect(page.locator(".state")).toHaveCount(0);
  for (const preview of await page.locator(".message-preview p").all()) {
    await expect(preview).not.toHaveText("…");
  }
}

for (const { path, name, heading } of PAGES) {
  test(`${name} looks right on desktop and phone`, async ({ page }) => {
    await login(page);
    await page.goto(`/g/${guild}${path}`);
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
    await settled(page, name);
    await shot(page, `${name}-desktop`, true);

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
    await settled(page, name);
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await shot(page, `${name}-phone`, true);
  });
}

test("a server id that is not a number is not a page", async ({ page }) => {
  await login(page);
  await page.goto("/g/not-a-server/leveling");
  await expect(page.getByText("LOST AT SEA")).toBeVisible();
});
