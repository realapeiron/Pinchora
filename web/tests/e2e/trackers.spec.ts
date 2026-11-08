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
  await expect(page.getByRole("heading", { name: "Game trackers" })).toBeVisible();
  await page.getByLabel("League of Legends").selectOption("7001");
  await page.getByLabel("Server address").fill("8.8.8.8:25565");
  await page.getByLabel("Status channel").selectOption("7001");
  await page.getByRole("button", { name: "Add server" }).click();
  await expect(page.getByText("8.8.8.8:25565")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status")).toContainText("Tracker settings saved");
  const data = await (await page.request.get(`/api/guilds/${guild}/trackers`)).json();
  expect(data.patch_channels.lol).toBe("7001");
  expect(data.mc_watch).toHaveLength(1);
  await shot(page, "trackers-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "trackers-phone", true);
});
