// The public site in a real browser: the landing page, /commands, the feature and legal pages, the tags a
// crawler reads, and screenshots of every page on a desktop, a tablet and a phone. Set SCREENSHOT_DIR to keep
// the screenshots (they are only taken then).

import { mkdirSync } from "node:fs";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow } from "./helpers";

const PAGES = [
  { name: "landing", path: "/" },
  { name: "commands", path: "/commands" },
  { name: "feature-house-and-pets", path: "/features/house-and-pets" },
  { name: "feature-games", path: "/features/games" },
  { name: "feature-profile-cards", path: "/features/profile-cards" },
  { name: "feature-leveling-and-economy", path: "/features/leveling-and-economy" },
  { name: "feature-server-tools", path: "/features/server-tools" },
  { name: "feature-web-dashboard", path: "/features/web-dashboard" },
  { name: "privacy", path: "/privacy" },
  { name: "terms", path: "/terms" },
  { name: "not-found", path: "/no-such-page" },
];

const VIEWPORTS = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet", width: 820, height: 1180 },
  { name: "phone", width: 390, height: 844 },
];

const screens = process.env.SCREENSHOT_DIR;

/** Loads `path` and waits until fonts and the pictures in view are in, so a screenshot shows the finished page. */
async function open(page: Page, path: string) {
  await page.goto(path);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForFunction(() =>
    [...document.images]
      .filter((i) => i.checkVisibility() && i.getBoundingClientRect().top < window.innerHeight)
      .every((i) => i.complete),
  );
}

test.describe("screenshots", () => {
  test.skip(!screens, "set SCREENSHOT_DIR to take them");
  test.use({ contextOptions: { reducedMotion: "reduce" } });

  for (const viewport of VIEWPORTS) {
    for (const { name, path } of PAGES) {
      test(`${name} on a ${viewport.name}`, async ({ page }) => {
        mkdirSync(screens!, { recursive: true });
        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        // The Discord CDN is not reachable in a test run: a plain tile stands in for any avatar.
        await open(page, path);
        // Lazy pictures below the fold load as the page is scrolled through once.
        await page.evaluate(async () => {
          for (let y = 0; y < document.body.scrollHeight; y += 600) {
            window.scrollTo(0, y);
            await new Promise((r) => setTimeout(r, 40));
          }
          window.scrollTo(0, 0);
        });
        await page.waitForFunction(() =>
          [...document.images].filter((i) => i.checkVisibility()).every((i) => i.complete),
        );
        expect(await overflow(page), "no sideways scroll").toBeLessThanOrEqual(0);
        await page.screenshot({
          path: `${screens}/${name}-${viewport.name}.png`,
          fullPage: true,
          animations: "disabled",
        });
      });
    }
  }
});

