import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("branding previews nickname, bio, colour and fits a phone", async ({ page }) => {
  const guild = `${Date.now()}77`;
  const state = {
    nickname: null,
    avatar_upload_id: null,
    banner_upload_id: null,
    bio: null,
    embed_color: null,
    applied_at: null,
    last_error: null,
    avatar_supported: null,
    banner_supported: null,
    bio_supported: null,
    avatar_url: null,
    banner_url: null,
  };
  await page.route(`**/api/guilds/${guild}/branding**`, async (route) => {
    if (route.request().method() === "PUT") Object.assign(state, route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(state) });
  });
  await devLogin(page, "1", [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "32" }]);
  await page.goto(`/g/${guild}/branding`);
  await expect(page.getByRole("heading", { name: "Server branding" })).toBeVisible();
  await page.getByLabel("Server nickname").fill("Coral Helper");
  await page.getByLabel("Bio").fill("Your reef guide");
  await page.getByLabel("Hex colour").fill("#31B8D4");
  await expect(page.getByLabel("Live preview")).toContainText("Coral Helper");
  await expect(page.getByLabel("Live preview")).toContainText("Your reef guide");
  await shot(page, "branding-desktop", true);
  await page.getByRole("group", { name: "Language" }).getByRole("button", { name: "TR" }).click();
  await expect(page.getByRole("heading", { name: "Sunucu marka kimliği" })).toBeVisible();
  await expect(page.getByLabel("Canlı önizleme")).toContainText("Coral Helper");
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "branding-phone", true);
});
