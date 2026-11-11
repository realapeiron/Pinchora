import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("leveling parity saves only changes, previews imports and fits a phone", async ({ page }) => {
  const guild = `${Date.now()}77`;
  const config = {
    curve: "classic",
    message_min: 15,
    message_max: 25,
    message_cooldown_secs: 60,
    messages: true,
    reactions: false,
    voice: true,
    effort: false,
    votes: false,
    separate_voice_level: false,
    reset_on_leave: false,
    leave_grace_secs: 300,
    reaction_xp: 5,
    effort_bonus: 5,
    vote_xp: 10,
    highlights_channel_id: null,
    vanity_slug: null,
  };
  const writes: unknown[] = [];
  let releasePreview: (() => void) | undefined;
  let holdPreview = true;
  await page.route(`**/api/guilds/${guild}/leveling-parity**`, async (route) => {
    const url = route.request().url();
    let result: unknown = config;
    if (url.endsWith("/voice-roles")) result = { items: [], max: 25 };
    else if (url.endsWith("/highlights")) result = { week: "2026-11-02", items: [] };
    else if (url.endsWith("/import/preview")) {
      if (holdPreview)
        await new Promise<void>((resolve) => {
          releasePreview = resolve;
        });
      result = {
        preview_id: "a".repeat(32),
        count: 1,
        changed: 1,
        sample: [{ user_id: "123", xp: 500, current_xp: 0, result_xp: 500 }],
      };
    } else if (url.endsWith("/import/apply")) result = { checked: 1, updated: 1 };
    else if (route.request().method() === "PUT") {
      const body = route.request().postDataJSON();
      writes.push(body);
      Object.assign(config, body);
    }
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(result) });
  });
  await devLogin(page, "1", [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "32" }]);
  await page.goto(`/g/${guild}/leveling`);
  await expect(page.getByRole("heading", { level: 1, name: "LEVELING" })).toHaveCount(1);
  const save = page.getByRole("button", { name: "Save XP options" });
  await expect(save).toBeDisabled();
  await page.getByLabel("Minimum message XP").fill("20");
  await expect(save).toBeEnabled();
  await page.getByLabel("Minimum message XP").fill("15");
  await expect(save).toBeDisabled();
  await page.getByLabel("Minimum message XP").fill("20");
  await save.click();
  expect(writes).toHaveLength(1);
  expect((writes[0] as typeof config).message_min).toBe(20);
  await page.getByRole("button", { name: /IMPORT LEVELS FROM ANOTHER BOT/ }).click();
  await page.getByLabel("Export file").setInputFiles({
    name: "arcane.csv",
    mimeType: "text/csv",
    buffer: Buffer.from("user_id,xp\n123,500\n"),
  });
  await page
    .getByLabel("Export file")
    .setInputFiles({ name: "large.csv", mimeType: "text/csv", buffer: Buffer.alloc(2_000_001) });
  await expect(page.getByRole("button", { name: "Preview import" })).toBeDisabled();
  await page
    .getByLabel("Export file")
    .setInputFiles({ name: "valid.csv", mimeType: "text/csv", buffer: Buffer.from("user_id,xp\n123,500\n") });
  await page.getByRole("button", { name: "Preview import" }).click();
  await expect.poll(() => Boolean(releasePreview)).toBeTruthy();
  await page.getByLabel("How to combine XP").selectOption("replace");
  holdPreview = false;
  releasePreview?.();
  await expect(page.getByRole("button", { name: "Preview import" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Apply import" })).toHaveCount(0);
  await page.getByRole("button", { name: "Preview import" }).click();
  await expect(page.getByText("123: 0 → 500 XP")).toBeVisible();
  await page.getByRole("button", { name: "Apply import" }).click();
  await expect(page.getByText("Import complete")).toBeVisible();
  await shot(page, "leveling-parity-desktop", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "leveling-parity-phone", true);
});
