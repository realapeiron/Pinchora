import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";
test("command controls and backup preview work on desktop and phone", async ({ page }) => {
  const guild = `${Date.now()}64`;
  const user = `${Date.now()}65`;
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/command-control`);
  await expect(page.getByRole("heading", { name: "Commands", exact: true })).toBeVisible();
  await page.getByLabel("Search commands", { exact: true }).fill("wordle");
  await page.getByRole("button", { name: "Edit command: /play wordle", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByLabel("Additional cooldown (seconds)").fill("60");
  await page.screenshot({ path: "../output/playwright/command-control-desktop.png", fullPage: true });
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByLabel("Alias", { exact: true }).fill("w");
  await page.getByLabel("Command", { exact: true }).selectOption("play wordle");
  await page.getByRole("button", { name: "Add alias", exact: true }).click();
  await expect(page.getByText("w → play wordle", { exact: true })).toBeVisible();
  const exported = await page.request.get(`/api/guilds/${guild}/backup/export`);
  expect(exported.ok(), await exported.text()).toBeTruthy();
  const backup = await exported.json();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Edit command: /play wordle", exact: true }).click();
  await page.screenshot({ path: "../output/playwright/command-control-phone.png", fullPage: true });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await page.goto(`/g/${guild}/backup`);
  await expect(page.getByRole("heading", { name: "Backup", exact: true })).toBeVisible();
  await page.getByLabel("Choose a JSON backup").setInputFiles({
    name: "backup.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(backup)),
  });
  await page.getByRole("button", { name: "Preview changes", exact: true }).click();
  await expect(page.getByText("Preview ready. No changes have been saved.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Import these changes", exact: true })).toBeEnabled();
  await page.screenshot({ path: "../output/playwright/backup-phone.png", fullPage: true });
  expect(await overflow(page)).toBeLessThanOrEqual(1);
  await page.getByRole("button", { name: "Import these changes", exact: true }).click();
  await expect(page.getByText("Configuration imported. A restore point was saved first.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Preview restore", exact: true })).toHaveCount(1);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.screenshot({ path: "../output/playwright/backup-desktop.png", fullPage: true });
});

test("switching cached guilds clears backup previews and command drafts", async ({ page }) => {
  const first = `${Date.now()}71`;
  const second = `${Date.now()}72`;
  await devLogin(page, `${Date.now()}73`, [
    { id: first, name: "First reef", owner: true, permissions: "0" },
    { id: second, name: "Second reef", owner: true, permissions: "0" },
  ]);
  const navigate = async (path: string) => {
    await page.evaluate((url) => {
      history.pushState({}, "", url);
      dispatchEvent(new PopStateEvent("popstate"));
    }, path);
  };
  await page.goto(`/g/${second}/backup`);
  await expect(page.getByRole("heading", { name: "Backup", exact: true })).toBeVisible();
  await navigate(`/g/${first}/backup`);
  const exported = await page.request.get(`/api/guilds/${first}/backup/export`);
  expect(exported.ok()).toBeTruthy();
  await page.getByLabel("Choose a JSON backup").setInputFiles({
    name: "first-guild-only.json",
    mimeType: "application/json",
    buffer: Buffer.from(await exported.text()),
  });
  await page.getByRole("button", { name: "Preview changes", exact: true }).click();
  await expect(page.getByRole("button", { name: "Import these changes", exact: true })).toBeEnabled();
  await navigate(`/g/${second}/backup`);
  await expect(page.getByText("first-guild-only.json", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Import these changes", exact: true })).toHaveCount(0);
  await navigate(`/g/${second}/command-control`);
  await expect(page.getByLabel("Alias", { exact: true })).toBeVisible();
  await navigate(`/g/${first}/command-control`);
  await page.getByLabel("Alias", { exact: true }).fill("firstguild");
  await navigate(`/g/${second}/command-control`);
  await expect(page.getByLabel("Alias", { exact: true })).toHaveValue("");
});
