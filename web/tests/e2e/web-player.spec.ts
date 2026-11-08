import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "api", "seed_web_player", "--", "--ignored", "--test-threads=2"],
    {
      cwd: fileURLToPath(new URL("../../..", import.meta.url)),
      stdio: "inherit",
      env: { ...process.env, CARGO_BUILD_JOBS: "4" },
    },
  );
});
test("ordinary player edits badges, accent, background and saves at desktop and phone widths", async ({
  page,
}) => {
  await devLogin(page, "770000000000000051", []);
  await page.goto("/me");
  await expect(page.getByRole("heading", { name: "My world", exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Edit profile card", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Profile studio", exact: true })).toBeVisible();
  await expect(page.getByRole("img", { name: "Profile card preview" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
  const pick = page.getByRole("button", { name: "Select badge", exact: true });
  await pick.first().click();
  await pick.first().click();
  await page
    .getByRole("button", { name: /^Reorder / })
    .last()
    .focus();
  await page.keyboard.press("Alt+ArrowUp");
  const ordered = page.getByRole("button", { name: /^Reorder / });
  await ordered.first().dragTo(ordered.last());
  await page.getByRole("button", { name: "Style", exact: true }).click();
  await page.getByLabel("Accent colour", { exact: true }).fill("#06d6a0");
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Profile saved." })).toBeVisible();
  await page.getByRole("button", { name: "Background", exact: true }).click();
  await page.getByRole("button", { name: "Try background", exact: true }).first().click();
  await expect(page.getByText("Buy this background before saving it.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
  const selected = page.locator(".player-collectible.is-picked");
  await selected.getByRole("button", { name: /^Buy · / }).click();
  await expect(page.getByText("Added to your collection.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Profile saved." })).toBeVisible();
  await shot(page, "web-player-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "web-player-phone", true);
  await page.getByRole("link", { name: "Wallet", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Wallet", exact: true })).toBeVisible();
});

test("character randomise is a draft, outfit saves, and phone controls fit", async ({ page }) => {
  await devLogin(page, `${run}92`, []);
  await page.goto("/me/character");
  await expect(page.getByRole("heading", { name: "Character", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Randomise", exact: true }).click();
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByText("Character saved.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Outfits", exact: true }).click();
  await page.getByLabel("Outfit name 1", { exact: true }).fill("Reef day");
  await page.getByRole("button", { name: "Save to slot", exact: true }).first().click();
  await expect(page.getByText("Outfit collection updated.")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await shot(page, "web-player-character-phone", true);
});

test("profile draft survives language changes and edits made while saving", async ({ page }) => {
  await devLogin(page, `${run}93`, []);
  await page.goto("/me/profile");
  await page.getByRole("button", { name: "Bio", exact: true }).click();
  await page.getByLabel("Bio", { exact: true }).fill("A draft worth keeping");
  await page.route("**/api/me/profile?lang=tr", (route) =>
    route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({ error: { code: "fixture", message: "Fixture catalog unavailable" } }),
    }),
  );
  await page.getByRole("button", { name: "TR", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Fixture catalog unavailable", { timeout: 15000 });
  await expect(page.getByLabel("Biyografi", { exact: true })).toHaveValue("A draft worth keeping");
  await page.unroute("**/api/me/profile?lang=tr");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await expect(page.getByLabel("Bio", { exact: true })).toHaveValue("A draft worth keeping");
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/api/me/profile", async (route) => {
    if (route.request().method() !== "PUT") return route.continue();
    const response = await route.fetch();
    await gate;
    await route.fulfill({ response });
  });
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await page.getByLabel("Bio", { exact: true }).fill("The next draft");
  release?.();
  await expect(page.getByLabel("Bio", { exact: true })).toHaveValue("The next draft");
  await expect(page.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
});
