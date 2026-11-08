import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("collection book and wishlist work at desktop and phone widths", async ({ page }) => {
  await devLogin(page, "770000000000088084", []);
  await page.goto("/me/collection");
  await expect(page.getByRole("heading", { name: /collection book/i })).toHaveCount(1);
  await expect(page.getByText(/48|84/).first()).toBeVisible();
  await expect(page.getByRole("button", { name: /roll a card/i })).toBeVisible();
  await shot(page, "collection-book-desktop", true);
  await page.getByRole("link", { name: "Wishlist" }).click();
  await expect(page.getByRole("heading", { name: /critter wishlist/i })).toHaveCount(1);
  await page.getByLabel("Choose a card").selectOption({ index: 1 });
  await page.getByLabel("Ping me when this card spawns").check();
  await page.getByRole("button", { name: "Add wish" }).click();
  await expect(page.getByText("Spawn ping on")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => overflow(page)).toBeFalsy();
  await shot(page, "collection-wishlist-phone", true);
});
