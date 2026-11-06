// The moderation pages in a real browser: the mod-log and a rung on the warn ladder, a channel of its
// own and a single event on the server log with someone left alone, and both changes in the staff log.

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}5`;
const user = `${run}9`;

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

test("set the mod-log and the ladder, shape the server log, and find both in the staff log", async ({
  page,
}) => {
  await login(page);
  await page.goto(`/g/${guild}/moderation`);
  await expect(page.getByRole("heading", { name: "MODERATION", exact: true })).toBeVisible();
  await page.getByLabel("Mod-log channel").selectOption({ label: "#mod-log" });
  await page.getByRole("switch", { name: "Tell members" }).click();
  await page.getByRole("button", { name: "Add rung" }).click();
  await page.getByLabel("Warns", { exact: true }).fill("5");
  await page.getByLabel("Brings", { exact: true }).selectOption("kick");
  await page.getByRole("button", { name: "Add rung" }).click();
  await expect(page.getByRole("list", { name: "Warn ladder" }).locator("li")).toHaveCount(2);
  await expect(page.getByText("3 unsaved changes")).toBeVisible();
  await shot(page, "moderation-edited", true);
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/moderation`)).json();
  expect(saved.log_channel_id).toBe("7021");
  expect(saved.dm_members).toBe(false);
  expect(saved.ladder.map((r: { label: string }) => r.label)).toEqual([
    "3 warns → 1h timeout",
    "5 warns → kick",
  ]);

  // A timeout without a length is refused with the command's sentence.
  await page.getByLabel("How long, at 3 warns").fill("");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("alert")).toContainText("Say how long");
  await page.getByRole("button", { name: "Reset" }).click();

  await page.goto(`/g/${guild}/log`);
  await expect(page.getByRole("heading", { name: "SERVER LOG", exact: true })).toBeVisible();
  await page.getByLabel("Channel", { exact: true }).selectOption({ label: "#general" });
  await page.getByLabel("Channel for Moderation").selectOption({ label: "#mod-log" });
  await page.getByRole("switch", { name: "Message edited" }).click();
  await page.getByRole("switch", { name: "Self mute or deafen" }).click();
  await page.locator("#ignore-role").selectOption({ label: "@Fan" });
  await page.getByRole("button", { name: "Add", exact: true }).nth(1).click();
  await expect(page.locator(".channel-chip")).toContainText("@Fan");
  await expect(page.getByText("5 unsaved changes")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const logs = await (await page.request.get(`/api/guilds/${guild}/logs`)).json();
  expect(logs.channel_id).toBe("7001");
  expect(logs.ignored_roles).toEqual(["9001"]);
  await shot(page, "server-log-edited", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "server-log-edited-phone", true);
  await page.setViewportSize({ width: 1280, height: 800 });

  await page.goto(`/g/${guild}/staff-log`);
  await expect(page.getByText(/changed the server log/)).toBeVisible();
  await expect(page.getByText(/changed the moderation settings/)).toBeVisible();
  await page.getByLabel("Where").selectOption("command");
  await expect(page.getByText("Nothing matches.")).toBeVisible();
  await page.getByLabel("Where").selectOption("");
  await shot(page, "staff-log-filled", true);

  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText(/changed the server log/)).toBeVisible();
  await expect(page.getByText(/changed the moderation settings/)).toBeVisible();
});
