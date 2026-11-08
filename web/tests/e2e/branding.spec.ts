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
    active_nickname: null,
    active_bio: null,
    active_avatar_upload_id: null,
    active_banner_upload_id: null,
    active_embed_color: null,
    applied_at: null,
    last_error: null,
    avatar_supported: null,
    banner_supported: null,
    bio_supported: null,
    avatar_url: null,
    banner_url: null,
  };
  const payloads: unknown[] = [];
  await page.route(`**/api/guilds/${guild}/branding`, async (route) => {
    if (route.request().method() === "PUT") {
      const payload = route.request().postDataJSON();
      payloads.push(payload);
      Object.assign(state, payload, { embed_color: payload.embed_color?.toUpperCase() ?? null });
    }
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(state) });
  });
  await devLogin(page, "1", [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "32" }]);
  await page.goto(`/g/${guild}/branding`);
  await expect(page.getByRole("heading", { level: 1, name: "Server branding", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Server branding", exact: true })).toHaveCount(1);
  const save = page.getByRole("button", { name: "Save changes", exact: true });
  await expect(save).toBeDisabled();
  await page.getByLabel("Server nickname").fill("Temporary");
  await expect(save).toBeEnabled();
  await page.getByLabel("Server nickname").fill("");
  await expect(save).toBeDisabled();
  await page.getByLabel("Server nickname").fill("Coral Helper");
  await page.getByLabel("Bio").fill("Your reef guide");
  await page.getByLabel("Hex colour").fill("#31b8d4");
  await expect(page.getByLabel("Live preview")).toContainText("Coral Helper");
  await expect(page.getByLabel("Live preview")).toContainText("Your reef guide");
  await save.click();
  await expect(page.getByRole("status").filter({ hasText: "Changes saved." })).toBeVisible();
  await expect(save).toBeDisabled();
  await expect(page.getByLabel("Hex colour")).toHaveValue("#31B8D4");
  expect(payloads).toEqual([
    {
      nickname: "Coral Helper",
      bio: "Your reef guide",
      avatar_upload_id: null,
      banner_upload_id: null,
      embed_color: "#31b8d4",
    },
  ]);
  await shot(page, "branding-desktop", true);
  const languageSaved = page.waitForResponse(
    (response) => response.url().endsWith("/api/me/language") && response.request().method() === "POST",
  );
  await page.getByRole("group", { name: "Language" }).getByRole("button", { name: "TR" }).click();
  expect((await languageSaved).ok()).toBeTruthy();
  expect((await (await page.request.get("/api/me")).json()).language).toBe("tr");
  await expect(
    page.getByRole("heading", { level: 1, name: "Sunucu marka kimliği", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "Sunucu marka kimliği", exact: true })).toHaveCount(1);
  await expect(page.getByLabel("Canlı önizleme")).toContainText("Coral Helper");
  await page.setViewportSize({ width: 390, height: 844 });
  // Long unbroken profile text must wrap inside the narrow preview.
  await page.locator('.branding-editor input[maxlength="32"]').fill("C".repeat(32));
  await page.locator(".branding-editor textarea").fill("B".repeat(190));
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "branding-phone", true);

  // Reusing this account must seed English again, even with a cached login and saved TR preference.
  await devLogin(page, "1", [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "32" }]);
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "Server branding", exact: true })).toBeVisible();
});
