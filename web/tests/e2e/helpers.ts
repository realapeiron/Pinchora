// Shared steps for the Playwright runs: a dev login with a few servers, and screenshots for people
// (and agents) to look at in test-results/screens/.

import { type Cookie, type Page, expect } from "@playwright/test";

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

/** Logs in through `/auth/dev-login` as `user` with `guilds` on their server list. */
export async function devLogin(page: Page, user: string, guilds: Guild[], username = "captain_crab") {
  await page.route("https://cdn.discordapp.com/**", (route) =>
    route.fulfill({ contentType: "image/svg+xml", body: AVATAR }),
  );
  const sessionKey = JSON.stringify({ user, username, guilds });
  const known = sessions.get(sessionKey);
  if (known) {
    await page.context().addCookies(known);
    if ((await page.request.get("/api/me")).ok()) return;
  }
  const response = await page.request.post("/auth/dev-login", {
    data: { user_id: user, username, guilds: guilds.map((g) => ({ ...g, icon: null })) },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  sessions.set(sessionKey, await page.context().cookies());
}

export async function shot(page: Page, name: string, fullPage = false) {
  await page.evaluate(() => document.fonts.ready);
  return page.screenshot({ path: `test-results/screens/${name}.png`, animations: "disabled", fullPage });
}

/** How far the page scrolls sideways; a phone layout must not. */
export function overflow(page: Page) {
  return page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
}
