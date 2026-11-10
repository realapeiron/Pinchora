// P3's module switches in a real browser: switch parts off, limit one to a channel, see the grid on the
// overview, the names in the audit log, and the timed storm settings.

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}7`;
const user = `${run}8`;

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

const row = (page: Page, module: string) => page.locator(`[data-module="${module}"]`);

test("switch a part off, keep another to one channel, and see it on the overview and in the log", async ({
  page,
}) => {
  await login(page);
  await page.goto(`/g/${guild}/modules`);
  await expect(page.getByRole("heading", { name: "MODULES", exact: true })).toBeVisible();
  await expect(page.getByRole("switch", { name: "Pets", exact: true })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  // Gambling starts off; whole-server parts have no channel list.
  await expect(page.getByRole("switch", { name: "Gambling-style games" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
  await expect(row(page, "welcome")).toContainText("Whole server");
  await expect(row(page, "welcome").getByRole("button", { name: "Channels" })).toHaveCount(0);
  await shot(page, "modules-before", true);

  await page.getByRole("switch", { name: "Pets", exact: true }).click();
  // The switch says on or off; a badge appears only for what it cannot say (kept to some channels).
  await expect(page.getByRole("switch", { name: "Pets", exact: true })).toContainText("OFF");
  await expect(row(page, "pets").locator(".badge")).toHaveCount(0);

  // A list needs a channel before it can be saved.
  const fishing = row(page, "fishing");
  await fishing.getByRole("button", { name: "Channels" }).click();
  await fishing.getByRole("radio", { name: /Only in these/ }).check({ force: true });
  await expect(fishing.getByRole("alert")).toContainText("Pick at least one channel");
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await fishing.getByLabel("Add a channel").selectOption({ label: "#general" });
  await fishing.getByRole("button", { name: "Add", exact: true }).click();
  await expect(fishing.locator(".channel-chip")).toContainText("#general");
  await expect(fishing.locator(".badge")).toHaveText("ONLY IN 1 CHANNEL");
  await expect(page.getByText("2 unsaved changes")).toBeVisible();
  await shot(page, "modules-edited", true);

  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/modules`)).json();
  const by = (key: string) => saved.items.find((m: { key: string }) => m.key === key);
  expect(by("pets").enabled).toBe(false);
  expect(by("fishing").channel_mode).toBe("only");
  expect(by("fishing").channels).toEqual(["7001"]);

  // The overview catalog reflects module state; channel scope stays on Modules.
  await page.goto(`/g/${guild}`);
  const catalog = page.getByRole("region", { name: "Module catalog" });
  await catalog.getByRole("searchbox", { name: "Search modules" }).fill("Pets");
  await expect(catalog.getByRole("switch", { name: "Pets module" })).toHaveAttribute("aria-checked", "false");
  await catalog.getByRole("searchbox", { name: "Search modules" }).fill("Fishing");
  await expect(catalog.getByRole("switch", { name: "Fishing module" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(catalog.locator(".overview-module", { hasText: "Fishing" })).toContainText(
    "ONLY IN 1 CHANNEL",
  );
  // A regular expression is case-sensitive: the name is the module's label, not its key.
  await expect(page.getByText(/turned Pets off/)).toBeVisible();
  await shot(page, "overview-modules", true);

  // The audit log says Pets and #general, not keys and ids.
  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText(/limited Fishing to 1 channel/)).toBeVisible();
  await page.getByText(/limited Fishing to 1 channel/).click();
  await expect(page.locator(".audit-diff")).toContainText("#general");
  await expect(page.locator(".audit-diff")).not.toContainText("7001");
  await shot(page, "audit-modules");

  // Back to all channels: the badge and the row go back to normal.
  await page.goto(`/g/${guild}/modules`);
  await row(page, "fishing").getByRole("button", { name: "Channels" }).click();
  await row(page, "fishing")
    .getByRole("radio", { name: /All channels/ })
    .check({ force: true });
  await page.getByRole("switch", { name: "Pets", exact: true }).click();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const reset = await (await page.request.get(`/api/guilds/${guild}/modules`)).json();
  expect(
    reset.items.every(
      (m: { key: string; enabled: boolean }) => m.enabled === !["gambling", "welcome"].includes(m.key),
    ),
  ).toBe(true);
});

test("the modules page fits a phone", async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/modules`);
  await expect(page.getByRole("heading", { name: "MODULES", exact: true })).toBeVisible();
  await row(page, "house").getByRole("button", { name: "Channels" }).click();
  await row(page, "house")
    .getByRole("radio", { name: /Everywhere but these/ })
    .check({ force: true });
  await row(page, "house").getByLabel("Add a channel").selectOption({ label: "▸ Chat (whole category)" });
  await row(page, "house").getByRole("button", { name: "Add", exact: true }).click();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "modules-phone-sheet", true);
  await page.getByRole("button", { name: "Reset" }).click();
});

test("timed storm surges are set on the community page", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/community`);
  await expect(page.getByRole("heading", { name: "STORM SURGES" })).toBeVisible();
  await page.getByRole("switch", { name: "Timed storms" }).click();
  await page.getByLabel("Storm channel").selectOption({ label: "#announcements" });
  await page.getByLabel("How often").selectOption({ label: "Every 12 hours" });
  await expect(page.getByText("3 unsaved changes")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const now = await (await page.request.get(`/api/guilds/${guild}/community`)).json();
  expect(now.surge).toMatchObject({ enabled: true, channel_id: "7011", every_hours: 12 });
  expect(now.surge.next_at).toBeTruthy();
  await page.reload();
  await expect(page.getByText(/The next one comes/)).toBeVisible();
  await shot(page, "community-surge", true);
});
