import { type Page, expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}8`;
const user = `${run}9`;
async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

test("tracker channels and Minecraft watch list work on desktop and phone", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/trackers`);
  await expect(page.getByRole("heading", { level: 1, name: "Game trackers", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Game trackers", exact: true })).toHaveCount(1);
  const save = page.getByRole("button", { name: "Save changes", exact: true });
  await expect(save).toBeDisabled();
  await page.getByRole("button", { name: /^League of Legends/ }).click();
  await page.getByRole("button", { name: /^Valorant/ }).click();
  await page.getByRole("button", { name: /^MINECRAFT WATCH LIST/ }).click();
  await page.getByRole("button", { name: /^MINECRAFT NEWS/ }).click();
  await page.locator("#patch-lol").selectOption("7001");
  await expect(save).toBeEnabled();
  await page.locator("#patch-lol").selectOption("");
  await expect(save).toBeDisabled();
  await page.locator("#patch-lol").selectOption("7001");
  await page.locator("#patch-valorant").selectOption("7001");
  await page.getByLabel("Server address").fill("8.8.8.8:25565");
  await page.getByLabel("Status channel").selectOption("7001");
  await page.getByRole("button", { name: "Add server" }).click();
  await expect(page.getByText("8.8.8.8:25565")).toBeVisible();
  await save.click();
  await expect(page.getByRole("status").filter({ hasText: "Tracker settings saved" })).toBeVisible();
  await expect(save).toBeDisabled();
  // Removing and restoring a channel changes object insertion order, not settings.
  await page.locator("#patch-lol").selectOption("");
  await page.locator("#patch-lol").selectOption("7001");
  await expect(save).toBeDisabled();
  await page.locator("#mc-news").selectOption("7001");
  const request = page.waitForRequest(
    (request) => request.method() === "PUT" && request.url().endsWith(`/api/guilds/${guild}/trackers`),
  );
  const response = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" && response.url().endsWith(`/api/guilds/${guild}/trackers`),
  );
  await save.click();
  expect((await response).ok()).toBeTruthy();
  expect((await request).postDataJSON()).toEqual({
    patch_channels: { lol: "7001", valorant: "7001" },
    mc_watch: [{ host: "8.8.8.8:25565", channel_id: "7001" }],
    mc_news_channel_id: "7001",
  });
  await expect(save).toBeDisabled();
  const data = await (await page.request.get(`/api/guilds/${guild}/trackers`)).json();
  expect(data.patch_channels.lol).toBe("7001");
  expect(data.mc_watch).toHaveLength(1);
  await shot(page, "trackers-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  const longHost = `${"a".repeat(63)}.${"b".repeat(63)}.example.com:25565`;
  await page.getByLabel("Server address").fill(longHost);
  await page.getByLabel("Status channel").selectOption("7001");
  await page.getByRole("button", { name: "Add server" }).click();
  await expect(page.getByText(longHost, { exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "trackers-phone", true);
});
