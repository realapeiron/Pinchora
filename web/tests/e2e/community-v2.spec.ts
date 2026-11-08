import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}1`;
const user = `${stamp}2`;

test("community tools pages and personal lists work at desktop and phone widths", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);

  const pages = [
    ["giveaways", "GIVEAWAYS"],
    ["polls", "POLLS"],
    ["suggestions", "SUGGESTIONS"],
    ["reminders", "REMINDERS & CHECKLISTS"],
    ["highlights", "KEYWORD HIGHLIGHTS"],
    ["auto-channels", "AUTO CHANNELS"],
    ["anniversaries", "MEMBER ANNIVERSARIES"],
    ["confessions", "ANONYMOUS CONFESSIONS"],
  ] as const;
  for (const [path, title] of pages) {
    await page.goto(`/g/${guild}/${path}`);
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
  }

  await page.goto(`/g/${guild}/reminders`);
  await page.getByLabel("Reminder text").fill("Check the reef");
  await expect(page.getByRole("button", { name: "Add reminder" })).toBeEnabled();
  await page.getByRole("button", { name: "Add reminder" }).click();
  await expect(page.getByText("Check the reef")).toBeVisible();
  await page.getByLabel("New task").fill("Welcome newcomers");
  await page.getByRole("button", { name: "Add task" }).click();
  await expect(page.getByText("Welcome newcomers")).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();
  await expect(page.getByText("☑ Welcome newcomers")).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  for (const [path, title] of pages) {
    await page.goto(`/g/${guild}/${path}`);
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
    expect(await overflow(page), path).toBeLessThanOrEqual(0);
  }
});
