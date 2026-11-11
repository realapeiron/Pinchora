import { expect, test } from "@playwright/test";
import { countingChannelId } from "../discord-fixtures";
import { devLogin, overflow, shot } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}4`;
const user = `${stamp}5`;

test("community pack settings and forms work on desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/community-engagement`);
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
    await page.getByRole("button", { name: /COUNTING/ }).click();
    await expect(page.getByRole("button", { name: /COUNTING/ })).toHaveAttribute("aria-expanded", "true");
    await page.getByLabel("Counting channel").selectOption({ label: "#counting" });
  } finally {
    releaseRefresh();
  }
  await expect(page.getByRole("button", { name: "Save counting" })).toBeEnabled();
  await expect(page.getByLabel("Counting channel")).toHaveValue(countingChannelId(guild));
  await page.unroute(`**${packPath}`);
  const countingSaved = page.waitForResponse(
    (response) => response.url().endsWith(`${packPath}/counting`) && response.request().method() === "PUT",
  );
  await page.getByRole("button", { name: "Save counting" }).click();
  expect((await countingSaved).ok()).toBeTruthy();
  const saved = await (await page.request.get(packPath)).json();
  expect(saved.counting.channel_id).toBe(countingChannelId(guild));
  expect(saved.qotd.channel_id).toBe("7001");
  await page.getByRole("button", { name: /APPLICATION FORMS/ }).click();
  await page.getByRole("button", { name: "New form" }).click();
  await page.getByLabel("Form name").fill("Helper");
  await page.getByLabel("Question 1").fill("Why would you like to help?");
  await page.getByLabel("Review channel").selectOption({ label: "#general" });
  await page.getByRole("button", { name: "Save form" }).click();
  const formsSaved = await (await page.request.get(packPath)).json();
  expect(formsSaved.forms.some((item: { name: string }) => item.name === "Helper")).toBe(true);
  await expect(page.getByText("Helper")).toBeVisible();
  await shot(page, "community-pack-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "community-pack-phone", true);
});

test("counting API saves independently for two guilds", async ({ request }) => {
  const guilds = [`${stamp}6`, `${stamp}7`];
  const login = await request.post("/auth/dev-login", {
    data: {
      user_id: user,
      username: "counting_owner",
      guilds: guilds.map((id) => ({ id, name: "Counting reef", owner: true, permissions: "0", icon: null })),
    },
  });
  expect(login.ok(), await login.text()).toBeTruthy();
  const me = await request.get("/api/me");
  expect(me.ok()).toBeTruthy();
  const { csrf } = await me.json();
  // Save and update both guilds, then read both back to detect collisions or cross-guild overwrites.
  for (const allowMath of [false, true]) {
    for (const id of guilds) {
      const response = await request.put(`/api/guilds/${id}/community-pack/counting`, {
        headers: { "X-CSRF-Token": csrf },
        data: {
          channel_id: countingChannelId(id),
          reset_on_fail: true,
          allow_math: allowMath,
          fail_role_id: null,
        },
      });
      expect(response.ok(), await response.text()).toBeTruthy();
    }
    for (const id of guilds) {
      const response = await request.get(`/api/guilds/${id}/community-pack`);
      expect(response.ok()).toBeTruthy();
      expect((await response.json()).counting).toMatchObject({
        channel_id: countingChannelId(id),
        allow_math: allowMath,
      });
    }
  }
});
