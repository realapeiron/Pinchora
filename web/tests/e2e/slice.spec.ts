// P1's slice in a real browser: log in, pick a server, turn leveling off, see it saved and recorded.
// Screenshots of each page land in test-results/screens/ for a human (or agent) to look at.

import { type Page, expect, test } from "@playwright/test";

import { devLogin as login, shot } from "./helpers";

const run = Date.now();
const reef = `${run}1`;
const cafe = `${run}2`;
const chatting = `${run}3`;
const kelp = `${run}9`; // ends in 9: the mock says the bot is not there
const user = `${run}5`;

async function devLogin(page: Page) {
  await login(page, user, [
    { id: reef, name: "Pinchora Reef", owner: true, permissions: "0" },
    { id: cafe, name: "Coral Cafe", owner: false, permissions: "32" },
    { id: kelp, name: "Kelp Corner", owner: false, permissions: "8" },
    { id: chatting, name: "Just Chatting", owner: false, permissions: "2048" },
  ]);
}

test("the login page offers Discord login", async ({ page }) => {
  await page.goto("/servers");
  await expect(page).toHaveURL(/\/login\?next=%2Fservers/);
  const login = page.getByRole("link", { name: "Log in with Discord" });
  await expect(login).toHaveAttribute("href", "/auth/login?next=%2Fservers");
  await page.evaluate(() => document.fonts.ready);
  await shot(page, "login");
});

test("login, pick a server, turn leveling off", async ({ page }) => {
  await devLogin(page);
  await page.goto("/servers");
  await expect(page).toHaveURL(/\/servers$/);

  const cards = page.locator(".server-card");
  await expect(cards).toHaveCount(3);
  await expect(page.getByText("Just Chatting")).toHaveCount(0);
  const invite = page.getByRole("link", { name: "Add Pinchora to Kelp Corner" });
  await expect(invite).toHaveAttribute("href", new RegExp(`guild_id=${kelp}`));
  await page.evaluate(() => document.fonts.ready);
  await shot(page, "servers");

  await page.getByRole("link", { name: "Open Pinchora Reef" }).click();
  await expect(page).toHaveURL(new RegExp(`/g/${reef}$`));
  const toggle = page.getByRole("switch", { name: "Chat XP" });
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText("No changes from the dashboard yet.")).toBeVisible();
  await shot(page, "overview-before");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await expect(page.getByText("1 unsaved change")).toBeVisible();
  await shot(page, "overview-unsaved");

  // Leaving with unsaved changes asks first.
  await page.getByRole("link", { name: "Switch server" }).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await shot(page, "overview-leave-dialog");
  await page.getByRole("button", { name: "Stay" }).click();
  await expect(page).toHaveURL(new RegExp(`/g/${reef}$`));

  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  await expect(page.getByText("turned leveling off")).toBeVisible();
  await expect(page.locator(".save-bar")).not.toHaveClass(/is-open/);
  await shot(page, "overview-saved");

  const api = await page.request.get(`/api/guilds/${reef}`);
  const overview = await api.json();
  expect(overview.settings.leveling_enabled).toBe(false);
  expect(overview.audit[0].after).toEqual({ leveling_enabled: false });
});

test("a server without the bot offers the invite", async ({ page }) => {
  await devLogin(page);
  await page.goto(`/g/${kelp}`);
  await expect(page.getByText("Pinchora is not here yet")).toBeVisible();
  await expect(page.getByRole("link", { name: "Add Pinchora" })).toHaveAttribute(
    "href",
    new RegExp(`guild_id=${kelp}`),
  );
});

test("the overview fits a phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await devLogin(page);
  await page.goto(`/g/${cafe}`);
  const toggle = page.getByRole("switch", { name: "Chat XP" });
  await toggle.click();
  await expect(page.getByText("1 unsaved change")).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await page.evaluate(() => document.fonts.ready);
  await shot(page, "overview-phone");
});
