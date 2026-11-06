// Social feeds in a real browser: follow a YouTube channel by its @handle (checked live), a podcast and a
// subreddit; watch the bot's own preview; see the poller's health on the list (one source failing); send a
// test post; edit; delete; and the phone layout. Every feed address is served by a small server in this
// file: the debug server sends every feed request to it (PINCHORA_FEEDS__DEV_ENDPOINT), never to the internet.

import http from "node:http";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const feedPort = Number(process.env.E2E_FEED_PORT ?? 18193);
const run = Date.now();
const guild = `${run}7`;
const user = `${run}9`;
const channelId = `UC${String(run).padStart(22, "0")}`.slice(0, 24);
const handle = `reefcrew${run % 100000}`;
const podcastHost = `radio${run}.example`;
const subreddit = `reef${run % 100000}`;

const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000);

/** What the feed server answers, by `/<host><path>?<query>`. */
const pages = new Map<string, { status: number; body: string }>([
  [
    `/www.youtube.com/@${handle}`,
    {
      status: 200,
      body: `<html><head><link rel="canonical" href="https://www.youtube.com/channel/${channelId}"></head></html>`,
    },
  ],
  [
    `/www.youtube.com/feeds/videos.xml?channel_id=${channelId}`,
    {
      status: 200,
      body: `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
<title>Reef Crew</title>
<entry><id>yt:video:abc123XYZ90</id><title>We built a lighthouse out of shells</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=abc123XYZ90"/><author><name>Reef Crew</name></author>
<published>${hoursAgo(5).toISOString()}</published>
<media:group><media:content url="https://www.youtube.com/v/abc123XYZ90" type="application/x-shockwave-flash"/>
<media:thumbnail url="https://i2.ytimg.com/vi/abc123XYZ90/hqdefault.jpg"/></media:group></entry>
</feed>`,
    },
  ],
  [
    `/${podcastHost}/feed.xml`,
    {
      status: 200,
      body: `<?xml version="1.0"?><rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"><channel>
<title>Tide Pool Radio</title><item><title>Episode 41: Hermit crab housing market</title>
<link>https://${podcastHost}/41</link><guid>tide-41</guid><pubDate>${hoursAgo(26).toUTCString()}</pubDate>
<description>&lt;p&gt;Why shells are so expensive this year.&lt;/p&gt;</description>
<enclosure url="https://cdn.${podcastHost}/41.mp3" length="1" type="audio/mpeg"/><itunes:duration>38:12</itunes:duration>
</item></channel></rss>`,
    },
  ],
  [
    `/www.reddit.com/r/${subreddit}/new/.rss`,
    {
      status: 200,
      body: `<?xml version="1.0" encoding="UTF-8"?><feed xmlns="http://www.w3.org/2005/Atom"><title>newest submissions</title>
<entry><author><name>/u/ferris</name></author><id>t3_e2e1</id><link href="https://www.reddit.com/r/${subreddit}/comments/e2e1/"/>
<published>${hoursAgo(1).toISOString()}</published><updated>${hoursAgo(1).toISOString()}</updated><title>Look at this crab</title></entry></feed>`,
    },
  ],
]);

let server: http.Server;

test.beforeAll(async () => {
  server = http.createServer((req, res) => {
    const page = pages.get(req.url ?? "");
    res.writeHead(page?.status ?? 404, { "content-type": "text/xml; charset=utf-8" });
    res.end(page?.body ?? "");
  });
  await new Promise<void>((done) => server.listen(feedPort, "127.0.0.1", done));
});

test.afterAll(async () => {
  await new Promise<void>((done) => server.close(() => done()));
});

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

