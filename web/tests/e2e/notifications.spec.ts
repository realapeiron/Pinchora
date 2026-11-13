import { expect, test } from "@playwright/test";
import { devLogin, testSql } from "./helpers";

const run = Date.now();
const guild = `${run}6`;
const manager = `${run}7`;
const other = `${run}8`;

function seed() {
  testSql(`
    INSERT INTO tickets(guild_id,number,opener_id,opened_at) VALUES(${guild},1,${other},now()-interval '6 minutes');
    INSERT INTO automod_settings(guild_id) VALUES(${guild});
    INSERT INTO automod_rules(guild_id,name,kind,enabled,log_only,native,params,actions)
      VALUES(${guild},'links','links',true,false,false,'{}','[]');
    INSERT INTO automod_hits(guild_id,rule_id,user_id,channel_id,action,created_at)
      SELECT ${guild},id,${other},123,'delete',date_trunc('hour',now()) FROM automod_rules WHERE guild_id=${guild} AND name='links';
    INSERT INTO automod_hits(guild_id,rule_id,user_id,channel_id,action,created_at)
      SELECT ${guild},id,${other},123,'delete',date_trunc('hour',now()) FROM automod_rules WHERE guild_id=${guild} AND name='links';
    INSERT INTO mod_reports(guild_id,user_id,reporter_id,body,created_at)
      VALUES(${guild},${other},${other},'private body',now()-interval '3 minutes');
    INSERT INTO guild_activity_hourly(guild_id,hour,joins) VALUES(${guild},date_trunc('hour',now()),18);
    INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action,created_at)
      VALUES(${guild},${other},'Other manager','web','settings.patch',now()-interval '1 minute');
  `);
}

test("recent notifications, read mark, new item, and phone sheet", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await devLogin(page, manager, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}`);
  await expect(page.getByRole("heading", { name: "Pinchora Reef", exact: true })).toBeVisible();
  seed();
  await page.reload();
  const bell = page.getByRole("button", { name: "Notifications, 4 unread", exact: true });
  await expect(bell).toBeVisible();
  await bell.click();
  const pop = page.getByRole("dialog", { name: "Notifications", exact: true });
  const desktopBox = await pop.boundingBox();
  expect(desktopBox).not.toBeNull();
  expect(desktopBox!.x + desktopBox!.width).toBeLessThanOrEqual(1440);
  await expect(pop.locator(".notification-item")).toHaveCount(5);
  await expect(pop.locator(".notification-new")).toHaveCount(4);
  const times = await pop
    .locator(".notification-item time")
    .evaluateAll((nodes) => nodes.map((node) => Date.parse(node.getAttribute("datetime") ?? "")));
  expect(times).toEqual([...times].sort((a, b) => b - a));
  await expect(pop.getByText("Dashboard changed", { exact: true })).toBeVisible();
  await expect(pop.getByText("New pending report", { exact: true })).toBeVisible();
  await expect(pop.getByText("AutoMod acted 2 times", { exact: true })).toBeVisible();
  await expect(pop.getByText("Ticket #1 opened", { exact: true })).toBeVisible();
  await expect(pop.getByText("18 new members joined", { exact: true })).toBeVisible();
  await expect(
    pop.getByRole("link").filter({ hasText: "18 new members joined" }).locator(".notification-new"),
  ).toHaveCount(0);
  for (const [title, target] of [
    ["Ticket #1 opened", "/tickets"],
    ["AutoMod acted 2 times", "/automod"],
    ["New pending report", "/moderation-desk"],
    ["18 new members joined", "/stats"],
    ["Dashboard changed", "/audit"],
  ] as const) {
    const link = pop.getByRole("link").filter({ hasText: title });
    await expect(link).toHaveAttribute("href", new RegExp(target));
    await link.click();
    await expect(page).toHaveURL(new RegExp(`/g/${guild}${target}`));
    await page.goto(`/g/${guild}`);
    await page.getByRole("button", { name: "Notifications, 4 unread", exact: true }).click();
  }
  await pop.getByRole("button", { name: "Mark all as read", exact: true }).click();
  await expect(page.getByRole("button", { name: "Notifications", exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Notifications", exact: true })).toBeVisible();
  testSql(`INSERT INTO tickets(guild_id,number,opener_id) VALUES(${guild},2,${other});`);
  await page.reload();
  await expect(page.getByRole("button", { name: "Notifications, 1 unread", exact: true })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Notifications, 1 unread", exact: true }).click();
  await expect(pop).toBeVisible();
  const phoneBox = await pop.boundingBox();
  expect(phoneBox).not.toBeNull();
  expect(phoneBox!.x).toBe(0);
  expect(phoneBox!.width).toBe(390);
  expect(phoneBox!.y + phoneBox!.height).toBeLessThanOrEqual(844);
  await pop.getByRole("link", { name: /Ticket #2 opened/ }).click();
  await expect(page).toHaveURL(new RegExp(`/g/${guild}/tickets`));
});
