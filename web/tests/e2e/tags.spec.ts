// P4 in a real browser: write a custom command with replies, an embed and an action and watch the bot's own
// preview, save it, switch it off, export and import it, delete it; and the auto-responders, where keyword
// triggers are locked because this bot runs without the Message Content intent.

import { type Page, expect, test } from "@playwright/test";

import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}3`;
const user = `${run}5`;

async function login(page: Page) {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
}

/** A write to the dashboard's API as its own script sends it: the session's CSRF token, same origin. */
async function send(page: Page, method: "PUT" | "PATCH" | "POST", path: string, data: unknown) {
  const csrf = ((await (await page.request.get("/api/me")).json()) as { csrf: string }).csrf;
  return page.request.fetch(path, {
    method,
    data,
    headers: { "x-csrf-token": csrf, origin: new URL(page.url()).origin },
  });
}

/** The same, for a write that must work. */
async function write(page: Page, method: "PUT" | "PATCH" | "POST", path: string, data: unknown) {
  const response = await send(page, method, path, data);
  expect(response.ok(), await response.text()).toBeTruthy();
}

const row = (page: Page, name: string) => page.locator(`[data-command="${name}"]`);
const responderRow = (page: Page, name: string) => page.locator(`[data-responder="${name}"]`);

/** The preview, once it has caught up with what was typed. */
async function previewed(page: Page) {
  const live = page.locator(".preview-live");
  await expect(live).not.toHaveClass(/is-busy/, { timeout: 15_000 });
  return live;
}

test("write a command with the bot's own preview, save it, switch it off, move it and delete it", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await login(page);
  await page.goto(`/g/${guild}/commands`);
  await expect(page.getByRole("heading", { name: "CUSTOM COMMANDS", exact: true })).toBeVisible();
  await expect(page.getByText("No custom commands yet")).toBeVisible();
  await shot(page, "commands-empty");

  await page.getByRole("link", { name: "New command" }).click();
  await expect(page.getByRole("heading", { name: "NEW COMMAND", exact: true })).toBeVisible();
  await expect(page.getByText("Write a reply to see it here.")).toBeVisible();

  // The name is checked as it is typed, with the bot's sentences.
  await page.getByLabel("Name", { exact: true }).fill("Help me");
  await expect(page.getByText("starting with a letter or number")).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("help");
  await expect(page.getByText("belongs to one of Pinchora's own commands")).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("Rules");
  await expect(page.getByText("starting with a letter or number")).toHaveCount(0);
  await page.getByLabel("Description").fill("The server rules");

  // The reply: chips put placeholders in, and the preview is the bot's own pipeline.
  await page.getByLabel("Reply 1", { exact: true }).fill("Be kind, ");
  await page.getByRole("button", { name: "{user}", exact: true }).click();
  await page.getByLabel("Try it with").fill("<@5> everyone @here");
  await expect(page.getByLabel("Try it with")).toHaveValue("<@5> everyone @here");
  await expect(page.getByLabel("Reply 1", { exact: true })).toHaveValue("Be kind, {user}");
  const live = await previewed(page);
  await expect(live).toContainText("Be kind, @");
  await expect(live).not.toContainText("<@");
  await page.getByRole("button", { name: "Add another reply (1/5)" }).click();
  await page.getByLabel("Reply 2", { exact: true }).fill("Rule one: {1:friend}, be kind.");
  await page.getByRole("button", { name: "Add an embed" }).click();
  await page.getByLabel("Title", { exact: true }).fill("The rules of {server}");
  await page.getByLabel("Description", { exact: true }).nth(1).fill("1. Be kind\n2. No spam");
  await page.getByRole("button", { name: "Add a field" }).click();
  await page.getByLabel("Field 1 name").fill("Where");
  await page.getByLabel("Field 1 value").fill("Right {channel}");
  await page.getByRole("button", { name: "Roll again" }).click();
  await expect(live).toContainText("The rules of");
  await expect(live.locator(".embed-card")).toContainText("1. Be kind");
  await expect(live.locator(".embed-card")).toContainText("Right #");

  // An action: a role the bot can hand out is offered, one above it is greyed out with the reason.
  await page.getByRole("button", { name: "Add an action (0/3)" }).click();
  await expect(page.getByRole("option", { name: /Admin — above my role/ })).toBeDisabled();
  await page.locator(".action-row").first().locator("select").nth(1).selectOption({ label: "@Fan" });
  await page.getByLabel("Cooldown").fill("30");
  await shot(page, "command-editor-desktop", true);
  await expect(page.getByText(/unsaved change/)).toBeVisible();

  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved /tag rules" })).toBeVisible();
  await expect(row(page, "rules")).toContainText("/tag rules");
  await expect(row(page, "rules")).toContainText("cooldown 30 s");
  await expect(row(page, "rules")).toContainText("embed");
  await expect(row(page, "rules")).toContainText("2 replies");
  await shot(page, "commands-list-desktop", true);

  // The server saved what the form said, ids as strings.
  const list = await (await page.request.get(`/api/guilds/${guild}/commands`)).json();
  expect(list.items).toHaveLength(1);
  expect(list.items[0].allow_roles).toEqual([]);
  expect(list.items[0].actions).toEqual([{ kind: "add_role", role_id: "9001" }]);
  expect(list.items[0].responses).toEqual(["Be kind, {user}", "Rule one: {1:friend}, be kind."]);

  // Switched off from the list.
  await page.getByRole("switch", { name: "Command rules" }).click();
  await expect(page.getByRole("switch", { name: "Command rules" })).toHaveAttribute("aria-checked", "false");
  const off = await (await page.request.get(`/api/guilds/${guild}/commands`)).json();
  expect(off.items[0].enabled).toBe(false);

  // Export, then import the file into a second command list: skipped by default, replaced on request.
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Export to a file" }).click(),
  ]);
  expect(download.suggestedFilename()).toBe(`pinchora-commands-${guild}.json`);
  const text = await (await import("node:fs/promises")).readFile((await download.path())!, "utf8");
  const file = JSON.parse(text);
  expect(file.version).toBe(1);
  expect(file.items[0].name).toBe("rules");
  file.items.push({ name: "faq", responses: ["Ask in #help."], cooldown_secs: 5 });
  file.items.push({ name: "BAD NAME", responses: ["x"] });
  await page.getByLabel("Import commands file").setInputFiles({
    name: "commands.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(file)),
  });
  const result = page.locator(".import-result");
  await expect(result).toContainText("1 added, 1 skipped (already there), 1 refused");
  await expect(result).toContainText("BAD NAME");
  await expect(row(page, "faq")).toBeVisible();
  await shot(page, "commands-imported", true);

  // Delete asks first.
  await page.getByRole("button", { name: "Delete faq" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("Delete /tag faq?");
  await page.getByRole("button", { name: "Keep it" }).click();
  await expect(row(page, "faq")).toBeVisible();
  await page.getByRole("button", { name: "Delete faq" }).click();
  await page.getByRole("button", { name: "Delete it" }).click();
  await expect(row(page, "faq")).toHaveCount(0);

  // The audit log words it, with names and no ids.
  await page.goto(`/g/${guild}/audit`);
  await expect(page.getByText(/wrote the custom command \/tag rules/)).toBeVisible();
  await expect(page.getByText(/imported custom commands \(1 new\)/)).toBeVisible();
  await expect(page.getByText(/deleted \/tag faq/)).toBeVisible();
  await expect(page.getByText(/turned \/tag rules off/)).toBeVisible();
  await shot(page, "audit-commands");
});

test("the command editor fits a phone", async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/commands/new`);
  await expect(page.getByRole("heading", { name: "NEW COMMAND", exact: true })).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("phone");
  await page.getByLabel("Reply 1", { exact: true }).fill("Hello from a phone, {user}!");
  await page.getByRole("button", { name: "Add an embed" }).click();
  await page.getByLabel("Title", { exact: true }).fill("A title");
  await page.getByRole("button", { name: "Add a field" }).click();
  await page.getByRole("button", { name: "Add an action (0/3)" }).click();
  await previewed(page);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "command-editor-phone", true);
  await page.getByRole("button", { name: "Reset" }).click();

  await page.goto(`/g/${guild}/commands`);
  await expect(page.getByRole("heading", { name: "CUSTOM COMMANDS", exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "commands-list-phone", true);
});