/** A write to the dashboard's API as its own script sends it. */
async function write(page: Page, path: string, data: unknown) {
  const csrf = ((await (await page.request.get("/api/me")).json()) as { csrf: string }).csrf;
  const response = await page.request.fetch(path, {
    method: "POST",
    data,
    headers: { "x-csrf-token": csrf, origin: new URL(page.url()).origin },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** The whole page at the current width, as one tall viewport (sticky bars stay where they belong). */
async function tallShot(page: Page, name: string) {
  const width = page.viewportSize()?.width ?? 390;
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  await page.setViewportSize({ width, height });
  await shot(page, name);
  await page.setViewportSize({ width, height: 844 });
}

const row = (page: Page, name: string) => page.locator(`[data-feed="${name}"]`);

test("follow a YouTube channel by its handle, with the bot's own preview, then watch it on the list", async ({
  page,
}) => {
  test.setTimeout(150_000);
  await login(page);
  await page.goto(`/g/${guild}/feeds`);
  await expect(page.getByRole("heading", { name: "SOCIAL FEEDS", exact: true })).toBeVisible();
  await expect(page.getByText("Nothing followed yet")).toBeVisible();
  await expect(page.getByText("0/10 used")).toBeVisible();
  await expect(page.getByRole("link", { name: "Social feeds" })).toBeVisible();
  await shot(page, "feeds-empty");

  // The YouTube card opens the form with YouTube picked; the handle is looked up as it is typed.
  await page.getByRole("link", { name: /^YouTube/ }).click();
  await expect(page.getByRole("heading", { name: "ADD A FEED", exact: true })).toBeVisible();
  await expect(page.getByRole("radio", { name: /YouTube/ })).toBeChecked();
  await expect(page.getByRole("radio", { name: /TikTok/ })).toBeDisabled();
  await page.getByLabel("YouTube channel").fill("@nobody-at-all");
  await expect(page.getByText("couldn't find the channel")).toBeVisible({ timeout: 15_000 });
  await page.getByLabel("YouTube channel").fill(`@${handle}`);
  const found = page.getByTestId("feed-found");
  await expect(found).toContainText("Reef Crew", { timeout: 15_000 });
  await expect(found).toContainText("We built a lighthouse out of shells");

  await page.locator("#feed-channel").selectOption("7011");
  await page.locator("#feed-role").selectOption("9001");
  await page.getByLabel("Text", { exact: true }).fill("New from ");
  await page.getByRole("button", { name: "{account}", exact: true }).click();
  await page.getByLabel("Text", { exact: true }).press("End");
  await page.getByLabel("Text", { exact: true }).pressSequentially(": ");
  await page.getByRole("button", { name: "{title}", exact: true }).click();
  await page.getByLabel("Only titles with").fill("lighthouse, Shells, shells");
  await expect(page.locator(".feed-word")).toHaveCount(2);
  const live = page.locator(".preview-live");
  await expect(live).not.toHaveClass(/is-busy/, { timeout: 15_000 });
  await expect(live).toContainText("@Fan New from Reef Crew: We built a lighthouse out of shells");
  await expect(page.locator(".preview-notes")).toContainText("Pings @Fan");
  await tallShot(page, "feeds-editor");
  await page.setViewportSize({ width: 1280, height: 800 });

  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("heading", { name: "SOCIAL FEEDS", exact: true })).toBeVisible();
  await expect(page.locator(".toast")).toContainText("Following Reef Crew");
  await expect(row(page, "Reef Crew")).toContainText("announcements");
  await expect(row(page, "Reef Crew")).toContainText("Waiting for its first check");

  // Two more, through the API: a podcast and a subreddit that will start failing.
  await write(page, `/api/guilds/${guild}/feeds`, {
    kind: "rss",
    source: `https://${podcastHost}/feed.xml`,
    channel_id: "7001",
  });
  await write(page, `/api/guilds/${guild}/feeds`, {
    kind: "reddit",
    source: `r/${subreddit}`,
    channel_id: "7002",
    filter_exclude: ["nsfw"],
  });
  pages.set(`/www.reddit.com/r/${subreddit}/new/.rss`, { status: 503, body: "down" });

  // The poller (every 30 s) checks them: two healthy, one failing with the reason.
  await expect
    .poll(
      async () => {
        await page.reload();
        await row(page, "Reef Crew").waitFor();
        return (
          (await page.locator(".feed-health.is-ok").count()) >= 2 &&
          (await page.locator(".feed-health.is-failing").count()) === 1
        );
      },
      { timeout: 75_000, intervals: [5_000] },
    )
    .toBe(true);
  await expect(row(page, "r/" + subreddit)).toContainText("HTTP 503");
  await expect(page.getByText("3/10 used")).toBeVisible();
  await shot(page, "feeds-list");

  // A test post from the list, then the editor of an existing feed.
  await row(page, "Tide Pool Radio")
    .getByRole("button", { name: /test post/i })
    .click();
  await expect(page.locator(".toast")).toContainText("Posted Tide Pool Radio's latest item in #general");
  await row(page, "Reef Crew").getByRole("link", { name: "Edit" }).click();
  await expect(page.getByRole("heading", { name: "EDIT FEED", exact: true })).toBeVisible();
  await expect(page.locator(".feed-source-read")).toContainText("Reef Crew");
  await expect(page.locator(".preview-live")).toContainText("We built a lighthouse", { timeout: 15_000 });
  await page.getByRole("button", { name: "Send a test post" }).click();
  await expect(page.getByText("Posted the latest item in #announcements.")).toBeVisible();
  await page.getByRole("switch", { name: "Pinchora's card" }).click();
  await expect(page.locator(".preview-live .embed-card")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: "Send a test post" })).toBeDisabled();
  await tallShot(page, "feeds-edit");
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.locator(".toast")).toContainText("Saved Reef Crew");

  // Phone: no sideways scroll on the list or the form.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(row(page, "Reef Crew")).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await tallShot(page, "feeds-list-phone");
  await page.goto(`/g/${guild}/feeds/new?kind=rss`);
  await expect(page.getByRole("radio", { name: /RSS & podcasts/ })).toBeChecked();
  await page.getByLabel("Feed address").fill(`${podcastHost}/feed.xml`);
  await expect(page.getByTestId("feed-found")).toContainText("Tide Pool Radio", { timeout: 15_000 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await expect(page.locator(".preview-live")).toContainText("Hermit crab housing market", {
    timeout: 15_000,
  });
  await tallShot(page, "feeds-editor-phone");

  // Back on a desktop: stop following the failing one.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto(`/g/${guild}/feeds`);
  await row(page, "r/" + subreddit)
    .getByRole("button", { name: /^Delete/ })
    .click();
  await page.getByRole("button", { name: "Stop following" }).click();
  await expect(page.locator(".toast")).toContainText(`Stopped following r/${subreddit}`);
  await expect(page.getByText("2/10 used")).toBeVisible();
});
