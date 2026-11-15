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

test("moderation desk save bar opens on a change, resets and saves", async ({ page }) => {
  const guild = `${Date.now()}78`;
  await devLogin(page, `${Date.now()}79`, [
    { id: guild, name: "Moderation desk", owner: true, permissions: "0" },
  ]);
  await page.goto(`/g/${guild}/moderation-desk`);
  const save = page.getByRole("button", { name: "Save changes" });
  const appeals = page.getByLabel("Allow ban and warn appeals");
  await expect(page.getByRole("button", { name: "Validate policy" })).toBeVisible();
  await expect(save).toBeHidden();
  await appeals.click();
  await expect(save).toBeVisible();
  await expect(page.getByText("1 unsaved change")).toBeVisible();
  await page.getByRole("button", { name: "Reset" }).click();
  await expect(save).toBeHidden();
  await expect(appeals).not.toBeChecked();
  await appeals.click();
  const policyPath = `/api/guilds/${guild}/mod-parity`;
  const refused = page.waitForRequest((r) => r.method() === "PUT" && r.url().endsWith(policyPath));
  await save.click();
  expect((await refused).postDataJSON().appeals).toBe(true);
  await expect(page.getByRole("region", { name: "Unsaved changes" })).toContainText(
    "Check the action and required fields.",
  );
  await page.route(`**${policyPath}`, (route) =>
    route.request().method() === "PUT"
      ? route.fulfill({
          json: { config: route.request().postDataJSON(), queues: { reports: [], appeals: [] } },
        })
      : route.fallback(),
  );
  await save.click();
  await expect(page.getByText("Policy saved.")).toBeVisible();
  await expect(save).toBeHidden();
  const validate = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith(`${policyPath}/preview`),
  );
  await page.getByRole("button", { name: "Validate policy" }).click();
  await validate;
});
