import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

const run = Date.now();
const guild = `${run}8`;
const user = `${run}9`;

test("create, preview, edit and duplicate an event at desktop and phone widths", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/events`);
  await expect(page.getByRole("heading", { name: "EVENTS", exact: true })).toBeVisible();
  await expect(page.getByText("No events on the calendar yet.")).toBeVisible();
  await shot(page, "events-empty-desktop", true);
  await page.getByRole("button", { name: "Create event" }).click();
  const editor = page.getByRole("dialog", { name: "Create event" });
  await editor.getByLabel("Title").fill("Crab night");
  await editor.getByLabel("Description").fill("Meet at the reef and bring a friend.");
  await editor.getByLabel("Post RSVP card in").selectOption({ label: "#general" });
  await editor.getByLabel("Going capacity").fill("12");
  await editor.getByRole("button", { name: "Preview" }).click();
  await expect(editor.getByText("Crab night")).toBeVisible();
  await shot(page, "events-editor-desktop", true);
  await editor.getByRole("button", { name: "Create and post" }).click();
  await expect(page.getByRole("status")).toContainText("Event created and posted.");
  await expect(page.getByRole("heading", { name: "Crab night" })).toBeVisible();
  const list = await (await page.request.get(`/api/guilds/${guild}/events`)).json();
  expect(list.events[0].title).toBe("Crab night");
  expect(list.events[0].channel_id).toBe("7001");
  await page.getByRole("button", { name: "Attendees" }).click();
  await expect(page.getByRole("dialog", { name: "Event attendees" })).toContainText(
    "Nobody has answered yet.",
  );
  await page.getByRole("dialog", { name: "Event attendees" }).getByRole("button", { name: "Close" }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await shot(page, "events-list-phone", true);
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await page.getByRole("button", { name: "Edit" }).click();
  await page.getByRole("dialog", { name: "Edit event" }).getByLabel("Title").fill("Moonlit crab night");
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("heading", { name: "Moonlit crab night" })).toBeVisible();
  await page.getByRole("button", { name: "Duplicate" }).click();
  await expect(page.getByRole("dialog", { name: "Create event" })).toBeVisible();
  // A page can fit the viewport while its scrollable dialog still clips form controls.
  const duplicate = page.getByRole("dialog", { name: "Create event" });
  expect(await duplicate.evaluate((el) => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
  const dialogBounds = await duplicate.boundingBox();
  for (const input of await duplicate.locator("input, textarea, select, button").all()) {
    const bounds = await input.boundingBox();
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(dialogBounds!.x + dialogBounds!.width);
  }
  await shot(page, "events-editor-phone", true);
});