test("keyword responders are locked without the Message Content intent; the others work", async ({
  page,
}) => {
  await login(page);
  await page.goto(`/g/${guild}/responders`);
  await expect(page.getByRole("heading", { name: "AUTO-RESPONDERS", exact: true })).toBeVisible();
  await expect(page.getByTestId("keyword-lock")).toContainText("Message Content intent");
  await shot(page, "responders-empty-locked");

  await page.getByRole("link", { name: "New responder" }).click();
  await expect(page.getByRole("heading", { name: "NEW RESPONDER", exact: true })).toBeVisible();
  const keyword = page.getByRole("radio", { name: /A message has a keyword/ });
  await expect(keyword).toBeDisabled();
  await expect(page.getByTestId("keyword-reason")).toContainText("runs without");
  await expect(page.locator(".choice-card.is-locked .badge")).toHaveText("LOCKED");

  // A greeting: the join trigger answers in a channel, with the join-only placeholder filled by the bot.
  await page.getByLabel("Name", { exact: true }).fill("Welcome greeter");
  await page
    .getByLabel("Reply 1", { exact: true })
    .fill("Hi {user.name}, you are member {count} of {server}!");
  await page.getByLabel("Channel", { exact: true }).selectOption({ label: "#welcome" });
  await page.getByLabel("Cooldown").fill("3600");
  const live = await previewed(page);
  await expect(live).toContainText("you are member 1234 of Pinchora Reef!");
  await page.getByRole("button", { name: "Add an action (0/3)" }).click();
  await expect(page.getByLabel("Action 1", { exact: true })).not.toContainText("React to the message");
  // An action without its role waits to be finished: nothing is saved half-written, and the preview carries on.
  await expect(page.getByText("Action 1 needs a role.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Save changes" })).toBeDisabled();
  await expect(live).toContainText("you are member 1234 of Pinchora Reef!");
  await shot(page, "responder-editor-desktop", true);
  await page.getByRole("button", { name: "Remove action 1" }).click();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved “Welcome greeter”" })).toBeVisible();
  await expect(responderRow(page, "Welcome greeter")).toContainText("A member joins");
  await expect(responderRow(page, "Welcome greeter")).toContainText("Answers in one channel");
  await shot(page, "responders-list-desktop", true);

  // The server refuses a keyword responder switched on, and says why.
  const refused = await send(page, "POST", `/api/guilds/${guild}/responders`, {
    name: "k",
    trigger_kind: "keyword",
    trigger_value: "hi",
    responses: ["x"],
    destination: "current",
  });
  expect(refused.status()).toBe(422);
  expect((await refused.json()).error.code).toBe("blocked");

  // A level-up responder shows its own chips and answers where the member levelled up.
  await page.goto(`/g/${guild}/responders/new`);
  await page.getByRole("radio", { name: /A member levels up/ }).check({ force: true });
  await expect(page.getByRole("button", { name: "{level}", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "{count}", exact: true })).toHaveCount(0);
  await expect(page.getByRole("radio", { name: /Where it happened/ })).toBeChecked();
  await page.getByRole("radio", { name: /A member leaves/ }).check({ force: true });
  await expect(page.getByRole("radio", { name: /A direct message/ })).toHaveCount(0);
  await page
    .getByRole("button", { name: "Reset" })
    .click()
    .catch(() => undefined);
});

