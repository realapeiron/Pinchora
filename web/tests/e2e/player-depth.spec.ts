import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot, switchPanelLanguage } from "./helpers";

test("player depth pages have one title and fit on desktop and phone in both languages", async ({ page }) => {
  await devLogin(page, `${Date.now()}81`, []);
  for (const language of ["EN", "TR"]) {
    for (const path of ["inventory", "pets", "stall", "fishbook", "quests", "guestbook", "friends"]) {
      await page.goto(`/me/${path}`);
      await switchPanelLanguage(page, language as "EN" | "TR");
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

test("populated pet meters and long filters fit at 390 px", async ({ page }) => {
  await devLogin(page, `${Date.now()}82`, []);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route("**/api/me/pets?*", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: "42",
            name: "WWWWWWWWWWWWWWWWWWWW",
            species_name: "Crab",
            level: 1,
            xp: 0,
            active: true,
            status: "awake",
            needs: { hunger: 100, fun: 100, energy: 100, hygiene: 100 },
            trip_dest: null,
          },
        ],
        trips: [{ id: "trip", name: "Very long destination name for a narrow phone", hours: 1, level: 1 }],
      },
    }),
  );
  await page.route("**/api/me/inventory?*", (route) =>
    route.fulfill({
      json: {
        items: [],
        total: 0,
        categories: [{ id: "chairs", name: "A very long furniture category name on a phone" }],
        rarities: [],
        sets: [],
      },
    }),
  );
  for (const language of ["EN", "TR"]) {
    await page.goto("/me/pets");
    await switchPanelLanguage(page, language as "EN" | "TR");
    await expect(page.locator(".player-needs progress")).toHaveCount(4);
    expect(await overflow(page)).toBeLessThanOrEqual(1);
    await page.goto("/me/inventory");
    await expect(page.locator(".player-depth-filters select").first()).toBeVisible();
    expect(await overflow(page)).toBeLessThanOrEqual(1);
  }
});
