import { expect, test } from "@playwright/test";
import { devLogin, openRow, overflow } from "./helpers";
const run = Date.now();
const guild = `${run}1`;
const user = `${run}2`;
test("AutoMod profiles, rule editing, safe preview, and responsive layouts", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/automod`);
  await expect(page.getByRole("heading", { name: "AUTOMOD", exact: true })).toBeVisible();
  await page.getByRole("button", { name: /Standard Balanced/ }).click();
  await expect(page.getByText(/Observation ends/)).toBeVisible();
  const rules = page.locator(".automod-page > .settings-detail-list > .settings-detail-row").nth(1);
  const rulesHead = rules.locator(":scope > .settings-detail-head > .settings-detail-expand");
  await openRow(rulesHead);
  await expect(rulesHead).toHaveAttribute("aria-expanded", "true");
  await expect(rules.locator(".automod-rules > .settings-detail-row")).toHaveCount(16);
  const caps = rules
    .locator(".automod-rules > .settings-detail-row")
    .filter({ hasText: "Excessive capitals" });
  await openRow(caps.locator(".settings-detail-expand"));
  await expect(caps.locator(".settings-detail-expand")).toHaveAttribute("aria-expanded", "true");
  await caps.getByText("Configure rule").click();
  await caps.getByLabel("Threshold", { exact: true }).fill("90");
  await caps.getByRole("switch", { name: "Log only", exact: true }).click();
  // The button says "Saving…" while PUT is pending, so its old name disappearing is not a save.
  const savedResponse = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/api/guilds/${guild}/automod`) && response.request().method() === "PUT",
  );
  await page.getByRole("button", { name: "Save changes" }).click();
  expect((await savedResponse).ok()).toBeTruthy();
  await expect(page.getByRole("region", { name: "Unsaved changes" })).toHaveCount(0);
  const response = await page.request.get(`/api/guilds/${guild}/automod`);
  expect(response.ok()).toBeTruthy();
  const data = await response.json();
  const saved = data.config.rules.find((r: { kind: string }) => r.kind === "caps");
  expect(saved.params.threshold).toBe(90);
  expect(saved.log_only).toBe(true);
  const testRow = page
    .locator(".automod-page .settings-detail-row")
    .filter({ has: page.locator("#am-test") });
  await openRow(testRow.locator(".settings-detail-expand"));
  await expect(testRow.locator(".settings-detail-expand")).toHaveAttribute("aria-expanded", "true");
  await page.getByLabel("Message text", { exact: true }).fill("THIS MESSAGE IS TOO LOUD");
  await page.getByRole("button", { name: "Run test", exact: true }).click();
  await expect(page.locator(".automod-test-result")).toContainText("Preview only");
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: "../.reports/automod/desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.screenshot({ path: "../.reports/automod/phone.png", fullPage: true });
  await page.getByRole("button", { name: /Off Pause/ }).click();
  await expect(page.locator('.automod-profile[aria-pressed="true"]')).toContainText("Off");
});
