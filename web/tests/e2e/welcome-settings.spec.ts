import { expect, test, type Page } from "@playwright/test";
import { devLogin, overflow } from "./helpers";

const run = Date.now();
async function open(page: Page, width: number, guild: string) {
  await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
  await devLogin(page, `${run}${width}`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "0" },
  ]);
  await page.goto(`/g/${guild}/welcome`);
  await expect(page.getByRole("heading", { name: "WELCOME", exact: true })).toBeVisible();
}

for (const width of [1440, 390]) {
  test(`Welcome settings rows, preview, save undo and module at ${width}px`, async ({ page }) => {
    const guild = `${run}${width === 390 ? 3 : 4}`;
    await open(page, width, guild);
    const rows = page.locator(".settings-detail-row");
    await expect(rows).toHaveCount(3);
    const welcome = rows.nth(0);
    const join = rows.nth(1);
    const goodbye = rows.nth(2);
    const heading = welcome.getByRole("button", { name: /Welcome message/ });
    await expect(heading).toHaveAttribute("aria-expanded", "true");
    await heading.focus();
    await page.keyboard.press("Enter");
    await expect(heading).toHaveAttribute("aria-expanded", "false");
    await expect(heading).toContainText(/#|Off/);
    await page.keyboard.press("Space");
    await expect(heading).toHaveAttribute("aria-expanded", "true");

    const message = welcome.getByLabel("Text above the card");
    await message.fill("Hello world");
    await message.evaluate((element: HTMLTextAreaElement) => element.setSelectionRange(6, 6));
    await welcome.getByRole("button", { name: "{user}", exact: true }).click();
    await expect(message).toHaveValue("Hello {user}world");
    await expect(welcome.getByRole("region", { name: "Welcome preview" })).toContainText("Hello", {
      timeout: 10_000,
    });
    await welcome.getByLabel("Channel").selectOption({ label: "#welcome" });
    await heading.focus();
    await page.keyboard.press("Enter");
    await expect(heading).toContainText("#welcome");

    const roleHeading = join.getByRole("button", { name: /Join role/ });
    await roleHeading.focus();
    await page.keyboard.press("Enter");
    await join.getByLabel("Role new members get").selectOption({ label: "@Fan" });
    await page.keyboard.press("Tab");
    await roleHeading.focus();
    await page.keyboard.press("Enter");
    await expect(roleHeading).toContainText("@Fan");

    const goodbyeHeading = goodbye.getByRole("button", { name: /Goodbye message/ });
    await expect(goodbyeHeading).toContainText("Off");
    await goodbyeHeading.focus();
    await page.keyboard.press("Enter");
    await goodbye.getByLabel("Channel").selectOption({ label: "#general" });
    await goodbye.getByLabel("Goodbye text").fill("Goodbye {user}!");
    await expect(goodbye.getByRole("region", { name: "Goodbye preview" })).toContainText("Goodbye", {
      timeout: 10_000,
    });
    await goodbyeHeading.focus();
    await page.keyboard.press("Enter");
    await expect(goodbyeHeading).toContainText("#general");

    const before = (await (await page.request.get(`/api/guilds/${guild}`)).json()).settings;
    await page.getByRole("button", { name: "Save changes" }).click();
    await expect(page.getByRole("button", { name: "Undo", exact: true })).toBeVisible();
    const saved = (await (await page.request.get(`/api/guilds/${guild}`)).json()).settings;
    expect(saved.welcome_message).toBe("Hello {user}world");
    expect(saved.join_role_id).toBe("9001");
    expect(saved.goodbye_channel_id).toBe("7001");
    await page.getByRole("button", { name: "Undo", exact: true }).click();
    await expect(page.getByRole("status").filter({ hasText: "Previous values restored" })).toBeVisible();
    const restored = (await (await page.request.get(`/api/guilds/${guild}`)).json()).settings;
    expect(restored.welcome_message).toBe(before.welcome_message);
    expect(restored.join_role_id).toBe(before.join_role_id);
    expect(restored.goodbye_channel_id).toBe(before.goodbye_channel_id);

    const master = page.getByRole("switch", { name: "Module: Welcome cards" });
    await expect(master).toHaveCount(1);
    if ((await master.getAttribute("aria-checked")) === "false") {
      await expect(page.locator(".module-off-note")).toContainText("Nothing is sent while it is off");
      await master.click();
      await expect(master).toHaveAttribute("aria-checked", "true");
    }
    await master.click();
    await expect(master).toHaveAttribute("aria-checked", "false");
    await expect(page.locator(".module-off-note")).toContainText("Nothing is sent while it is off");
    await roleHeading.focus();
    await page.keyboard.press("Enter");
    await expect(join.getByLabel("Role new members get")).toBeEnabled();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
  });
}
