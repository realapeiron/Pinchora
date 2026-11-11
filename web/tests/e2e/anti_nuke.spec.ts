import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";
const run = Date.now();
const guild = `${run}1`;
const user = "1";
test("anti-nuke draft, limits, safe preview, incident confirmation and responsive layout", async ({
  page,
}) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/anti-nuke`);
  await expect(page.getByRole("heading", { name: "Anti-nuke", exact: true })).toBeVisible();
  await page.getByLabel("Watch destructive actions").click();
  const previewRow = page
    .locator(".antinuke-page .settings-detail-row")
    .filter({ has: page.locator(".antinuke-preview") });
  await previewRow.locator(".settings-detail-expand").click();
  await expect(previewRow.locator(".settings-detail-expand")).toHaveAttribute("aria-expanded", "true");
  await page.getByRole("button", { name: "Run preview" }).click();
  // Slider <output> elements also have the status role; select the preview announcement.
  await expect(page.getByRole("status").filter({ hasText: "Preview only" })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: "../output/playwright/anti-nuke-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.screenshot({ path: "../output/playwright/anti-nuke-phone.png", fullPage: true });
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.getByRole("button", { name: "Save changes" })).toHaveCount(0);
});

test("anti-nuke API checks live ownership, self trust and bot permissions", async ({ request }) => {
  const id = `${Date.now()}4`;
  const login = async (user: string) => {
    const response = await request.post("/auth/dev-login", {
      data: {
        user_id: user,
        username: "safety_manager",
        guilds: [{ id, name: "Safety API", owner: user === "1", permissions: "32", icon: null }],
      },
    });
    expect(response.ok()).toBeTruthy();
    return { "X-CSRF-Token": (await response.json()).csrf as string };
  };
  let headers = await login("1");
  const path = `/api/guilds/${id}/anti_nuke`;
  const data = await (await request.get(path)).json();
  expect(data.can_trust).toBe(true);
  let response = await request.put(path, { headers, data: { ...data.config, trusted_users: ["1"] } });
  expect(response.status()).toBe(422);
  response = await request.put(path, { headers, data: { ...data.config, enabled: true } });
  expect(response.status()).toBe(422);
  expect(await response.text()).toContain("View Audit Log");
  const trusted = `${Date.now()}5`;
  const config = { ...data.config, trusted_users: [trusted] };
  response = await request.put(path, { headers, data: config });
  expect(response.ok(), await response.text()).toBeTruthy();
  headers = await login(trusted);
  response = await request.put(path, { headers, data: { ...config, owner_only_config: false } });
  expect(response.status()).toBe(422);
  const restore = await request.post(`${path}/events/1/restore`, { headers, data: { confirm: false } });
  expect(restore.status()).toBe(422);
});
