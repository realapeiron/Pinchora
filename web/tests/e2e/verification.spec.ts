import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";
test("Safety settings, captcha preview, and desktop/phone layouts", async ({ page }) => {
  const stamp = Date.now();
  const guild = `${stamp}7`;
  const user = `${stamp}8`;
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/safety`);
  await expect(page.getByRole("heading", { name: "Safety", exact: true })).toBeVisible();
  await page.getByLabel("Minimum account age (hours)", { exact: true }).fill("24");
  await page.getByRole("button", { name: "Save settings", exact: true }).click();
  await expect(page.locator(".save-bar")).not.toHaveClass(/is-open/);
  const saved = await page.request.get(`/api/guilds/${guild}/safety`);
  expect((await saved.json()).config.verification.min_account_age_hours).toBe(24);
  await page.getByRole("button", { name: "Preview captcha", exact: true }).click();
  await expect(page.getByRole("img", { name: "Sample pixel captcha card" })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: "../target/sim/verification/desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.screenshot({ path: "../target/sim/verification/phone.png", fullPage: true });
});

test("A wrong captcha answer replaces the image and clears the answer", async ({ page }) => {
  const stamp = Date.now();
  const guild = `${stamp}7`;
  const user = `${stamp}8`;
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  const settings = await page.request.get(`/api/guilds/${guild}/safety`);
  const { strings } = await settings.json();
  let issued = 0;
  const answers: string[] = [];
  await page.route("**/api/verify/ui-regression**", async (route) => {
    const url = route.request().url();
    let json;
    if (url.endsWith("/captcha")) {
      issued += 1;
      json = {
        nonce: `nonce-${issued}`,
        image:
          "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a3ioAAAAASUVORK5CYII=",
      };
    } else if (url.endsWith("/answer")) {
      answers.push(route.request().postDataJSON().nonce);
      json = {
        message: answers.length === 1 ? strings.new_card : strings.verified,
        done: answers.length > 1,
      };
    } else {
      json = { strings };
    }
    await route.fulfill({ json });
  });
  await page.goto("/verify/ui-regression");
  await page.getByRole("button", { name: strings.get_card, exact: true }).click();
  await page.locator("#verify-answer").fill("AAAAA");
  await page.getByRole("button", { name: strings.submit, exact: true }).click();
  await expect.poll(() => issued).toBe(2);
  await expect(page.locator("#verify-answer")).toHaveValue("");
  await page.locator("#verify-answer").fill("BBBBB");
  await page.getByRole("button", { name: strings.submit, exact: true }).click();
  await expect(page.locator("#verify-answer")).toHaveCount(0);
  expect(answers).toEqual(["nonce-1", "nonce-2"]);
});