test("the front page offers the bot and the dashboard", async ({ page }) => {
  await open(page, "/");
  await expect(page).toHaveTitle(/^Pinchora: a cozy pixel world/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Make your server a cozy pixel world.");

  const add = page.getByRole("link", { name: "Add to Discord" }).first();
  await expect(add).toHaveAttribute("href", "/invite");
  const dashboard = page.getByRole("link", { name: "Open dashboard" }).first();
  await expect(dashboard).toHaveAttribute("href", "/servers");
  await expect(page.getByRole("link", { name: "Log in" })).toBeVisible();

  // The invite goes to Discord's page for adding a bot, with the commands scope and permissions.
  const invite = await page.request.get("/invite", { maxRedirects: 0 });
  expect(invite.status()).toBe(307);
  const location = new URL(invite.headers()["location"]!);
  expect(location.pathname).toBe("/oauth2/authorize");
  expect(location.searchParams.get("scope")).toBe("bot applications.commands");
  expect(Number(location.searchParams.get("permissions"))).toBeGreaterThan(0);

  // Six areas, each with its own page.
  const grid = page.locator(".feature-grid");
  await expect(grid.getByRole("link")).toHaveCount(6);
  await grid.getByRole("link", { name: /Games/ }).click();
  await expect(page).toHaveURL(/\/features\/games$/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Games");
  await expect(page).toHaveTitle(/Discord games/);
});

test("the numbers come from the server and the cards from the bot", async ({ page }) => {
  const seen: string[] = [];
  page.on("response", (r) => seen.push(new URL(r.url()).pathname));
  await open(page, "/");
  const stats = page.getByRole("region", { name: "Pinchora in numbers" });
  await expect(stats.getByText("SERVERS")).toBeVisible();
  await expect(stats.getByText("HOUSES BUILT")).toBeVisible();
  expect(seen).toContain("/api/public/landing");
  // Every picture has words for people who cannot see it, and the ones on screen have loaded (the rest are
  // lazy: below the fold, or on a tab nobody has opened yet).
  await page.evaluate(async () => {
    for (let y = 0; y < document.body.scrollHeight; y += 500) {
      window.scrollTo(0, y);
      await new Promise((r) => setTimeout(r, 30));
    }
  });
  const images = await page.locator("main img").evaluateAll((list) =>
    (list as HTMLImageElement[]).map((i) => ({
      src: i.currentSrc || i.src,
      alt: i.alt,
      shown: i.checkVisibility(),
      decorative: i.closest("[aria-hidden='true']") !== null,
      width: i.naturalWidth,
    })),
  );
  expect(images.length).toBeGreaterThan(5);
  expect(images.filter((i) => i.shown).length).toBeGreaterThan(3);
  for (const image of images) {
    if (!image.decorative) expect(image.alt.length, image.src).toBeGreaterThan(20);
    if (image.shown) expect(image.width, image.src).toBeGreaterThan(0);
  }
});

test("the gallery tabs show one card at a time and work from the keyboard", async ({ page }) => {
  await open(page, "/");
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(4);
  await expect(page.getByRole("tabpanel")).toHaveCount(1);
  await tabs.first().focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel")).toContainText("A room you decorated");
  await page.keyboard.press("End");
  await expect(tabs.nth(3)).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel").getByRole("img")).toBeVisible();
});

test("the questions open and close", async ({ page }) => {
  await open(page, "/");
  const first = page.locator(".faq-item").first();
  await expect(first).not.toHaveAttribute("open", "");
  await first.locator("summary").click();
  await expect(first).toHaveAttribute("open", "");
  await expect(first).toContainText("Manage Roles");
});

test("the commands page searches, filters and links", async ({ page }) => {
  await open(page, "/commands");
  await expect(page).toHaveTitle("Commands | Pinchora");
  const total = await page.locator(".cmd").count();
  expect(total).toBeGreaterThan(100);

  await page.getByRole("searchbox", { name: "Search commands" }).fill("fish cast");
  await expect(page).toHaveURL(/\?q=fish\+cast$/);
  await expect(page.locator(".cmd")).toHaveCount(1);
  await expect(page.locator(".cmd-path")).toHaveText("/fish cast");

  await page.getByRole("searchbox").fill("zzzzzz");
  await expect(page.getByText("NOTHING WASHED UP")).toBeVisible();
  await page.getByRole("button", { name: "Show every command" }).click();
  await expect(page.locator(".cmd")).toHaveCount(total);

  await page.getByLabel("Only for server managers").check();
  await expect(page).toHaveURL(/managers=1/);
  const managers = await page.locator(".cmd").count();
  expect(managers).toBeGreaterThan(10);
  expect(managers).toBeLessThan(total);
  await expect(page.locator(".cmd-manager")).toHaveCount(managers);

  // A link from a feature page lands on the same search.
  await open(page, "/commands?q=welcome");
  await expect(page.getByRole("searchbox")).toHaveValue("welcome");
  await expect(page.locator(".cmd-path", { hasText: "/config welcome" }).first()).toBeVisible();

  // "/" jumps to the search.
  await page.getByRole("heading", { level: 1 }).click();
  await page.keyboard.press("/");
  await expect(page.getByRole("searchbox")).toBeFocused();
});

test("a feature page lists what to try and links to the search", async ({ page }) => {
  await open(page, "/features/house-and-pets");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("House & pets");
  await page.getByRole("link", { name: "/pet adopt" }).click();
  await expect(page).toHaveURL(/\/commands\?q=pet%20adopt$/);
  await expect(page.locator(".cmd-path", { hasText: "/pet adopt" }).first()).toBeVisible();
});

test("an unknown address says so and stays out of search results", async ({ page }) => {
  await open(page, "/no-such-page");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Lost at sea");
  await expect(page).toHaveTitle("Page not found | Pinchora");
  await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, nofollow");
  await expect(page.getByRole("link", { name: "Back to the shore" })).toBeVisible();
  await page.goto("/features/not-an-area");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Lost at sea");
});

test("a logged-in visitor sees the dashboard in the header", async ({ page }) => {
  await devLogin(
    page,
    `${Date.now()}77`,
    [{ id: "1", name: "Reef", owner: true, permissions: "0" }],
    "marlowe",
  );
  await open(page, "/");
  const dashboard = page.locator(".site-nav").getByRole("link", { name: "Dashboard" });
  await expect(dashboard).toBeVisible();
  await expect(dashboard).toHaveAttribute("href", "/servers");
  await expect(page.getByRole("link", { name: "Log in" })).toHaveCount(0);
  // The dashboard keeps its own pages and its own tab title.
  await page.goto("/servers");
  await expect(page).toHaveTitle("Pinchora dashboard");
});

test("crawlers get each page's own tags without running a script", async ({ request, baseURL }) => {
  const origin = baseURL!.replace(/\/$/, "");
  for (const [path, title] of [
    ["/", "Pinchora: a cozy pixel world for your Discord server"],
    ["/commands", "Commands | Pinchora"],
    ["/features/games", "Discord games: chess, wordle, party games | Pinchora"],
    ["/privacy", "Privacy policy | Pinchora"],
    ["/terms", "Terms of service | Pinchora"],
  ] as const) {
    const html = await (await request.get(path)).text();
    expect(html, path).toContain(`<title>${title}</title>`);
    expect(html, path).toContain(`<link rel="canonical" href="${origin}${path}" />`);
    expect(html, path).toContain(`property="og:image" content="${origin}/og.png"`);
    expect(html, path).toContain('name="twitter:card" content="summary_large_image"');
    expect(html, path).not.toContain("__ORIGIN__");
  }
  const robots = await (await request.get("/robots.txt")).text();
  expect(robots).toContain(`Sitemap: ${origin}/sitemap.xml`);
  expect(robots).toContain("Disallow: /api/");
  const sitemap = await (await request.get("/sitemap.xml")).text();
  expect(sitemap).toContain(`<loc>${origin}/commands</loc>`);
  expect(sitemap).not.toContain("/servers");

  const og = await request.get("/og.png");
  expect(og.headers()["content-type"]).toBe("image/png");
  const bytes = await og.body();
  expect([bytes.readUInt32BE(16), bytes.readUInt32BE(20)]).toEqual([1200, 630]);
  for (const icon of [
    "/favicon.ico",
    "/favicon.svg",
    "/favicon-32x32.png",
    "/apple-touch-icon.png",
    "/site.webmanifest",
  ]) {
    expect((await request.get(icon)).ok(), icon).toBeTruthy();
  }
  // The dashboard's shell is kept out of search results.
  expect((await request.get("/servers")).headers()["x-robots-tag"]).toBe("noindex, nofollow");
});

test("no page scrolls sideways on a phone and each has one main heading", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 740 });
  for (const { path } of PAGES) {
    await open(page, path);
    expect(await overflow(page), `${path} scrolls sideways`).toBeLessThanOrEqual(0);
    await expect(page.getByRole("heading", { level: 1 }), path).toHaveCount(1);
    await expect(page.getByRole("main"), path).toHaveCount(1);
  }
  // The menu opens and closes on a phone.
  await open(page, "/");
  const toggle = page.getByRole("button", { name: "Open menu" });
  await toggle.click();
  await expect(
    page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Commands" }),
  ).toBeVisible();
  await page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Commands" }).click();
  await expect(page).toHaveURL(/\/commands$/);
  await expect(page.getByRole("button", { name: "Open menu" })).toHaveAttribute("aria-expanded", "false");
});

test("motion is off for people who ask for less", async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: "reduce" });
  const page = await context.newPage();
  await open(page, "/");
  const states = await page.evaluate(() => ({
    hero: getComputedStyle(document.querySelector(".hero-room")!).animationName,
    wave: getComputedStyle(document.querySelector(".sea-wave-mid")!).animationName,
    crab: getComputedStyle(document.querySelector(".sea-crab")!).animationName,
    reveal: getComputedStyle(document.querySelector(".reveal")!).opacity,
  }));
  expect(states).toEqual({ hero: "none", wave: "none", crab: "none", reveal: "1" });
  await context.close();
});

test("the page keeps its shape while pictures load", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => document.fonts.ready);
  const shifts = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        let total = 0;
        new PerformanceObserver((list) => {
          for (const entry of list.getEntries() as unknown as { value: number; hadRecentInput: boolean }[]) {
            if (!entry.hadRecentInput) total += entry.value;
          }
        }).observe({ type: "layout-shift", buffered: true });
        setTimeout(() => resolve(total), 1200);
      }),
  );
  expect(shifts, "cumulative layout shift").toBeLessThan(0.1);
});
