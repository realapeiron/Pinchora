import { expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}81`;
const manager = `${stamp}82`;
const member = `${stamp}83`;

test("member XP editor and MEE6 review fit desktop and phone", async ({ page }) => {
  await devLogin(page, manager, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.route(`**/api/guilds/${guild}/xp-admin/members?search=*`, async (route) => {
    await route.fulfill({ json: { items: [{ id: member, name: "Moon Crab", xp: 0 }] } });
  });
  await page.goto(`/g/${guild}/leveling`);
  await expect(page.getByRole("heading", { name: "MEMBER XP" })).toBeVisible();
  await page.getByLabel("Find member").fill("Moon");
  await expect(page.getByRole("list", { name: "Member search results" })).toContainText("Moon Crab");
  await page.getByRole("radio", { name: /Moon Crab/ }).check();
  await page.getByLabel("Amount").fill("220");
  await page.getByRole("button", { name: "Save XP", exact: true }).click();
  await expect(page.getByText("Member XP saved.")).toBeVisible();
  await page.getByRole("button", { name: "Review import", exact: true }).click();
  await expect(page.getByText("Existing XP stays if it is higher than MEE6 XP.")).toBeVisible();
  await shot(page, "info-xp-desktop", true);

  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "info-xp-phone", true);
});
