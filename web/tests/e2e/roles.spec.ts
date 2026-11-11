// The roles page in a real browser: auto roles with a delay and an expiry, the panel editor with its live
// preview and the bot posting the panel, and colour roles. Screenshots on desktop and phone.

import { type Locator, type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}3`;
const user = `${run}5`;
const ADMIN = "9003";

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

/** A write straight to the API, with the CSRF token a page would send. */
async function write(page: Page, method: "PUT" | "POST", path: string, data: unknown) {
  const me = await (await page.request.get("/api/me")).json();
  return page.request.fetch(`/api/guilds/${guild}/roles${path}`, {
    method,
    data,
    headers: { "X-CSRF-Token": me.csrf },
  });
}

/** A picture of one panel of the page, with the fixed bars out of the way as they would be when scrolled past. */
async function panelShot(section: Locator, name: string) {
  const page = section.page();
  const bars = (visibility: string) =>
    page.evaluate((v) => {
      for (const bar of document.querySelectorAll<HTMLElement>(".top-bar, .save-bar"))
        bar.style.visibility = v;
    }, visibility);
  await section.scrollIntoViewIfNeeded();
  await bars("hidden");
  await page.evaluate(() => document.fonts.ready);
  await section.screenshot({ path: `test-results/screens/${name}.png`, animations: "disabled" });
  await bars("");
}

const phone = { width: 390, height: 844 };
const desktop = { width: 1280, height: 800 };

test("auto roles: a delay, an expiry, pausing and sticky roles", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/roles`);
  await expect(page.getByRole("heading", { name: "ROLES", exact: true })).toBeVisible();
  await expect(page.getByText("No auto roles yet.")).toBeVisible();
  const auto = page
    .locator(".settings-detail-row")
    .filter({ has: page.getByRole("button", { name: /AUTO ROLES/ }) })
    .first();

  // A role the bot cannot give is greyed out in the picker, with the reason.
  await expect(page.locator("#auto-role option", { hasText: "@Admin" })).toBeDisabled();
  const refused = await write(page, "PUT", `/auto/${ADMIN}/join`, {});
  expect(refused.status()).toBe(422);
  expect((await refused.json()).error.message).toContain("above my highest role");

  await page.getByRole("radio", { name: /Members who join/ }).check({ force: true });
  await page.locator("#auto-role").selectOption({ label: "@Fan" });
  await page.locator("#auto-delay").fill("10");
  await page.locator(".duration select").first().selectOption("m");
  await page.locator("#auto-expiry").fill("1");
  await page.locator(".duration select").nth(1).selectOption("d");
  await auto.getByRole("button", { name: "Add", exact: true }).click();
  const row = page.getByRole("list", { name: "Auto roles" }).getByRole("listitem").first();
  await expect(row).toContainText("Fan");
  await expect(row).toContainText("after 10m");
  await expect(row).toContainText("taken back after 1d");
  await expect(page.getByText("0/10")).toHaveCount(0);

  // A delay that is not allowed is told so before anything is sent.
  await page.locator("#auto-delay").fill("10");
  await page.locator(".duration select").first().selectOption("d");
  await page.locator("#auto-delay").fill("40");
  await expect(page.getByText("A delay is none, or 30 seconds to 30 days.")).toBeVisible();
  await expect(auto.getByRole("button", { name: "Add", exact: true })).toBeDisabled();
  await page.locator("#auto-delay").fill("0");

  await page.getByRole("switch", { name: /Give Fan to members who join/ }).click();
  await expect(row).toContainText("PAUSED");
  await page.getByRole("switch", { name: "Sticky roles" }).click();
  await expect(page.getByRole("switch", { name: "Sticky roles" })).toHaveAttribute("aria-checked", "true");

  const saved = await (await page.request.get(`/api/guilds/${guild}/roles`)).json();
  expect(saved.auto.items).toEqual([
    { role_id: "9001", kind: "join", delay_secs: 600, remove_after_secs: 86_400, enabled: false },
  ]);
  expect(saved.auto.sticky).toBe(true);

  await panelShot(auto, "roles-auto-desktop");
  await page.setViewportSize(phone);
  await page.reload();
  await expect(page.getByRole("heading", { name: "ROLES", exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "roles-auto-phone");
  await page.setViewportSize(desktop);

  // Taking it away.
  await page.getByRole("button", { name: /Remove Fan from members who join/ }).click();
  await expect(page.getByText("No auto roles yet.")).toBeVisible();
});

