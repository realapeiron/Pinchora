import { expect, test, type Page } from "@playwright/test";
import { devLogin, testSql } from "./helpers";

const run = Date.now();
const mock = `http://127.0.0.1:${process.env.E2E_MOCK_PORT ?? 18191}`;

async function setup(page: Page, suffix: number, channel: number) {
  const guild = `${run}${suffix}`;
  testSql(`INSERT INTO guilds (id, welcome_enabled, welcome_channel_id) VALUES (${guild}, true, ${channel})`);
  await devLogin(page, `${run}${suffix + 20}`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "8" },
  ]);
  return guild;
}

test("Welcome warns on a blocked channel, sends the current draft, and explains a refusal", async ({
  page,
}) => {
  const guild = await setup(page, 51, 7010);
  await page.goto(`/g/${guild}/welcome`);
  const channel = page.getByLabel("Channel", { exact: true }).first();
  await channel.selectOption({ label: "#announcements" });
  await expect(page.getByText("Pinchora can't post in this channel:")).toBeVisible();
  await expect(channel).toHaveAttribute("aria-describedby", /.+/);
  await channel.selectOption({ label: "#welcome" });
  await expect(page.locator(".channel-post-warning")).toHaveCount(0);
  await page.getByLabel("Text above the card").fill(`Draft ${guild} for {user}`);
  const preview = page.locator(".settings-preview").filter({ hasText: "Welcome preview" });
  await preview.getByRole("button", { name: "Send test", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Test message sent to #welcome" })).toBeVisible();
  const posts = await (await page.request.get(`${mock}/mock/posts`)).json();
  expect(
    posts.items.some(
      (item: { channel: string; body: string }) =>
        item.channel === "7010" && item.body.includes(`Draft ${guild} for @`),
    ),
  ).toBe(true);

  await channel.selectOption({ label: "#memes" });
  await preview.getByRole("button", { name: "Send test", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Discord refused the test message" }),
  ).toBeVisible();
});

test("overview reports a configured channel Pinchora cannot post in", async ({ page }) => {
  const guild = await setup(page, 52, 7011);
  await page.goto(`/g/${guild}`);
  await expect(page.locator(".overview-attention")).toContainText("Pinchora can't post in #announcements");
  await expect(page.locator(".overview-attention")).toContainText("Send Messages");
  await expect(page.locator(".overview-attention")).toContainText("channel settings");
});
