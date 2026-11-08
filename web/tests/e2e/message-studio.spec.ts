import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";

const run = Date.now();
const guild = `${run}4`;
const user = `${run}5`;

test("message kit preview, revisions and event binding", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/message-studio`);
  await expect(page.getByRole("heading", { name: "MESSAGE STUDIO", exact: true })).toBeVisible();
  await page.getByLabel("Kit name").fill("Greeting kit");
  await page.getByLabel("Content", { exact: true }).fill("Hello {user} in {server}");
  await expect(page.getByRole("region", { name: "Message preview" })).toContainText("Pinchora Reef");
  await page.getByRole("button", { name: "Save kit" }).click();
  await expect(page.getByText("Kit saved.")).toBeVisible();
  await page.getByLabel("Welcome", { exact: true }).selectOption("greeting-kit");
  await expect(page.getByText("Event message updated.")).toBeVisible();
  await page.getByLabel("Content", { exact: true }).fill("Hello again {user}");
  await page.getByRole("button", { name: "Save kit" }).click();
  await expect(page.locator(".studio-revision")).toHaveCount(2);
  await page.locator(".studio-revision").last().getByRole("button", { name: "Restore" }).click();
  await expect(page.getByLabel("Content", { exact: true })).toHaveValue("Hello {user} in {server}");
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
});
