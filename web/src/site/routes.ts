// The pages of the public site: where they live, and what search results and link previews say about them.
// Data only (no React), because the build reads it to write each page's tags and the sitemap.

import { FEATURES } from "./features";

export const SITE_NAME = "Pinchora";
export const DASHBOARD_TITLE = "Pinchora dashboard";
/** What the build writes where the page needs the site's own address; the server swaps in the real one. */
export const ORIGIN = "__ORIGIN__";
export const OG_IMAGE = {
  path: "/og.png",
  width: 1200,
  height: 630,
  alt: "Pinchora crab logo above a pixel reef town at dusk. Build your reef. Play with your server.",
};

export interface PublicRoute {
  path: string;
  title: string;
  description: string;
  changefreq: "weekly" | "monthly" | "yearly";
  priority: number;
}

export const PUBLIC_ROUTES: PublicRoute[] = [
  {
    path: "/",
    title: "Pinchora: a cozy pixel world for your Discord server",
    description:
      "Decorate an isometric pixel house, raise pets, go fishing, play games and level up. Pinchora is a Discord bot with pixel-art cards and a web dashboard.",
    changefreq: "weekly",
    priority: 1,
  },
  {
    path: "/commands",
    title: "Commands | Pinchora",
    description:
      "Every Pinchora slash command by area, with what it does and what it takes. Search the list or show only the commands for server managers.",
    changefreq: "weekly",
    priority: 0.8,
  },
  {
    path: "/status",
    title: "Bot status | Pinchora",
    description:
      "Live Pinchora bot status across Discord, including server reach, players, uptime and gateway shard latency.",
    changefreq: "weekly",
    priority: 0.5,
  },
  {
    path: "/stats",
    title: "Pinchora in numbers | Pinchora",
    description:
      "How many servers, players, houses, pets and fish Pinchora has, and the messages it counts each day, brought up to date every few minutes.",
    changefreq: "weekly",
    priority: 0.5,
  },
  ...FEATURES.map((f) => ({
    path: `/features/${f.slug}`,
    title: f.metaTitle,
    description: f.metaDescription,
    changefreq: "monthly" as const,
    priority: 0.7,
  })),
  {
    path: "/privacy",
    title: "Privacy policy | Pinchora",
    description: "What Pinchora stores about players and servers, why, for how long, and how to delete it.",
    changefreq: "yearly",
    priority: 0.3,
  },
  {
    path: "/terms",
    title: "Terms of service | Pinchora",
    description: "The rules for using the Pinchora Discord bot, its website and its dashboard.",
    changefreq: "yearly",
    priority: 0.3,
  },
];

/** Pages of the dashboard: signed-in, and nothing a search engine should list. */
const APP_PREFIXES = ["/servers", "/login", "/g/", "/me/", "/house/"];

/** `/commands/` and `/commands` are the same page. */
function clean(pathname: string): string {
  return pathname.length > 1 ? pathname.replace(/\/+$/, "") : pathname;
}

export function publicRoute(pathname: string): PublicRoute | undefined {
  const path = clean(pathname);
  return PUBLIC_ROUTES.find((r) => r.path === path);
}

export function isAppPath(pathname: string): boolean {
  const path = clean(pathname);
  return APP_PREFIXES.some((p) => (p.endsWith("/") ? path.startsWith(p) : path === p));
}
