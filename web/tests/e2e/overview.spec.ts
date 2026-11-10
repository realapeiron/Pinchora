import { expect, test, type Page } from "@playwright/test";
import { devLogin } from "./helpers";

const stamp = Date.now();
const stats = (days: number, on = true) => ({
  days,
  from: "2026-11-01",
  to: "2026-11-10",
  module_on: on,
  public: false,
  voice_counted: false,
  totals: { messages: 120, joins: 9, leaves: 2, commands: 5, active_members: 21, voice_minutes: 0 },
  previous: { messages: 100, joins: 4, leaves: 1, commands: 10, active_members: 20, voice_minutes: 0 },
  daily: Array.from({ length: days }, (_, i) => ({
    day: `2026-11-${String(i + 1).padStart(2, "0")}`,
    messages: i === days - 1 ? 120 : 20 + i,
    joins: 0,
    leaves: 0,
    commands: 0,
    voice_minutes: 0,
  })),
  heatmap: [],
  top_channels: [],
  top_members: [],
});
async function open(page: Page, width: number, empty = false) {
  const guild = `${stamp}${width}${empty ? 8 : 7}`;
  await page.setViewportSize({ width, height: 900 });
  await devLogin(page, `${stamp}${width}${empty ? 6 : 5}`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "8" },
  ]);
  await page.route(`**/api/guilds/${guild}/stats?days=*`, async (route) => {
    const days = Number(new URL(route.request().url()).searchParams.get("days"));
    await route.fulfill({
      json: empty
        ? {
            ...stats(days, false),
            totals: { ...stats(days, false).totals, active_members: null },
            daily: stats(days, false).daily.map((row) => ({ ...row, messages: 0 })),
          }
        : stats(days),
    });
  });
  await page.goto(`/g/${guild}`);
  await expect(page.getByRole("heading", { name: "Pinchora Reef" })).toBeVisible();
  return guild;
}

for (const width of [1440, 390]) {
  test(`overview shows real activity and catalog at ${width}px`, async ({ page }) => {
    const guild = await open(page, width);
    await expect(page.locator(".overview-stat")).toHaveCount(4);
    await expect(page.locator(".overview-stat").first()).toContainText("120");
    await expect(page.getByRole("table", { name: "Daily messages" }).getByRole("row")).toHaveCount(8);
    const chart = page.locator(".overview-chart");
    const box = await chart.boundingBox();
    expect(box).not.toBeNull();
    await page.mouse.move(box!.x + box!.width * 0.8, box!.y + 80);
    await expect(page.locator(".overview-chart-tip")).toBeVisible();
    await expect(page.locator(".overview-chart-tip")).toContainText("Nov");
    await page.getByRole("button", { name: "30 days" }).click();
    await expect(page.getByRole("table", { name: "Daily messages" }).getByRole("row")).toHaveCount(31);
    const catalog = page.getByRole("region", { name: "Module catalog" });
    await expect(catalog.locator(".overview-module")).toHaveCount(12);
    await catalog.getByRole("button", { name: "Show all 55" }).click();
    await expect(catalog.locator(".overview-module")).toHaveCount(55);
    const first = catalog.locator(".overview-module").first();
    const toggle = first.getByRole("switch");
    const before = await toggle.getAttribute("aria-checked");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", before === "true" ? "false" : "true");
    await page.getByRole("button", { name: "Undo" }).click();
    await expect(toggle).toHaveAttribute("aria-checked", before!);
    await catalog
      .getByRole("button", { name: /server/i })
      .first()
      .click();
    await expect(catalog.locator(".overview-module")).not.toHaveCount(0);
    await catalog.getByRole("checkbox", { name: "Only switched on" }).check();
    for (const item of await catalog.getByRole("switch").all()) {
      await expect(item).toHaveAttribute("aria-checked", "true");
    }
    await catalog.getByRole("searchbox", { name: "Search modules" }).fill("nonsense-unmatched");
    await expect(catalog.getByText("No modules match. Clear the filters and try again.")).toBeVisible();
    const api = await (await page.request.get(`/api/guilds/${guild}`)).json();
    expect(api.guild.name).toBe("Pinchora Reef");
  });
  test(`overview shows one stats empty state at ${width}px`, async ({ page }) => {
    const guild = await open(page, width, true);
    await expect(page.locator(".overview-stat")).toHaveCount(0);
    await expect(
      page.getByText("No activity has been counted yet. Check Stats to start tracking."),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: "Open Stats" })).toHaveAttribute("href", `/g/${guild}/stats`);
  });
}
