import { expect, test } from "@playwright/test";
import { devLogin, testSql } from "./helpers";

test("settings and module Undo leave another manager's changes alone", async ({ page, browser }) => {
  const guild = `${Date.now()}91`;
  await devLogin(page, `${Date.now()}92`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "0" },
  ]);
  testSql(
    `INSERT INTO guilds(id,points_name) VALUES(${guild},'points') ON CONFLICT(id) DO UPDATE SET points_name='points'`,
  );
  await page.goto(`/g/${guild}/points`);
  await page.getByLabel("Name", { exact: true }).fill("coins");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("button", { name: "Undo", exact: true })).toBeVisible();
  await expect(page.locator("#points-name")).toHaveValue("coins");
  const otherContext = await browser.newContext({ baseURL: new URL(page.url()).origin });
  const otherPage = await otherContext.newPage();
  await devLogin(otherPage, `${Date.now()}95`, [
    { id: guild, name: "Pinchora Reef", owner: false, permissions: "32" },
  ]);
  const csrf = (await (await otherPage.request.get("/api/me")).json()).csrf as string;
  const headers = { "X-CSRF-Token": csrf };
  const other = await otherPage.request.patch(`/api/guilds/${guild}/settings`, {
    headers,
    data: { points_name: "stars" },
  });
  expect(other.ok(), await other.text()).toBeTruthy();
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Some values changed since this save and were left alone." }),
  ).toBeVisible();
  const settings = (await (await page.request.get(`/api/guilds/${guild}`)).json()).settings;
  expect(settings.points_name).toBe("stars");

  await page.goto(`/g/${guild}`);
  const catalog = page.getByRole("region", { name: "Module catalog" });
  await catalog.getByRole("searchbox", { name: "Search modules" }).fill("Pets");
  const pets = catalog.getByRole("switch", { name: "Pets module", exact: true });
  await pets.click();
  await expect(pets).toHaveAttribute("aria-checked", "false");
  await expect(page.getByRole("button", { name: "Undo", exact: true })).toBeVisible();
  const switched = await otherPage.request.put(`/api/guilds/${guild}/modules/pets`, {
    headers,
    data: { enabled: true },
  });
  expect(switched.ok(), await switched.text()).toBeTruthy();
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "The module changed since this save and was left alone." }),
  ).toBeVisible();
  await expect(pets).toHaveAttribute("aria-checked", "true");
  const modules = (await (await page.request.get(`/api/guilds/${guild}/modules`)).json()).items;
  expect(modules.find((item: { key: string }) => item.key === "pets").enabled).toBe(true);
  await otherContext.close();
});

test("Command permissions switch confirms off and can be restored", async ({ page }) => {
  const guild = `${Date.now()}93`;
  await devLogin(page, `${Date.now()}94`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "0" },
  ]);
  testSql(`INSERT INTO guilds(id) VALUES(${guild}) ON CONFLICT(id) DO NOTHING`);
  await page.goto(`/g/${guild}/command-control`);
  const master = page.locator(".module-master").getByRole("switch");
  await expect(master).toHaveAttribute("aria-checked", "true");
  await master.click();
  const dialog = page.getByRole("dialog", { name: "Turn off command permissions?" });
  await expect(dialog).toContainText(
    "Pinchora stops applying command permissions and aliases until you switch this back on. Discord may still enforce permissions set in Server Settings. Nothing is deleted.",
  );
  await dialog.getByRole("button", { name: "Turn off", exact: true }).click();
  await expect(master).toHaveAttribute("aria-checked", "false");
  const off = (await (await page.request.get(`/api/guilds/${guild}/modules`)).json()).items;
  expect(off.find((item: { key: string }) => item.key === "command_control").enabled).toBe(false);
  await master.click();
  await expect(master).toHaveAttribute("aria-checked", "true");
  const on = (await (await page.request.get(`/api/guilds/${guild}/modules`)).json()).items;
  expect(on.find((item: { key: string }) => item.key === "command_control").enabled).toBe(true);
});
