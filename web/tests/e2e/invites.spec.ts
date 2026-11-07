// Invite settings, reward roles and member accounting at desktop and phone widths.
import { expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const guild = `${Date.now()}6`;
const user = `${Date.now()}7`;

test("invites settings and leaderboard fit desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/invites`);
  await expect(page.getByRole("heading", { name: "INVITES", exact: true })).toBeVisible();
  await expect(page.getByText("Manage Server permission missing.")).toBeVisible();
  await expect(page.getByRole("link", { name: "Export CSV" })).toHaveAttribute(
    "href",
    new RegExp("export.csv$"),
  );
  await page.getByLabel("New account window").fill("14");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/invites`)).json();
  expect(saved.settings.fake_days).toBe(14);
  await page.getByLabel("Invites needed").fill("5");
  await page.getByLabel("Role", { exact: true }).selectOption({ label: "@Fan" });
  await page.getByRole("button", { name: "Add reward" }).click();
  await expect(page.getByRole("list", { name: "Invite rewards" })).toContainText("5 invites");
  await expect(page.getByText("No counted invitations in this period yet.")).toBeVisible();
  await shot(page, "invites-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "invites-phone", true);
});
