import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("collection book and wishlist work at desktop and phone widths", async ({ page }) => {
  await devLogin(page, String(770000000000088084n + BigInt(Date.now())), []);
  await page.goto("/me/collection");
  await expect(page.getByRole("heading", { name: /collection book/i })).toHaveCount(1);
  await expect(page.getByText(/48|84/).first()).toBeVisible();
  await expect(page.getByRole("button", { name: /roll a card/i })).toBeVisible();
  await shot(page, "collection-book-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect.poll(() => overflow(page)).toBeFalsy();
  await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
  await shot(page, "collection-book-phone", true);
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

test("recycling waits for confirmation and reuses its operation key", async ({ page }) => {
  await devLogin(page, "770000000000088085", []);
  let writes = 0;
  const requests: string[] = [];
  await page.route("**/api/me/collection/book", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: "crab",
            name: "Crab",
            kind: "critter",
            rarity: "Common",
            caught: 3,
            kept: 3,
            glimmer: 0,
            habitat: "shore",
          },
        ],
        found: 1,
        total: 84,
        shards: 24,
        fragments: 0,
        craft_cost: 12,
        sets: [],
        roll: null,
      },
    }),
  );
  await page.route("**/api/me/collection/recycle", async (route) => {
    writes++;
    const body = route.request().postDataJSON();
    expect(body.confirmed).toBe(true);
    expect(body.request_id).toBeTruthy();
    requests.push(body.request_id);
    if (writes === 1) {
      await route.fulfill({ status: 503, json: { error: { code: "unavailable", message: "Try again" } } });
      return;
    }
    await route.fulfill({ json: { shards: 2, earned: 2 } });
  });
  await page.goto("/me/collection");
  await page.getByRole("button", { name: "Recycle spare" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  expect(writes).toBe(0);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(writes).toBe(0);
  await page.getByRole("button", { name: "Recycle spare" }).click();
  await page.getByRole("button", { name: "Confirm recycle", exact: true }).click();
  await expect.poll(() => writes).toBe(1);
  await expect(page.getByRole("alert")).toBeVisible();
  await page.getByRole("button", { name: "Confirm recycle", exact: true }).click();
  await expect.poll(() => writes).toBe(2);
  expect(new Set(requests).size).toBe(1);
  const crafts: string[] = [];
  await page.route("**/api/me/collection/craft", async (route) => {
    crafts.push(route.request().postDataJSON().request_id);
    await route.fulfill(
      crafts.length === 1
        ? { status: 503, json: { message: "Try again" } }
        : { json: { item: "decor", shards: 12, cost: 12 } },
    );
  });
  await page.getByRole("button", { name: /Craft bound loot/ }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await page.getByRole("button", { name: /Craft bound loot/ }).click();
  await expect.poll(() => crafts.length).toBe(2);
  expect(new Set(crafts).size).toBe(1);
});
