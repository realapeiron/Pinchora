import { expect, test } from "@playwright/test";
import { devLogin } from "./helpers";

test("old community URL redirects to engagement with guild, query and hash", async ({ page }) => {
  const guild = `${Date.now()}74`;
  await devLogin(page, `${Date.now()}75`, [{ id: guild, name: "Community", owner: true, permissions: "0" }]);
  await page.goto("/servers");
  await page.goto(`/g/${guild}/community-pack?tab=forms#editor`);
  await expect(page).toHaveURL(new RegExp(`/g/${guild}/community-engagement\\?tab=forms#editor$`));
  await page.goBack();
  await expect(page).toHaveURL(/\/servers$/);
});

test("old moderation URL redirects to the desk with guild, query and hash", async ({ page }) => {
  const guild = `${Date.now()}76`;
  await devLogin(page, `${Date.now()}77`, [
    { id: guild, name: "Moderation desk", owner: true, permissions: "0" },
  ]);
  await page.goto("/servers");
  await page.goto(`/g/${guild}/mod-parity?tab=queue#appeals`);
  await expect(page).toHaveURL(new RegExp(`/g/${guild}/moderation-desk\\?tab=queue#appeals$`));
  await page.goBack();
  await expect(page).toHaveURL(/\/servers$/);
});
