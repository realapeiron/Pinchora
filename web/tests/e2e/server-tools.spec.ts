import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
for (const language of ["en", "tr"] as const) {
  test(`access and audit tools: ${language}, desktop and phone`, async ({ page }) => {
    const guild = `${run}${language === "en" ? "91" : "92"}`;
    const user = `${run}${language === "en" ? "93" : "94"}`;
    await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
    const me = await (await page.request.get("/api/me")).json();
    await page.request.post("/api/me/language", { headers: { "X-CSRF-Token": me.csrf }, data: { language } });
    await page.goto(`/g/${guild}/access`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(language === "en" ? "ACCESS" : "ERİŞİM");
    await expect(page.locator(".settings-detail-row")).toHaveCount(1);
    await page.locator("#access-role").selectOption("9001");
    await page
      .getByRole("button", { name: language === "en" ? "Add manager role" : "Yönetici rolü ekle" })
      .click();
    await page.getByRole("checkbox", { name: language === "en" ? "Roles" : "Roller", exact: true }).check();
    await page.getByRole("button", { name: language === "en" ? "Preview access" : "Erişimi önizle" }).click();
    const save = page.getByRole("button", {
      name: language === "en" ? "Save changes" : "Değişiklikleri kaydet",
      exact: true,
    });
    // Use the form's accessible save region so localization stays covered without selector ambiguity.
    const saveButton = page.locator(".save-bar .pixel-button-primary");
    await expect(saveButton).toBeEnabled();
    await saveButton.click();
    await expect(page.locator(".save-bar")).not.toHaveClass(/is-open/);
    const accessSaved = await (await page.request.get(`/api/guilds/${guild}/access`)).json();
    expect(accessSaved.items).toEqual(expect.arrayContaining([expect.objectContaining({ role_id: "9001" })]));
    await expect(save).not.toBeVisible();
    for (const viewport of [
      { width: 1280, height: 800 },
      { width: 390, height: 844 },
    ]) {
      await page.setViewportSize(viewport);
      expect(await overflow(page)).toBeLessThanOrEqual(1);
      await shot(page, `w7-access-${language}-${viewport.width}`, true);
    }
    await page.goto(`/g/${guild}/audit`);
    await page.locator("#audit-area").fill("access");
    await page.locator("#audit-actor").fill(user);
    await page
      .getByRole("button", { name: language === "en" ? "Apply filters" : "Filtreleri uygula" })
      .click();
    await expect(page.locator(".audit-entry")).toHaveCount(1);
    await page.locator(".audit-summary").click();
    await expect(page.locator(".audit-diff")).toContainText("9001");
    const download = page.waitForEvent("download");
    await page.locator("a[download]").click();
    expect((await download).suggestedFilename()).toBe("pinchora-audit.csv");
    for (const viewport of [
      { width: 1280, height: 800 },
      { width: 390, height: 844 },
    ]) {
      await page.setViewportSize(viewport);
      expect(await overflow(page)).toBeLessThanOrEqual(1);
      await shot(page, `w7-audit-${language}-${viewport.width}`, true);
    }
    await page.locator("#audit-text").fill("does-not-exist");
    await page
      .getByRole("button", { name: language === "en" ? "Apply filters" : "Filtreleri uygula" })
      .click();
    await expect(page.locator(".audit-entry")).toHaveCount(0);
  });
}

test("public board opt-in and opt-out reuse the existing slug policy", async ({ page }) => {
  const guild = `${run}95`;
  await devLogin(page, `${run}96`, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/leveling`);
  const toggle = page.getByRole("switch", { name: "Public leaderboard", exact: true });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  const slug = `w7-${run}`;
  await page.locator("#parity-vanity").fill(slug);
  await page.getByRole("button", { name: "Save XP options", exact: true }).click();
  await expect
    .poll(async () => (await page.request.get(`/api/public/leaderboard/${slug}`)).status())
    .toBe(200);
  await toggle.click();
  await page.getByRole("button", { name: "Save XP options", exact: true }).click();
  await expect
    .poll(async () => (await page.request.get(`/api/public/leaderboard/${slug}`)).status())
    .toBe(404);
});