test("the responder pages fit a phone", async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/g/${guild}/responders/new`);
  await expect(page.getByRole("heading", { name: "NEW RESPONDER", exact: true })).toBeVisible();
  await page.getByLabel("Reply 1", { exact: true }).fill("Hello!");
  await previewed(page);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "responder-editor-phone", true);
  await page.goto(`/g/${guild}/responders`);
  await expect(page.getByRole("heading", { name: "AUTO-RESPONDERS", exact: true })).toBeVisible();
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "responders-list-phone", true);
});

test("the overview, the modules page and the community page tell one story", async ({ page }) => {
  await login(page);
  await page.goto(`/g/${guild}`);
  // Goodbye has a channel but its module is off: the glance must say off, like the grid.
  await write(page, "PATCH", `/api/guilds/${guild}/settings`, { goodbye_channel_id: "7010" });
  await write(page, "PUT", `/api/guilds/${guild}/modules/goodbye`, { enabled: false });
  await page.reload();
  const glance = page.locator(".glance-tile", { hasText: "Goodbye" });
  await expect(glance).toContainText("OFF");
  await expect(glance).toContainText("switched off");
  await expect(
    page.getByRole("list", { name: "Modules" }).locator("li", { hasText: "Goodbye" }),
  ).toContainText("OFF");
  await shot(page, "overview-consistent", true);

  // One ON per row on the modules page: the switch says it, there is no second badge.
  await page.goto(`/g/${guild}/modules`);
  await expect(page.locator('[data-module="pets"] .badge')).toHaveCount(0);
  await expect(page.locator('[data-module="pets"]').getByRole("switch")).toContainText("ON");
  await shot(page, "modules-one-on", true);

  // The community page says when the module a section belongs to is off, and the next storm reads
  // without seconds.
  await write(page, "PUT", `/api/guilds/${guild}/modules/suggestions`, { enabled: false });
  await write(page, "PUT", `/api/guilds/${guild}/surge/settings`, {
    enabled: true,
    channel_id: "7011",
    every_hours: 12,
  });
  await page.goto(`/g/${guild}/community`);
  await expect(page.getByRole("heading", { name: "COMMUNITY", exact: true })).toBeVisible();
  await expect(
    page.getByRole("note").filter({ hasText: "Suggestions is switched off in Modules" }),
  ).toBeVisible();
  await expect(
    page.getByText(/The next one comes \w{3} \d+, \d{4}, \d{1,2}:\d{2}(\s?[AP]M)?\./),
  ).toBeVisible();
  await shot(page, "community-consistent", true);
  await write(page, "PUT", `/api/guilds/${guild}/modules/suggestions`, { enabled: true });
  await write(page, "PUT", `/api/guilds/${guild}/modules/goodbye`, { enabled: true });
});
