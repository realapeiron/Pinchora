import { expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const guild = `${Date.now()}8`;
const user = `${Date.now()}9`;

test("achievement editor, icon picker, preview and holder list fit desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/achievements`);
  await expect(page.getByRole("heading", { name: "ACHIEVEMENTS", exact: true })).toBeVisible();
  await expect(page.getByText("0/25")).toBeVisible();
  await page.getByRole("button", { name: "Add achievement" }).click();
  await page.getByLabel("Name", { exact: true }).fill("First Pearl");
  await page.getByLabel("Description", { exact: true }).fill("Send ten messages in the reef");
  await page.getByLabel("Target", { exact: true }).fill("10");
  await page.getByRole("button", { name: "shell", exact: true }).click();
  await page.getByRole("button", { name: "Test preview" }).click();
  await expect(page.locator(".ach-preview-earned")).toContainText("First Pearl");
  await shot(page, "achievements-editor-desktop", true);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator(".ach-list-card")).toHaveCount(1);
  const response = await page.request.get(`/api/guilds/${guild}/achievements`);
  expect(response.ok()).toBeTruthy();
  const data = await response.json();
  expect(data.items[0].icon).toBe("shell");
  await page.getByRole("button", { name: "Who earned it" }).click();
  await expect(page.getByText("No one yet.")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "achievements-phone", true);
});
