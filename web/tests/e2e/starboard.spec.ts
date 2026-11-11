import { expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}7`;
const user = `${stamp}8`;

test("boards and vote channels save and fit desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/starboard`);
  await expect(page.getByRole("heading", { name: "STARBOARD" })).toBeVisible();
  await expect(page.getByText("BOARDS · 0/3 used")).toBeVisible();
  await page.getByRole("button", { name: /New board/ }).click();
  await page.getByLabel("Name").fill("Best of Reef");
  await page.getByLabel("Board channel").selectOption({ label: "#general" });
  await page.getByLabel(/Threshold/).fill("2");
  await expect(page.getByRole("button", { name: "Save board" })).toBeEnabled();
  await page.getByRole("button", { name: "Save board" }).click();
  await expect(page.getByText("Best of Reef")).toBeVisible();
  await shot(page, "starboard-desktop", true);
  const saved = await (await page.request.get(`/api/guilds/${guild}/starboard`)).json();
  expect(saved.boards[0]).toMatchObject({ name: "Best of Reef", channel_id: "7001", threshold: 2 });
  await page.getByRole("button", { name: /^VOTE CHANNELS/ }).click();
  await page.getByRole("button", { name: "Add vote channel" }).click();
  await page.getByRole("combobox", { name: "Channel", exact: true }).selectOption({ label: "#general" });
  await page.getByRole("button", { name: "Save vote channel" }).click();
  await expect(page.getByRole("status")).toContainText("Vote channel saved.");
  const savedVote = await (await page.request.get(`/api/guilds/${guild}/starboard`)).json();
  expect(savedVote.votes).toContainEqual({
    channel_id: "7001",
    up_emoji: "⬆️",
    down_emoji: "⬇️",
    only_media: false,
    create_thread: false,
    enabled: true,
  });
  await expect(page.locator(".vote-row", { hasText: "#general" })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "starboard-phone", true);
});
