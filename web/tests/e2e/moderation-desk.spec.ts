import { test, expect } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";
test("moderation desk has accessible policy fields and a responsive staff queue", async ({ page }) => {
  const guild = `${Date.now()}76`;
  await devLogin(page, `${Date.now()}77`, [
    { id: guild, name: "Moderation desk", owner: true, permissions: "0" },
  ]);
  await page.goto(`/g/${guild}/moderation-desk`);
  await expect(page.getByRole("heading", { name: "Moderation desk", exact: true })).toBeVisible();
  await expect(page.getByLabel("Report channel")).toBeVisible();
  await page.getByRole("button", { name: "Add cleanup rule" }).click();
  await expect(page.getByLabel("Age in hours")).toHaveValue("24");
  await shot(page, "mod-parity-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "mod-parity-phone", true);
});
