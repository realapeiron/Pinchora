// Server stats in a real browser: the dashboard's Stats page busy (its numbers from a fixture, as no
// test can chat on Discord) and empty (straight from the server), a counter channel added through the
// form, the public switch, the public server page and the bot-wide /stats page, on desktop and phone.
// Screenshots land in test-results/screens/stats-*.png.

import { type Page, expect, test } from "@playwright/test";

import { devLogin, openRow, overflow, shot } from "./helpers";

const run = Date.now();
const busy = `${run}21`;
const quiet = `${run}23`;
const user = `${run}26`;

async function login(page: Page) {
  await devLogin(page, user, [
    { id: busy, name: "Pinchora Reef", owner: true, permissions: "0" },
    { id: quiet, name: "Quiet Cove", owner: true, permissions: "0" },
  ]);
}

/** `days` days ending today (UTC), oldest first, as `YYYY-MM-DD`. */
function lastDays(days: number): string[] {
  const today = Date.UTC(new Date().getUTCFullYear(), new Date().getUTCMonth(), new Date().getUTCDate());
  return Array.from({ length: days }, (_, i) =>
    new Date(today - (days - 1 - i) * 86_400_000).toISOString().slice(0, 10),
  );
}

/** A month in a lively server: weekend peaks, evenings busiest, a few people leaving. */
function period(days: number) {
  const daily = lastDays(days).map((day, i) => {
    const weekend = [0, 6].includes(new Date(`${day}T00:00:00Z`).getUTCDay());
    const messages = 180 + ((i * 37) % 23) * 11 + (weekend ? 240 : 0);
    return {
      day,
      messages,
      voice_minutes: 300 + ((i * 53) % 17) * 40 + (weekend ? 600 : 0),
      joins: (i * 5) % 7,
      leaves: i % 4 === 0 ? 2 : i % 5 === 0 ? 1 : 0,
      commands: Math.round(messages / 6),
    };
  });
  const sum = (key: "messages" | "voice_minutes" | "joins" | "leaves" | "commands") =>
    daily.reduce((a, d) => a + d[key], 0);
  const heatmap = Array.from({ length: 7 }, (_, d) =>
    Array.from({ length: 24 }, (_, h) =>
      h >= 8 && h <= 23 ? Math.round(((h - 7) * (25 - h) * (d >= 5 ? 1.6 : 1)) / 2) : h % 3 === 0 ? 2 : 0,
    ),
  );
  return {
    days,
    from: daily[0]!.day,
    to: daily[daily.length - 1]!.day,
    totals: {
      messages: sum("messages"),
      voice_minutes: sum("voice_minutes"),
      joins: sum("joins"),
      leaves: sum("leaves"),
      commands: sum("commands"),
      active_members: 214,
    },
    previous:
      days === 90
        ? null
        : {
            messages: Math.round(sum("messages") * 0.86),
            voice_minutes: Math.round(sum("voice_minutes") * 1.12),
            joins: sum("joins") - 6,
            leaves: sum("leaves") + 3,
            commands: sum("commands"),
            active_members: null,
          },
    daily,
    heatmap,
    top_channels: [
      { id: "7001", name: "general", messages: 4210, voice_minutes: 0 },
      { id: "7002", name: "memes", messages: 1882, voice_minutes: 0 },
      { id: "7011", name: "announcements", messages: 610, voice_minutes: 0 },
      { id: "7003", name: "Lounge", messages: 402, voice_minutes: 9120 },
      { id: "7010", name: "welcome", messages: 233, voice_minutes: 0 },
    ],
  };
}

const MEMBERS = [
  ["Coral", 912],
  ["Bubbles the Very Long Named Hermit Crab", 804],
  ["Pinch", 512],
  ["Kelpie", 377],
  ["Shelly", 120],
].map(([name, messages], i) => ({
  id: String(9100 + i),
  name,
  avatar_url: `https://cdn.discordapp.com/embed/avatars/${i}.png`,
  messages,
  voice_minutes: 0,
}));

async function serveBusy(page: Page) {
  await page.route(new RegExp(`/api/guilds/${busy}/stats\\?days=(\\d+)$`), (route) => {
    const days = Number(new URL(route.request().url()).searchParams.get("days"));
    return route.fulfill({
      json: {
        ...period(days),
        top_members: MEMBERS,
        public: false,
        voice_counted: true,
        module_on: true,
      },
    });
  });
}

