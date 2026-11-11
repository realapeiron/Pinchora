import { expect, test } from "@playwright/test";

import { devLogin, openRow, overflow, shot } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}5`;
const user = `${stamp}6`;

test("Messages editor saves a schedule and fits desktop and phone", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/utility`);
  await expect(page.getByRole("heading", { name: "MESSAGES", exact: true })).toBeVisible();
  await expect(page.getByText("Scheduled messages · 0/20 used")).toBeVisible();
  await openRow(page.locator(".settings-detail-expand").filter({ hasText: "Scheduled messages" }));
  await page.getByRole("button", { name: "Schedule a message" }).click();
  await page.getByLabel("Channel").selectOption({ label: "#general" });
  await page.getByLabel("Message", { exact: true }).fill("Reef cleanup starts soon.");
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await expect(page.getByRole("region", { name: "Message preview" })).toContainText(
    "Reef cleanup starts soon.",
  );
  await page.getByRole("button", { name: "Close preview" }).click();
  await page.getByRole("button", { name: "Schedule", exact: true }).click();
  await expect(page.getByText("Reef cleanup starts soon.").first()).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Scheduled message saved." })).toBeVisible();
  await expect(page.getByText("Scheduled messages · 1/20 used")).toBeVisible();
  const scheduledSaved = await (await page.request.get(`/api/guilds/${guild}/utility`)).json();
  expect(scheduledSaved.scheduled).toEqual(
    expect.arrayContaining([expect.objectContaining({ content: "Reef cleanup starts soon." })]),
  );
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await page.getByLabel("Message", { exact: true }).fill("Reef cleanup starts at noon.");
  await page.getByRole("button", { name: "Save schedule" }).click();
  await expect(page.getByText("Reef cleanup starts at noon.").first()).toBeVisible();
  await shot(page, "utility-desktop", true);
  await page.getByRole("button", { name: "Preview 5 runs" }).click();
  await expect(page.locator(".settings-detail-row .settings-detail-preview")).toContainText(
    "Reef cleanup starts at noon.",
  );
  await expect(page.getByText("NEXT RUNS")).toBeVisible();
  await page.getByRole("button", { name: "Close preview" }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await shot(page, "utility-phone", true);
});