test("the panel editor previews the message, saves it and has the bot post it", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/roles`);
  const newPanel = page.getByRole("button", { name: "New panel" });
  await newPanel.focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  await expect(newPanel).toBeFocused();
  expect(await newPanel.evaluate((button) => getComputedStyle(button).outlineStyle)).toBe("solid");
  await newPanel.click();
  await expect(page.getByRole("heading", { name: "NEW PANEL" })).toBeVisible();

  await page.getByLabel("Title", { exact: true }).fill("Pick your games");
  await page.getByLabel("Text above the roles").fill("Tell us what you play.");
  await page.getByRole("radio", { name: /Menu/ }).check({ force: true });
  const preview = page.getByRole("region", { name: "Panel preview" });
  await expect(preview).toContainText("Pick your games");
  await expect(preview).toContainText("Pick your roles from the menu");

  await page.getByRole("button", { name: "Add a role" }).click();
  await page.getByRole("button", { name: "Add a role" }).click();
  const rows = page.getByRole("list", { name: "Roles on the panel" }).getByRole("listitem");
  await rows.nth(0).getByLabel("Role", { exact: true }).selectOption({ label: "@Fan" });
  await expect(rows.nth(0).getByLabel("Label", { exact: true })).toHaveValue("Fan");
  await rows.nth(1).getByLabel("Role", { exact: true }).selectOption({ label: "@Regular" });
  await rows.nth(1).getByLabel("Emoji").fill("🎮");
  await rows.nth(1).getByLabel("Line under the label").fill("Plays all sorts");
  await expect(preview).toContainText("Plays all sorts");
  await expect(preview).toContainText("🎮");
  await expect(page.getByText("/25 roles")).toContainText("2/25");

  // Rules that make no sense are refused on the page, and nothing can be saved.
  await page.getByRole("switch", { name: "One at a time" }).click();
  await page.getByLabel("Fewest roles").fill("2");
  await expect(page.getByRole("alert").filter({ hasText: "One at a time can ask" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await shot(page, "roles-panel-invalid");
  await page.getByLabel("Fewest roles").fill("1");

  const editor = page.locator(".panel", { has: page.getByRole("heading", { name: "NEW PANEL" }) }).last();
  await panelShot(editor, "roles-panel-editor-desktop");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Panel saved." })).toBeVisible();
  await expect(page.getByRole("heading", { name: "EDIT PANEL" })).toBeVisible();

  // Posting: the bot's message, in the channel picked.
  await page.getByLabel("Post it in").selectOption({ label: "#general" });
  await page.getByRole("button", { name: "Post panel" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Posted in" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Update message" })).toBeVisible();
  const stored = await (await page.request.get(`/api/guilds/${guild}/roles`)).json();
  const panel = stored.panels.items[0];
  expect(panel).toMatchObject({
    title: "Pick your games",
    style: "select",
    exclusive: true,
    min_picks: 1,
    channel_id: "7001",
  });
  expect(panel.roles.map((r: { role_id: string }) => r.role_id)).toEqual(["9001", "9002"]);
  expect(panel.message_id).toBeTruthy();

  await panelShot(
    page.locator("section", { has: page.getByRole("heading", { name: "ROLE PANELS" }) }),
    "roles-panels-list-desktop",
  );
  await page.setViewportSize(phone);
  await page.reload();
  await page
    .locator("[data-panel]")
    .filter({ hasText: "Pick your games" })
    .locator(".settings-detail-expand")
    .click();
  await page.getByRole("button", { name: "Edit Pick your games" }).click();
  await expect(page.getByRole("heading", { name: "EDIT PANEL" })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.getByRole("region", { name: "Panel preview" }).scrollIntoViewIfNeeded();
  await shot(page, "roles-panel-editor-phone");
  await panelShot(
    page.locator(".panel", { has: page.getByRole("heading", { name: "EDIT PANEL" }) }).last(),
    "roles-panel-editor-phone-full",
  );
  await page.setViewportSize(desktop);

  // A reactions panel needs an emoji on every role, and says so before it is posted.
  await page.getByRole("radio", { name: /Reactions/ }).check({ force: true });
  await expect(page.getByText("Fan needs an emoji.")).toBeVisible();
  await expect(page.getByRole("button", { name: /Update message|Post panel/ })).toBeDisabled();

  // Deleting asks twice.
  await page.getByRole("button", { name: "Reset" }).click();
  await page.getByRole("button", { name: "Delete Pick your games" }).click();
  await page.getByRole("button", { name: "Delete it" }).click();
  await expect(page.getByText("No panels yet.")).toBeVisible();
});

test("colour roles: the palette, the mode, who may pick and the anchor", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/roles`);
  const colours = page.locator(".color-role-settings");
  await expect(colours).toBeVisible();
  await colours.locator(".settings-detail-expand").first().click();
  await colours.locator(".settings-detail-expand").nth(1).click();
  await expect(colours.getByRole("list", { name: "Palette" }).getByRole("listitem")).not.toHaveCount(0);

  await colours.getByRole("switch", { name: "Colour roles" }).click();
  await expect(colours.getByRole("switch", { name: "Colour roles" })).toHaveAttribute("aria-checked", "true");
  await colours.getByRole("radio", { name: /Any colour/ }).click({ force: true });
  await expect(colours.getByRole("radio", { name: /Any colour/ })).toBeChecked();
  await colours.getByRole("button", { name: /^Ocean/ }).click();
  await expect(colours.getByRole("img", { name: "A name in #3a9bdc" })).toBeVisible();

  await colours.getByLabel("Who may pick").selectOption({ label: "@Fan" });
  await colours.getByRole("button", { name: "Add", exact: true }).click();
  await expect(colours.getByRole("list", { name: "Roles that may pick" })).toContainText("Fan");
  await colours.getByLabel("Make new colour roles under").selectOption({ label: "@Regular" });
  await expect(colours.getByLabel("Make new colour roles under")).toHaveValue("9002");

  // The bot cannot make roles in this mock: the answer is shown, not swallowed.
  await colours.getByRole("button", { name: "Create the missing roles", exact: true }).click();
  await expect(colours.getByRole("status")).toContainText("Made no roles");
  const saved = await (await page.request.get(`/api/guilds/${guild}/roles`)).json();
  expect(saved.colors).toMatchObject({
    enabled: true,
    mode: "hex",
    allowed_roles: ["9001"],
    anchor_role_id: "9002",
  });

  // Naming something that is not a role is refused.
  const refused = await write(page, "PUT", "/colors", { enabled: true, mode: "rainbow" });
  expect(refused.status()).toBe(422);
  expect((await refused.json()).error.field).toBe("mode");

  await panelShot(colours, "roles-colours-desktop");
  await page.setViewportSize(phone);
  await page.reload();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.locator(".color-role-settings").scrollIntoViewIfNeeded();
  await shot(page, "roles-colours-phone");
  await panelShot(page.locator(".color-role-settings"), "roles-colours-phone-full");
});
