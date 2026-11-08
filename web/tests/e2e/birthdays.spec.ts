// Birthday setup in a real browser at desktop and phone widths.
import { type Page, expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}6`;
const user = `${run}7`;
async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

test("birthdays settings, preview and phone layout", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/birthdays`);
  await expect(page.getByRole("heading", { name: "BIRTHDAYS", exact: true })).toBeVisible();
  await expect(page.getByText("No shared birthdays in the next 30 days.")).toBeVisible();
  await shot(page, "birthdays-desktop", true);
  await page.getByRole("switch", { name: "Announce birthdays" }).click();
  await page.getByLabel("Announcement channel").selectOption("7001");
  await page.getByLabel("Server time zone").fill("Europe/Istanbul");
  await page.getByLabel("Shell gift").fill("100");
  await page.getByRole("switch", { name: "Cake Day badge" }).click();
  await page.getByRole("button", { name: "Preview card" }).click();
  await expect(page.getByRole("img", { name: "Birthday card preview" })).toContainText("CAKE DAY");
  await shot(page, "birthdays-preview", true);
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Birthday settings saved" })).toBeVisible();
  const data = await (await page.request.get(`/api/guilds/${guild}/birthdays`)).json();
  expect(data.settings).toMatchObject({ enabled: true, channel_id: "7001", shells_gift: 100, badge: true });
  await page.goto(`/g/${guild}/community`);
  await expect(page.getByRole("heading", { name: "BIRTHDAYS", exact: true })).toBeVisible();
  await expect(page.getByText("Announcements on")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/birthdays`);
  await expect(page.getByRole("heading", { name: "BIRTHDAYS", exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "birthdays-phone", true);
});
