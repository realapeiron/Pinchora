// The Tickets page in a real browser: a panel made in the editor (buttons, a form, a support role) and
// posted by the bot, the settings saved, then tickets opened and closed on that panel (by the Rust seed,
// standing in for members on Discord) shown in the table and one of them with its transcript. Desktop and
// phone screenshots of each.

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}3`;
const user = `${run}5`;

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

/** Opens and closes tickets on the panel, as members and staff would on Discord. */
function seedTickets() {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "api", "seed_demo_tickets", "--", "--ignored"],
    {
      cwd: fileURLToPath(new URL("../../..", import.meta.url)),
      env: { ...process.env, TICKETS_SEED_GUILD: guild, TICKETS_SEED_PANEL: "Help desk" },
      stdio: "inherit",
    },
  );
}

test.describe.configure({ mode: "serial" });

test("a panel is made, previewed and posted; the settings save", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/tickets`);
  await expect(page.getByRole("heading", { name: "TICKETS", exact: true })).toBeVisible();
  await expect(page.getByText("No panels yet.")).toBeVisible();
  await expect(page.getByRole("link", { name: "Tickets" })).toHaveAttribute("aria-current", "page");

  await page.getByRole("button", { name: "New panel" }).click();
  const editor = page.getByRole("region", { name: "New panel" });
  await editor.getByLabel("Name").fill("Help desk");
  await editor.getByLabel("Posted in").selectOption({ label: "#general" });
  await editor.getByLabel("Title").fill("Need a hand?");
  await editor.getByLabel("Topic 1 label").fill("Support");
  await editor.getByRole("button", { name: "Add topic" }).click();
  await editor.getByLabel("Topic 2 emoji").fill("💳");
  await editor.getByLabel("Topic 2 label").fill("Billing");
  await editor.getByLabel("Topic 2 colour").selectOption({ label: "Green" });
  // The preview follows the draft.
  const preview = editor.getByRole("region", { name: "Preview" });
  await expect(preview.getByText("Need a hand?")).toBeVisible();
  await expect(preview.getByText("Billing")).toBeVisible();
  await shot(page, "tickets-editor-buttons", true);

  // Buttons stop at five; a menu takes more. Empty topics are marked until they go again.
  for (let n = 3; n <= 5; n++) await editor.getByRole("button", { name: "Add topic" }).click();
  await expect(editor.getByRole("button", { name: "Add topic" })).toBeDisabled();
  await expect(editor.getByText("Topic 3 needs a label.")).toBeVisible();
  await expect(editor.getByRole("button", { name: "Save and post" })).toBeDisabled();
  await editor.getByRole("radio", { name: /Menu/ }).check({ force: true });
  await expect(editor.getByRole("button", { name: "Add topic" })).toBeEnabled();
  await expect(preview.getByRole("list", { name: "Menu options" })).toBeVisible();
  for (let n = 5; n >= 3; n--) await editor.getByRole("button", { name: `Remove topic ${n}` }).click();
  await editor.getByRole("radio", { name: /Buttons/ }).check({ force: true });

  await editor.getByRole("tab", { name: "Behaviour" }).click();
  await editor.getByLabel("Support roles").selectOption({ label: "@Fan" });
  await editor.getByRole("button", { name: "Add role" }).click();
  await expect(editor.getByRole("list", { name: "Support roles" })).toContainText("@Fan");
  await editor.getByRole("button", { name: "{user}" }).click();
  await shot(page, "tickets-editor-behaviour", true);

  await editor.getByRole("tab", { name: "Form" }).click();
  await editor.getByRole("button", { name: "Add question" }).click();
  await editor.getByLabel("Question 1", { exact: true }).fill("Your order number");
  await editor.getByLabel("Question 1 example").fill("#1234");
  await shot(page, "tickets-editor-form", true);

  await editor.getByRole("button", { name: "Save and post" }).click();
  await expect(page.getByRole("status").filter({ hasText: "is posted" })).toBeVisible();
  const row = page.locator('[data-panel="Help desk"]');
  await expect(row.getByText("POSTED", { exact: true })).toBeVisible();
  await expect(row).toContainText("2 topics · form of 1 question · 1 support role");
  const api = await (await page.request.get(`/api/guilds/${guild}/tickets`)).json();
  expect(api.panels[0]).toMatchObject({
    name: "Help desk",
    channel_id: "7001",
    support_roles: ["9001"],
    questions: [{ label: "Your order number", placeholder: "#1234" }],
  });
  expect(api.panels[0].message_id).toBeTruthy();

  // Settings: a category, a log channel, auto-close; the channel name shows what it makes.
  await page.getByLabel("Ticket category", { exact: true }).selectOption({ label: "▸ Staff" });
  await page.getByLabel("Log channel", { exact: true }).selectOption({ label: "#mod-log" });
  await page.getByLabel("Auto-close after (hours)", { exact: true }).fill("48");
  await page.getByLabel("Channel name", { exact: true }).fill("{topic}-{number}");
  await expect(page.getByText("#billing-0042")).toBeVisible();
  await page.getByRole("switch", { name: "Ratings" }).click();
  await expect(page.getByText("5 unsaved changes")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  const saved = await (await page.request.get(`/api/guilds/${guild}/tickets`)).json();
  expect(saved.settings).toMatchObject({
    open_category_id: "7020",
    log_channel_id: "7021",
    auto_close_hours: 48,
    naming: "{topic}-{number}",
    rating_enabled: true,
  });
  await shot(page, "tickets-page", true);
});

