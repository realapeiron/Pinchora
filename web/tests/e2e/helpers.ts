// Shared steps for the Playwright runs: a dev login with a few servers, and screenshots for people
// (and agents) to look at in test-results/screens/.

import { type Cookie, type Locator, type Page, expect } from "@playwright/test";

/** A pixel avatar for every Discord CDN image, so screenshots do not depend on the network. */
const AVATAR = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8" shape-rendering="crispEdges">
<rect width="8" height="8" fill="#2e3d75"/><rect x="2" y="2" width="4" height="4" fill="#ffd166"/>
<rect x="3" y="3" width="1" height="1" fill="#1b1d2b"/><rect x="5" y="3" width="1" height="1" fill="#1b1d2b"/></svg>`;

export interface Guild {
  id: string;
  name: string;
  owner: boolean;
  permissions: string;
}

/** Reuse identical login fixtures within a worker; a restarted worker can log in again. */
const sessions = new Map<string, Cookie[]>();

/** Logs in as `user` with `guilds` and seeds English for tests with English locators. */
export async function devLogin(page: Page, user: string, guilds: Guild[], username = "captain_crab") {
  await page.route("https://cdn.discordapp.com/**", (route) =>
    route.fulfill({ contentType: "image/svg+xml", body: AVATAR }),
  );
  const sessionKey = JSON.stringify({ user, username, guilds });
  const known = sessions.get(sessionKey);
  let csrf: string | undefined;
  if (known) {
    await page.context().addCookies(known);
    const response = await page.request.get("/api/me");
    if (response.ok()) csrf = (await response.json()).csrf;
  }
  if (!csrf) {
    const response = await page.request.post("/auth/dev-login", {
      data: { user_id: user, username, guilds: guilds.map((g) => ({ ...g, icon: null })) },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    csrf = (await response.json()).csrf;
    sessions.set(sessionKey, await page.context().cookies());
  }
  // Account preferences survive browser contexts and suite runs (notably mock owner "1").
  // Reset through the real API on both fresh and cached sessions, before the app loads /api/me.
  const response = await page.request.post("/api/me/language", {
    headers: { "X-CSRF-Token": csrf! },
    data: { language: "en" },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

export async function shot(page: Page, name: string, fullPage = false) {
  await page.evaluate(() => document.fonts.ready);
  return page.screenshot({ path: `test-results/screens/${name}.png`, animations: "disabled", fullPage });
}

/** How far the page scrolls sideways; a phone layout must not. */
export function overflow(page: Page) {
  return page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
}

/** Open a settings row without toggling one already open in this session. */
export async function openRow(root: Page | Locator, name?: string | RegExp) {
  const header = name === undefined ? (root as Locator) : root.getByRole("button", { name });
  if ((await header.getAttribute("aria-expanded")) === "false") await header.click();
  await expect(header).toHaveAttribute("aria-expanded", "true");
}

/** Close a settings row without toggling one already closed in this session. */
export async function closeRow(root: Page | Locator, name?: string | RegExp) {
  const header = name === undefined ? (root as Locator) : root.getByRole("button", { name });
  if ((await header.getAttribute("aria-expanded")) === "true") await header.click();
  await expect(header).toHaveAttribute("aria-expanded", "false");
}

export async function switchPanelLanguage(page: Page, language: "EN" | "TR") {
  const name = language === "TR" ? "Türkçe" : "English";
  const playerButton = page.getByRole("button", { name: language, exact: true });
  await expect(page.locator(".shell-page").or(playerButton).first()).toBeVisible();
  if (await page.locator(".shell-page").count()) {
    if ((page.viewportSize()?.width ?? 1280) > 960) {
      await page.locator(".language-trigger").click();
      await page.getByRole("menuitemradio", { name: new RegExp(name) }).click();
    } else {
      await page.locator(".shell-top-server").click();
      await page.locator(".language-drawer").getByRole("button", { name }).click();
      await page.locator(".shell-close").click();
    }
  } else {
    await playerButton.click();
  }
}
