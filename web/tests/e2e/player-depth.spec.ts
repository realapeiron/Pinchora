import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("player depth pages have one title and fit on desktop and phone in both languages", async ({ page }) => {
  await devLogin(page, `${Date.now()}81`, []);
  for (const language of ["EN", "TR"]) {
    for (const path of ["inventory", "pets", "stall", "fishbook", "quests", "guestbook", "friends"]) {
      await page.goto(`/me/${path}`);
      await page.getByRole("button", { name: language, exact: true }).click();
      await expect(page.locator("h1:visible")).toHaveCount(1);
      expect(await overflow(page)).toBeLessThanOrEqual(1);
      if (path === "pets" || path === "inventory")
        await shot(page, `player-depth-${path}-${language.toLowerCase()}-desktop`, true);
      await page.setViewportSize({ width: 390, height: 844 });
      expect(await overflow(page)).toBeLessThanOrEqual(1);
      if (path === "pets" || path === "inventory")
        await shot(page, `player-depth-${path}-${language.toLowerCase()}-phone`, true);
      await page.setViewportSize({ width: 1280, height: 800 });
    }
  }
});
