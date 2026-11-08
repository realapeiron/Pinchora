import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}4`;
const user = `${stamp}5`;

test("community pack settings and forms work on desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/community-pack`);
  await expect(page.getByRole("heading", { name: "COMMUNITY PACK" })).toBeVisible();
  await expect(page.getByText("NEEDS MESSAGE CONTENT")).toBeVisible();
  await page.getByLabel("Post channel").selectOption({ label: "#general" });
  // Hold the QOTD refetch so a counting edit definitely happens during the other section's save.
  let releaseRefresh!: () => void;
  let markRefreshStarted!: () => void;
  const refreshGate = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  const refreshStarted = new Promise<void>((resolve) => {
    markRefreshStarted = resolve;
  });
  const packPath = `/api/guilds/${guild}/community-pack`;
  await page.route(`**${packPath}`, async (route) => {
    markRefreshStarted();
    await refreshGate;
    await route.continue();
  });
  await page.getByRole("button", { name: "Save QOTD" }).click();
  await refreshStarted;
  try {
    await expect(page.getByText("Question queue")).toBeVisible();
    await page.getByLabel("Counting channel").selectOption({ label: "#general" });
  } finally {
    releaseRefresh();
  }
  await expect(page.getByRole("button", { name: "Save counting" })).toBeEnabled();
  await expect(page.getByLabel("Counting channel")).toHaveValue("7001");
  await page.unroute(`**${packPath}`);
  const countingSaved = page.waitForResponse(
    (response) => response.url().endsWith(`${packPath}/counting`) && response.request().method() === "PUT",
  );
  await page.getByRole("button", { name: "Save counting" }).click();
  expect((await countingSaved).ok()).toBeTruthy();
  const saved = await (await page.request.get(packPath)).json();
  expect(saved.counting.channel_id).toBe("7001");
  expect(saved.qotd.channel_id).toBe("7001");
  await page.getByRole("button", { name: "New form" }).click();
  await page.getByLabel("Form name").fill("Helper");
  await page.getByLabel("Question 1").fill("Why would you like to help?");
  await page.getByLabel("Review channel").selectOption({ label: "#general" });
  await page.getByRole("button", { name: "Save form" }).click();
  await expect(page.getByText("Helper")).toBeVisible();
  await shot(page, "community-pack-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "community-pack-phone", true);
});
