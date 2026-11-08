import { expect, test } from "@playwright/test";
import { devLogin, overflow } from "./helpers";

const run = Date.now();
const guild = `${run}4`;
const user = `${run}5`;

test("message kit preview, revisions and event binding", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/message-studio`);
  await expect(page.getByRole("heading", { name: "MESSAGE STUDIO", exact: true })).toBeVisible();
  await page.getByLabel("Kit name").fill("Greeting kit");
  await page.getByLabel("Content", { exact: true }).fill("Hello {user} in {server}");
  await expect(page.getByRole("region", { name: "Message preview" })).toContainText("Pinchora Reef");
  await page.getByRole("button", { name: "Save kit" }).click();
  await expect(page.getByText("Kit saved.")).toBeVisible();
  await page.getByLabel("Welcome", { exact: true }).selectOption("greeting-kit");
  await expect(page.getByText("Event message updated.")).toBeVisible();
  await page.getByLabel("Content", { exact: true }).fill("Hello again {user}");
  await page.getByRole("button", { name: "Save kit" }).click();
  await expect(page.locator(".studio-revision")).toHaveCount(2);
  await page.locator(".studio-revision").last().getByRole("button", { name: "Restore" }).click();
  await expect(page.getByLabel("Content", { exact: true })).toHaveValue("Hello {user} in {server}");
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
});

test("imports stay validated, long previews fit a phone, and guild switches clear drafts", async ({
  page,
}) => {
  const other = `${run}6`;
  await devLogin(page, `${run}7`, [
    { id: guild, name: "Pinchora Reef", owner: true, permissions: "0" },
    { id: other, name: "Other Reef", owner: true, permissions: "0" },
  ]);
  await page.goto(`/g/${guild}/message-studio`);
  const file = page.locator('input[type="file"]');
  await file.setInputFiles({
    name: "bad.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ name: "Bad", content: "Hello", components: [null] })),
  });
  await expect(page.getByRole("alert")).toHaveText("Invalid message kit JSON.");
  await expect(page.getByLabel("Kit name")).toHaveValue("");
  const payload = {
    name: "Phone",
    content: "x".repeat(2000),
    embeds: [],
    components: Array.from({ length: 25 }, () => ({
      type: "button",
      label: "x".repeat(80),
      url: null,
      response: "OK",
    })),
    allow_mass_mentions: true,
  };
  await file.setInputFiles({
    name: "phone.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(payload)),
  });
  await expect(page.getByLabel("Kit name")).toHaveValue("Phone");
  await expect(page.getByLabel("Admin allows @everyone and role pings from this kit")).not.toBeChecked();
  await expect(page.locator(".studio-component-preview")).toHaveCount(5);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  // Exercise a client-side route change, without reloading the document.
  await page.evaluate((id) => {
    window.history.pushState({}, "", `/g/${id}/message-studio`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, other);
  await expect(page.getByLabel("Kit name")).toHaveValue("");
  await expect(page.getByRole("button", { name: "Save kit", exact: true })).toBeDisabled();
});

test("a late preview response cannot replace the current draft preview", async ({ page }) => {
  await devLogin(page, `${run}8`, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let started = () => {};
  const requested = new Promise<void>((resolve) => {
    started = resolve;
  });
  await page.route("**/message-studio/preview", async (route) => {
    const content = route.request().postDataJSON().payload.content;
    if (content === "Slow") {
      started();
      await held;
    }
    await route.fulfill({ json: { content, embeds: [], components: [], allowed_mentions: "none" } });
  });
  await page.goto(`/g/${guild}/message-studio`);
  await page.getByLabel("Kit name").fill("Race");
  await page.getByLabel("Content", { exact: true }).fill("Slow");
  await requested;
  await page.getByLabel("Content", { exact: true }).fill("Fast");
  await expect(page.getByRole("region", { name: "Message preview" })).toContainText("Fast");
  const response = page.waitForResponse(
    (res) => res.url().endsWith("/preview") && res.request().postDataJSON().payload.content === "Slow",
  );
  release();
  await response;
  await expect(page.getByRole("region", { name: "Message preview" })).toContainText("Fast");
});
