// Temp voice in a real browser: set a creator up, tune it, see the rooms come and go, on a desktop and a phone.
// The rooms are made by the bot from voice events, which this run has none of, so they are put in the database
// with psql (`PSQL` names the binary; without one the live list is skipped).

import { execFileSync } from "node:child_process";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, openRow, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}7`;
const user = `${run}8`;
const LOUNGE = "7003";

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

const psql = process.env.PSQL ?? "/opt/homebrew/opt/postgresql@16/bin/psql";

/** Runs SQL on the test database; false when there is no psql to do it with. */
function sql(statement: string): boolean {
  try {
    execFileSync(
      psql,
      [process.env.PINCHORA_TEST_DATABASE_URL ?? "", "-v", "ON_ERROR_STOP=1", "-c", statement],
      {
        stdio: "ignore",
      },
    );
    return true;
  } catch {
    return false;
  }
}

const form = (page: Page, name: string) => page.getByRole("form", { name: `Settings of ${name}` });

test("set a creator up, tune it, and watch the rooms", async ({ page }) => {
  // The mock channel ids stay the same across runs; clear any interrupted run's creator first.
  sql(`DELETE FROM voice_creators WHERE channel_id = ${LOUNGE}`);
  sql(`DELETE FROM temp_voice_channels WHERE channel_id IN (7005, 7006)`);
  await login(page);
  await page.goto(`/g/${guild}/temp-voice`);
  await expect(page.getByRole("heading", { name: "TEMP VOICE", exact: true })).toBeVisible();
  await expect(page.getByText(/^No creator channels yet\./)).toBeVisible();
  await expect(page.getByText("No rooms right now", { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Temp voice" })).toBeVisible();
  await shot(page, "temp-voice-empty", true);

  // Pick a voice channel: it becomes a creator and its settings open.
  await expect(page.getByLabel("Add a creator").locator("option", { hasText: "Lounge" })).toBeAttached();
  await page.getByLabel("Add a creator").selectOption({ label: "🔊 Lounge" });
  await page.getByLabel("Add a creator").focus();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Add creator" })).toBeFocused();
  await expect(page.getByRole("button", { name: "Add creator" })).toHaveCSS("outline-style", "solid");
  await shot(page, "temp-voice-keyboard-focus");
  await page.getByRole("button", { name: "Add creator" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Creator added" })).toBeVisible();
  const settings = form(page, "Lounge");
  await expect(settings).toBeVisible();
  await expect(page.locator(".creator-summary")).toContainText("Mira's room");

  // The name is checked as the bot checks it, and shown the way members will see it.
  const name = settings.getByLabel("Room name");
  await name.fill("{nick}");
  await expect(settings.getByRole("alert")).toContainText("isn't one of the words");
  await expect(page.getByRole("button", { name: "Save creator" })).toBeDisabled();
  await name.fill("Room {n} · {user}");
  await expect(settings.locator(".sample-line")).toContainText("Room 1 · Mira");
  await settings.getByRole("button", { name: "{n}", exact: true }).click();
  await expect(name).toHaveValue("Room {n} · {user}{n}");
  await name.fill("Room {n} · {user}");
  await settings.getByLabel("People per room").fill("4");
  await settings.getByLabel("Rooms per member").selectOption("2");
  await settings.getByRole("switch", { name: "Starts locked" }).click();
  await settings.getByLabel("Private chat thread").selectOption({ label: "#general" });
  await settings.getByRole("button", { name: "Preview room" }).click();
  await expect(settings.getByRole("region", { name: "New room preview" })).toContainText("Room 1 · Mira");
  await expect(settings.getByRole("region", { name: "New room preview" })).toContainText("Starts locked");
  await shot(page, "temp-voice-editing", true);

  await page.getByRole("button", { name: "Save creator" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/temp-voice`)).json();
  expect(saved.creators).toHaveLength(1);
  expect(saved.creators[0]).toMatchObject({
    channel_id: LOUNGE,
    category_id: "7004",
    name_template: "Room {n} · {user}",
    default_limit: 4,
    default_locked: true,
    max_per_user: 2,
    panel: true,
    panel_channel_id: "7001",
  });

  // A bad limit is refused by the page before the server sees it.
  await settings.getByLabel("People per room").fill("100");
  await expect(settings.getByRole("alert")).toContainText("0 (no limit) to 99");
  await expect(page.getByRole("button", { name: "Save creator" })).toBeDisabled();
  await settings.getByLabel("People per room").fill("4");
  await page.getByRole("button", { name: "Close" }).first().click();
  await expect(form(page, "Lounge")).toHaveCount(0);
  const row = page.getByRole("list", { name: "Creator channels" }).locator("li").first();
  await expect(row).toContainText("Room 1 · Mira");
  await expect(row).toContainText("STARTS LOCKED");
  await expect(row).toContainText("in Chat");

  // The audit log wrote it down.
  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText(/set up a temp voice creator/).first()).toBeVisible();
  await page.goto(`/g/${guild}/temp-voice`);

  // Rooms alive right now: read-only, with who owns them and who is in.
  const seeded =
    sql(`DELETE FROM voice_creators WHERE channel_id IN (7005, 7006)`) &&
    sql(
      `INSERT INTO temp_voice_channels (channel_id, guild_id, source_id, owner_id, created_at, locked, hidden, user_limit)
       VALUES (7005, ${guild}, ${LOUNGE}, ${run}1, now() - interval '12 minutes', true, false, 4),
              (7006, ${guild}, ${LOUNGE}, ${run}2, now() - interval '3 hours', false, true, 0)
       ON CONFLICT (channel_id) DO UPDATE SET guild_id = EXCLUDED.guild_id, source_id = EXCLUDED.source_id,
         owner_id = EXCLUDED.owner_id, created_at = EXCLUDED.created_at, locked = EXCLUDED.locked,
         hidden = EXCLUDED.hidden, user_limit = EXCLUDED.user_limit`,
    );
  if (seeded) {
    await page.reload();
    await openRow(page, /^Live rooms/);
    const rooms = page.getByRole("list", { name: "Live rooms" });
    await expect(rooms.locator("li")).toHaveCount(2);
    await expect(rooms.locator("li", { hasText: "Gaming" })).toContainText("LOCKED");
    await expect(rooms.locator("li", { hasText: "Study hall" })).toContainText("HIDDEN");
    await expect(rooms.locator("li").first()).toContainText("OWNER");
    await expect(page.getByRole("status").filter({ hasText: "2 of 60 live rooms open" })).toBeVisible();
    await expect(row).toContainText("2 open now");
  }
  await shot(page, "temp-voice-desktop", true);

  // On a phone nothing runs off the side.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/temp-voice`);
  await expect(page.getByRole("heading", { name: "TEMP VOICE", exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "temp-voice-phone", true);
  const creatorRow = page.getByRole("button", { name: /^🔊 Lounge/ });
  await openRow(creatorRow);
  await page.getByRole("button", { name: "Edit Lounge" }).click();
  await expect(form(page, "Lounge")).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "temp-voice-phone-editing", true);
  await page.setViewportSize({ width: 1280, height: 800 });

  // Removing asks twice; the rooms it made stay in the list until they empty.
  await page.getByRole("button", { name: "Remove Lounge" }).click();
  await expect(page.getByRole("button", { name: "Really remove Lounge" })).toBeVisible();
  await page.getByRole("button", { name: "Keep" }).click();
  await expect(page.getByRole("button", { name: "Remove Lounge" })).toBeVisible();
  await page.getByRole("button", { name: "Remove Lounge" }).click();
  await page.getByRole("button", { name: "Really remove Lounge" }).click();
  await expect(page.getByText(/^No creator channels yet\./)).toBeVisible();
  if (seeded) {
    const roomsRow = page.getByRole("button", { name: /^Live rooms/ });
    await expect(roomsRow).toHaveAttribute("aria-expanded", "true");
    const rooms = page.getByRole("list", { name: "Live rooms" });
    await expect(rooms.locator("li")).toHaveCount(2);
    await expect(rooms.locator("li", { hasText: "Gaming" })).toContainText("LOCKED");
    await expect(rooms.locator("li", { hasText: "Study hall" })).toContainText("HIDDEN");
  }
  await shot(page, "temp-voice-removed", true);
});

test("the page says so when the module is off", async ({ page }) => {
  await login(page);
  const modules = `/api/guilds/${guild}/modules/temp_voice`;
  const off = await page.request.put(modules, {
    data: { enabled: false },
    headers: { "x-csrf-token": (await (await page.request.get("/api/me")).json()).csrf },
  });
  expect(off.ok(), await off.text()).toBeTruthy();
  await page.goto(`/g/${guild}/temp-voice`);
  await expect(page.getByRole("note").filter({ hasText: "switched off" })).toBeVisible();
  await shot(page, "temp-voice-module-off");
  await page.request.put(modules, {
    data: { enabled: true },
    headers: { "x-csrf-token": (await (await page.request.get("/api/me")).json()).csrf },
  });
});
