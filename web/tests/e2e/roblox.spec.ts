// Roblox settings and role binds at desktop and phone widths.
import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const guild = `${Date.now()}6`;
const user = `${Date.now()}7`;

test("Roblox configuration saves and fits phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/roblox`);
  // The setting "Enable Roblox verification" is a separate, legitimate subsection heading.
  await expect(page.getByRole("heading", { name: "Roblox verification", exact: true })).toBeVisible();
  await page.getByRole("switch", { name: "Enable Roblox verification" }).click();
  await page.getByRole("button", { name: /Nickname template/ }).click();
  await page.getByLabel("Nickname template").fill("{discord-name} ({roblox-name})");
  await expect(page.getByText("DiscordMember (RobloxPlayer)")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Roblox settings saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/roblox`)).json();
  expect(saved.settings).toMatchObject({
    enabled: true,
    nickname_template: "{discord-name} ({roblox-name})",
  });
  await shot(page, "roblox-desktop", true);
  await page.getByRole("button", { name: /Role binds/ }).click();
  await page.getByLabel("Condition").selectOption("verified");
  await page.getByLabel("Role to grant").selectOption({ label: "@Fan" });
  await page.getByRole("button", { name: "Add bind" }).click();
  await expect(page.getByText("verified ·")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "roblox-phone", true);
});