test("tickets opened on the panel show in the table and one shows its transcript", async ({ page }) => {
  test.setTimeout(600_000);
  seedTickets();
  await login(page);
  await page.goto(`/g/${guild}/tickets`);
  const table = page.getByRole("table");
  await expect(table.getByRole("row")).toHaveCount(4);
  await expect(page.getByText("Order problem")).toBeVisible();
  await expect(table.getByText("Kai").first()).toBeVisible();
  await expect(table.getByLabel("5 of 5 stars")).toBeVisible();
  await expect(page.locator(".tk-stat").filter({ hasText: "Opened" })).toContainText("3");
  await shot(page, "tickets-table", true);

  await page.getByRole("button", { name: "Closed", exact: true }).click();
  await expect(table.getByRole("row")).toHaveCount(2);
  await page.getByRole("button", { name: "All", exact: true }).click();
  await page.getByLabel("Words or number").fill("billing");
  await page.getByLabel("Words or number").press("Enter");
  await expect(table.getByRole("row")).toHaveCount(2);
  await expect(table).toContainText("Billing");
  await page.getByLabel("Words or number").fill("");
  await page.getByLabel("Words or number").press("Enter");

  await page.getByRole("link", { name: /#0001/ }).click();
  await expect(page.getByRole("heading", { name: "TICKET #0001" })).toBeVisible();
  const transcript = page.getByRole("list", { name: "Transcript of ticket #0001" });
  await expect(transcript.getByRole("listitem")).toHaveCount(4);
  // Hostile text is shown as text, never run.
  await expect(transcript.getByText("<script>alert('x')</script> my order never came")).toBeVisible();
  await expect(transcript.getByText("receipt.png", { exact: false })).toBeVisible();
  await expect(page.getByText("Solved: replacement sent")).toBeVisible();
  await expect(page.getByText("after 14 min")).toBeVisible();
  const download = page.getByRole("link", { name: "Download transcript" });
  const href = await download.getAttribute("href");
  const html = await (await page.request.get(href!)).text();
  expect(html).toContain("&lt;script&gt;");
  expect(html).not.toContain("<script");
  await shot(page, "tickets-transcript", true);

  await page.getByRole("link", { name: "All tickets" }).click();
  const support = table.locator('[data-ticket="3"]');
  await support.getByRole("button", { name: "Close" }).click();
  await expect(support).toContainText("CLOSED");
  await table.getByRole("link", { name: /#0002/ }).click();
  await page.getByRole("button", { name: "Close ticket" }).click();
  await expect(page.getByRole("heading", { name: "TICKET #0002" })).toBeVisible();
  await expect(page.getByText("CLOSED", { exact: true })).toBeVisible();
});

test("ticket operation settings and panel FAQ save on desktop and phone", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}/tickets`);
  await expect(page.locator("h1")).toHaveCount(1);
  const operations = page.getByRole("region", { name: "SUPPORT HOURS & SAVED REPLIES" });
  await operations.getByLabel("Support timezone").fill("Europe/Istanbul");
  await operations.getByRole("button", { name: "Add window" }).click();
  await operations.getByLabel("Weekday").selectOption("1");
  await operations.getByLabel("Starts").fill("09:00");
  await operations.getByLabel("Ends").fill("17:00");
  await operations.getByLabel("Outside-hours message").fill("We return tomorrow.");
  const saveRequest = page.waitForRequest(
    (request) => request.method() === "PUT" && request.url().endsWith("/tickets/operations/settings"),
  );
  await operations.getByRole("button", { name: "Save operations" }).click();
  expect((await saveRequest).postDataJSON()).not.toHaveProperty("status_message_id");
  await expect(operations.getByRole("button", { name: "Save operations" })).toBeDisabled();
  const panel = page.getByRole("region", { name: "PANEL OPERATIONS" });
  await panel.getByLabel("Claim mode").selectOption("exclusive");
  await panel.getByLabel("Open tickets per panel (0 = unlimited)").fill("2");
  await panel.getByRole("button", { name: "Add FAQ answer" }).click();
  await panel.getByLabel("Question", { exact: true }).fill("When is support open?");
  await panel.getByLabel("Answer", { exact: true }).fill("Weekdays, 9 to 5.");
  await panel.getByRole("button", { name: "Save panel operations" }).click();
  await expect(panel.getByRole("button", { name: "Save panel operations" })).toBeDisabled();
  await shot(page, "ticket-operations-operations", true);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "ticket-operations-operations-phone", true);
});

test("the tickets pages fit a phone", async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/tickets`);
  await expect(page.getByRole("heading", { name: "TICKETS", exact: true })).toBeVisible();
  await expect(page.locator(".tk-table")).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "tickets-phone", true);
  await page.locator('[data-panel="Help desk"]').getByRole("button", { name: "Edit Help desk" }).click();
  await expect(page.getByRole("tab", { name: "Buttons" })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "tickets-phone-editor", true);
  await page.getByRole("link", { name: /#0001/ }).click();
  await expect(page.getByRole("heading", { name: "TICKET #0001" })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "tickets-phone-transcript", true);
});
