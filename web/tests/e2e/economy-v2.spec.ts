import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";
const run = Date.now();
test("wallet job tasks and review pages fit desktop and phone", async ({ page }) => {
  await devLogin(page, `${run}71`, []);
  await page.goto("/me/wallet");
  await expect(page.locator("h1")).toHaveCount(1);
  await page.getByRole("button", { name: "Start shift" }).first().click();
  for (const task of ["Sort the fronds", "Tie the bundles", "Stock the reef pantry"])
    await page.getByRole("button", { name: task, exact: true }).click();
  await expect(page.getByText("1/3 shifts today", { exact: false })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.screenshot({ path: "test-results/economy-wallet-desktop.png", fullPage: true });
  for (const tab of ["Auctions", "Trades", "Season & prestige"]) {
    await page.getByRole("button", { name: tab, exact: true }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await overflow(page)).toBeLessThanOrEqual(0);
  }
  await page.screenshot({ path: "test-results/economy-wallet-phone.png", fullPage: true });
});
test("economy settings save only changed writable fields", async ({ page }) => {
  const guild = `${run}72`;
  await devLogin(page, `${run}73`, [{ id: guild, name: "Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/economy`);
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await page.getByLabel("Donation goal", { exact: true }).fill("15000");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await page.screenshot({ path: "test-results/economy-settings-desktop.png", fullPage: true });
  await page.getByRole("button", { name: "Preview affected balances" }).click();
  await expect(page.getByRole("button", { name: "Apply point reduction" })).toBeDisabled();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await expect(page.locator("h1")).toHaveCount(1);
  await page.screenshot({ path: "test-results/economy-settings-phone.png", fullPage: true });
});
