import { type Page, expect, test } from "@playwright/test";
import { devLogin, overflow, testSql } from "./helpers";

const run = Date.now();

function seed(guild: string) {
  testSql(`INSERT INTO mod_cases
    (guild_id, number, kind, target_id, target_name, moderator_id, moderator_name, reason, duration_secs, details, active)
    VALUES
    (${guild}, 1, 'warn', 81001, 'Mia', 91001, 'Captain', 'spam links', null, 'Repeated links', true),
    (${guild}, 2, 'warn', 81001, 'Mia', 91001, 'Captain', 'rude message', null, null, true),
    (${guild}, 3, 'kick', 81002, 'Zed', 91002, 'Morgan', 'raid', null, null, false),
    (${guild}, 4, 'ban', 81001, 'Mia', 91001, 'Captain', 'repeat spam', 3600, null, true);
    INSERT INTO mod_warn_thresholds (guild_id, warns, action, duration_secs) VALUES (${guild}, 3, 'timeout', 3600)`);
}

async function login(page: Page, guild: string, user: string) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "8" }]);
}

test("cases search, counts, sheet, reason, history and URL navigation", async ({ page }) => {
  const guild = `${run}51`,
    user = `${run}91`;
  seed(guild);
  await login(page, guild, user);
  await page.goto(`/g/${guild}/cases`);
  const api = `/api/guilds/${guild}/cases`;
  const first = await (await page.request.get(api)).json();
  const expected = new Map(first.kinds.map((kind: { key: string; count: number }) => [kind.key, kind.count]));
  await expect(
    page.getByRole("button", {
      name: `All ${[...expected.values()].reduce((a, b) => Number(a) + Number(b), 0)}`,
      exact: true,
    }),
  ).toBeVisible();
  // Only kinds with cases get a tab.
  await expect(page.getByRole("button", { name: "Kick 1", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Note 0", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: /Warn 2/, exact: true }).click();
  await expect(page.getByRole("row", { name: "Open case 4" })).toHaveCount(0);
  const afterKind = await (await page.request.get(`${api}?kind=warn`)).json();
  expect(afterKind.kinds.map((kind: { count: number }) => kind.count)).toEqual(
    first.kinds.map((kind: { count: number }) => kind.count),
  );
  await page.getByRole("button", { name: /All 4/, exact: true }).click();
  await page.getByRole("searchbox", { name: "Search cases" }).fill("spam");
  const searched = await (await page.request.get(`${api}?q=spam`)).json();
  await expect(
    page.getByRole("button", {
      name: `All ${searched.kinds.reduce((sum: number, kind: { count: number }) => sum + kind.count, 0)}`,
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByRole("row", { name: "Open case 3" })).toHaveCount(0);
  await page.getByRole("searchbox", { name: "Search cases" }).fill("");
  await page.getByRole("switch", { name: "Active only" }).click();
  await expect(page.getByRole("row", { name: "Open case 3" })).toHaveCount(0);
  await page.getByRole("switch", { name: "Active only" }).click();

  await page.getByRole("row", { name: "Open case 4" }).click();
  const sheet = page.getByRole("dialog", { name: "Case #4" });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole("textbox", { name: "Reason" })).toHaveValue("repeat spam");
  await sheet.getByRole("textbox", { name: "Reason" }).fill("edited reason");
  await sheet.getByRole("button", { name: "Save reason" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Case reason saved." })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Saved", exact: true })).toBeVisible();
  expect((await (await page.request.get(`${api}/4`)).json()).case.reason).toBe("edited reason");
  await sheet.getByRole("button", { name: /#2 · Warn/ }).click();
  await expect(page.getByRole("dialog", { name: "Case #2" })).toBeVisible();
  await expect(
    page.getByRole("dialog", { name: "Case #2" }).getByText(/^Mia has 2 active warnings\. Next: 3 warns → /),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const row = page.getByRole("row", { name: "Open case 3" });
  await row.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Case #3" })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).not.toHaveURL(/case=3/);
  await row.focus();
  await page.keyboard.press("Space");
  await expect(page.getByRole("dialog", { name: "Case #3" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).not.toHaveURL(/case=3/);
  await page.goto(`/g/${guild}/cases?case=1`);
  await expect(page.getByRole("dialog", { name: "Case #1" })).toBeVisible();
  await page.getByRole("button", { name: "Close case" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.goto(`/g/${guild}/cases?case=999`);
  await expect(page.getByRole("alert")).toContainText("Case #999 does not exist.");
});

test("case cards and full-screen sheet fit a phone", async ({ page }) => {
  const guild = `${run}52`,
    user = `${run}92`;
  seed(guild);
  await page.setViewportSize({ width: 390, height: 844 });
  await login(page, guild, user);
  await page.goto(`/g/${guild}/cases`);
  await page.getByRole("button", { name: "Open case 2", exact: true }).click();
  const sheet = page.getByRole("dialog", { name: "Case #2" });
  await expect(sheet).toBeVisible();
  const bounds = await sheet.boundingBox();
  // Layout widths can carry sub-pixel rounding.
  expect(bounds?.width).toBeCloseTo(390, 1);
  expect(bounds?.height).toBe(844);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  const close = sheet.getByRole("button", { name: "Close case" });
  expect((await close.boundingBox())?.height).toBeGreaterThanOrEqual(44);
  await close.click();
  await expect(sheet).toHaveCount(0);
});

test("a warning can be cancelled or revoked from its case", async ({ page }) => {
  const guild = `${run}53`,
    user = `${run}93`;
  seed(guild);
  await login(page, guild, user);
  const mock = `http://127.0.0.1:${process.env.E2E_MOCK_PORT ?? 18191}`;
  await page.request.put(`${mock}/api/v10/guilds/${guild}/members/${user}/roles/9003`);
  await page.goto(`/g/${guild}/cases`);
  const api = `/api/guilds/${guild}/cases`;
  await page.getByRole("row", { name: "Open case 2" }).click();
  const sheet = page.getByRole("dialog", { name: "Case #2" });
  await sheet.getByRole("button", { name: "Revoke", exact: true }).click();
  await expect(
    sheet.getByText("Remove the warning for @Mia? Their active warning count drops by one."),
  ).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Cancel", exact: true })).toBeFocused();
  await sheet.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(sheet.getByRole("button", { name: "Revoke", exact: true })).toBeFocused();
  const cancelled = await (await page.request.get(`${api}/2`)).json();
  expect(cancelled.case.active).toBe(true);
  expect(cancelled.history.some((item: { kind: string }) => item.kind === "unwarn")).toBe(false);
  await expect(page.getByRole("button", { name: "All 4", exact: true })).toBeVisible();
  await sheet.getByRole("button", { name: "Revoke", exact: true }).click();
  await sheet.getByRole("button", { name: "Yes, revoke", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Case revoked." })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Revoke", exact: true })).toHaveCount(0);
  await expect(sheet.getByText("Taken back", { exact: true })).toBeVisible();
  await expect(sheet.getByText(/active warnings?\. Next:/)).toHaveCount(0);
  await sheet.getByRole("button", { name: /#1 · Warn/ }).click();
  await expect(
    page.getByRole("dialog", { name: "Case #1" }).getByText(/^Mia has 1 active warning\. Next:/),
  ).toBeVisible();
  await page
    .getByRole("dialog", { name: "Case #1" })
    .getByRole("button", { name: /#2 · Warn/ })
    .click();
  await expect(sheet.getByRole("button", { name: /^#5 · Warn removed/ })).toBeVisible();
  const detail = await (await page.request.get(`${api}/2`)).json();
  expect(detail.case.active).toBe(false);
  expect(detail.history.some((entry: { kind: string }) => entry.kind === "unwarn")).toBe(true);
  await expect(page.getByRole("button", { name: "All 5", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("switch", { name: "Active only" }).click();
  await expect(page.getByRole("button", { name: "Warn 1", exact: true })).toBeVisible();
});

test("a ban reversal reaches Discord and kick or ended cases have no action", async ({ page }) => {
  const guild = `${run}54`,
    user = `${run}94`;
  seed(guild);
  await login(page, guild, user);
  const mock = `http://127.0.0.1:${process.env.E2E_MOCK_PORT ?? 18191}`;
  await page.request.put(`${mock}/api/v10/guilds/${guild}/members/${user}/roles/9003`);
  await page.goto(`/g/${guild}/cases`);
  await page.getByRole("row", { name: "Open case 3" }).click();
  await expect(
    page.getByRole("dialog", { name: "Case #3" }).getByRole("button", { name: "Revoke", exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.getByRole("row", { name: "Open case 4" }).click();
  const sheet = page.getByRole("dialog", { name: "Case #4" });
  await sheet.getByRole("button", { name: "Revoke", exact: true }).click();
  await expect(sheet.getByText("Remove the ban for @Mia?")).toBeVisible();
  await sheet.getByRole("button", { name: "Yes, revoke", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Case revoked." })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Revoke", exact: true })).toHaveCount(0);
  const seen = await (await page.request.get(`${mock}/mock/reversals`)).json();
  expect(seen.items).toContain(`unban:${guild}:81001`);
  const detail = await (await page.request.get(`/api/guilds/${guild}/cases/4`)).json();
  expect(detail.case.active).toBe(false);
  expect(detail.history.some((entry: { kind: string }) => entry.kind === "unban")).toBe(true);
});