test("the Stats page draws a busy server, reads out a day and switches periods", async ({ page }) => {
  await login(page);
  await serveBusy(page);
  await page.goto(`/g/${busy}/stats`);
  await expect(page.getByRole("heading", { name: "STATS", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "30 days" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".stat-tile").first()).toContainText("MESSAGES");
  await expect(page.getByText("214 members took part")).toBeVisible();
  await expect(page.locator(".rank").first()).toContainText("#general");
  await expect(page.locator(".rank").nth(1)).toContainText("Coral");
  await shot(page, "stats-desktop", true);

  // Hovering a column reads that day out; the keyboard does too.
  const chart = page.locator(".chart svg").first();
  const box = (await chart.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.7, box.y + box.height * 0.6);
  await expect(page.locator(".chart-readout").first()).toContainText("messages");
  await shot(page, "stats-hover");
  await page.mouse.move(0, 0);
  await chart.focus();
  await page.keyboard.press("End");
  await expect(page.locator(".chart-readout").first()).toBeVisible();
  await page.locator(".heat-grid").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(".heat-readout")).toContainText("UTC");

  // The numbers are there as a table too.
  await page.getByText("Show the numbers").first().click();
  await expect(page.getByRole("table", { name: "Each day's numbers" })).toBeVisible();

  await page.getByRole("button", { name: "7 days" }).click();
  await expect(page.getByRole("button", { name: "7 days" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".chart svg").first()).toHaveAttribute("aria-label", /messages per day/);
  await page.getByRole("button", { name: "90 days" }).click();
  await expect(page.locator(".stat-tile-delta")).toHaveCount(0);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "30 days" }).click();
  await expect(page.locator(".stat-tile").first()).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "stats-phone", true);
});

test("an empty server, a counter channel and the public page", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${quiet}/stats`);
  await expect(page.getByRole("heading", { name: "STATS", exact: true })).toBeVisible();
  await expect(page.locator(".stat-tile").first()).toContainText("0");
  await expect(page.getByText("Nothing counted in these days yet.").first()).toBeVisible();
  await expect(page.getByText("No counter channels yet.")).toBeVisible();

  // A counter: the Lounge voice channel shows the member count.
  await page.getByLabel("Channel", { exact: true }).selectOption({ label: "🔊 Lounge" });
  await page.getByLabel("Counts").selectOption({ label: "Members" });
  await page.getByLabel("Name").fill("Crabs: {count}");
  await page.getByRole("button", { name: "Preview name" }).click();
  await expect(page.locator(".counter-preview [role=status]")).toBeVisible();
  await page.getByRole("button", { name: "Add counter" }).click();
  await expect(page.locator(".counter-row")).toHaveCount(1);
  await expect(page.locator(".counter-row")).toContainText("Lounge");
  const countersSaved = await (await page.request.get(`/api/guilds/${quiet}/stats/counters`)).json();
  expect(countersSaved.counters).toEqual(
    expect.arrayContaining([expect.objectContaining({ template: "Crabs: {count}" })]),
  );
  await expect(page.getByText("1 of 10 counter channels used")).toBeVisible();
  await page.getByLabel("Name").fill("Crabs");
  await expect(page.getByText("Put {count} where the number goes.")).toBeVisible();
  await shot(page, "stats-empty", true);

  // Public: the switch opens the page to anyone with the link.
  expect((await page.request.get(`/api/public/stats/${quiet}`)).status()).toBe(404);
  await page.getByRole("switch", { name: "Public stats page" }).click();
  await expect(page.getByRole("switch", { name: "Public stats page" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await openRow(page.locator(".settings-detail-expand").filter({ hasText: "Public stats page" }));
  await expect(page.getByRole("link", { name: "Open the public page" })).toBeVisible();
  expect((await page.request.get(`/api/public/stats/${quiet}`)).status()).toBe(200);

  await openRow(page.locator(".settings-detail-expand").filter({ hasText: "Preview the /stats card" }));
  await page.getByRole("button", { name: "Preview the /stats card", exact: true }).click();
  const card = page.locator(".card-preview img");
  await expect(card).toBeVisible();
  await expect
    .poll(() => card.evaluate((img: HTMLImageElement) => img.naturalWidth), { timeout: 60_000 })
    .toBe(900);

  await page.getByRole("button", { name: /Stop counting in/ }).click();
  await expect(page.locator(".counter-row")).toHaveCount(0);
  const countersRemoved = await (await page.request.get(`/api/guilds/${quiet}/stats/counters`)).json();
  expect(countersRemoved.counters).toHaveLength(0);
});

test("the public pages: a server that opened its stats, one that did not, and the bot's totals", async ({
  page,
}) => {
  await page.route(`**/api/public/stats/${busy}`, (route) =>
    route.fulfill({
      json: { ...period(30), guild: { id: busy, name: "Pinchora Reef", icon_url: null } },
    }),
  );
  await page.goto(`/stats/${busy}`);
  await expect(page.getByRole("heading", { name: "Pinchora Reef" })).toBeVisible();
  await expect(page.getByText("TOP MEMBERS")).toHaveCount(0);
  await shot(page, "stats-public-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "stats-public-phone", true);

  await page.goto(`/stats/${run}99`);
  await expect(page.getByRole("heading", { name: "These stats are private" })).toBeVisible();

  const totals = await page.request.get("/api/public/stats");
  expect(totals.ok()).toBeTruthy();
  const history = lastDays(30).map((day, i) => ({
    day,
    servers: 1200 + i * 9,
    players: 48_000 + i * 310,
    messages: 90_000 + ((i * 41) % 19) * 2_600,
  }));
  await page.route("**/api/public/stats", async (route) => {
    const real = await route.fetch();
    const body = await real.json();
    return route.fulfill({ json: { ...body, history } });
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/stats");
  await expect(page.getByRole("heading", { name: "Pinchora in numbers" })).toBeVisible();
  await expect(page.locator(".stat-tile")).toHaveCount(6);
  await shot(page, "stats-global-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "stats-global-phone", true);
});
