import { expect, test, type Page } from "@playwright/test";
import { devLogin, overflow } from "./helpers";

const stamp = Date.now();
const guild = `${stamp}1`;
const user = `${stamp}2`;

async function openRow(page: Page, title: string) {
  const row = page
    .locator(".settings-detail-row")
    .filter({ has: page.locator(".settings-detail-copy strong", { hasText: title }) });
  const button = row.locator(".settings-detail-expand");
  await button.click();
  await expect(button).toHaveAttribute("aria-expanded", "true");
}

test("community tools pages and personal lists work at desktop and phone widths", async ({ page }) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);

  const pages = [
    ["giveaways", "GIVEAWAYS"],
    ["polls", "POLLS"],
    ["suggestions", "SUGGESTIONS"],
    ["reminders", "REMINDERS & CHECKLISTS"],
    ["highlights", "KEYWORD HIGHLIGHTS"],
    ["auto-channels", "AUTO CHANNELS"],
    ["anniversaries", "MEMBER ANNIVERSARIES"],
    ["confessions", "ANONYMOUS CONFESSIONS"],
  ] as const;
  for (const [path, title] of pages) {
    await page.goto(`/g/${guild}/${path}`);
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expect(page.locator(".settings-detail-row").first()).toBeVisible();
  }

  await page.goto(`/g/${guild}/reminders`);
  await page.getByLabel("Reminder text").fill("Check the reef");
  await expect(page.getByRole("button", { name: "Add reminder" })).toBeEnabled();
  await page.getByRole("button", { name: "Add reminder" }).click();
  await expect(page.getByText("Check the reef")).toBeVisible();
  await openRow(page, "CHECKLIST");
  await page.getByLabel("New task").fill("Welcome newcomers");
  await page.getByRole("button", { name: "Add task" }).click();
  await expect(page.getByText("Welcome newcomers")).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();
  await expect(page.getByText("☑ Welcome newcomers")).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  for (const [path, title] of pages) {
    await page.goto(`/g/${guild}/${path}`);
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
    expect(await overflow(page), path).toBeLessThanOrEqual(0);
  }
});

test("community forms preserve failed drafts, surface review errors and send only editable fields", async ({
  page,
}) => {
  await devLogin(page, user, [{ id: guild, name: "Pinchora Reef", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/reminders`);
  await page.getByLabel("Reminder text").fill("Keep this failed draft");
  await page.route(`**/api/guilds/${guild}/community-v2/personal/reminders`, (route) =>
    route.fulfill({ status: 500, json: { error: { code: "internal", message: "Could not save." } } }),
  );
  await page.getByRole("button", { name: "Add reminder" }).click();
  await expect(page.getByRole("status")).toBeVisible();
  await expect(page.getByLabel("Reminder text")).toHaveValue("Keep this failed draft");
  await page.unroute(`**/api/guilds/${guild}/community-v2/personal/reminders`);

  const data = {
    giveaways: [],
    polls: [],
    suggestions: [],
    autopost: [],
    anniversary: { channel_id: null, enabled: false },
    confession_settings: { review_channel_id: null, publish_channel_id: null, enabled: false },
    confessions: [
      {
        id: "7",
        body: "a".repeat(1000),
        status: "pending",
        created_at: new Date().toISOString(),
        reports: 0,
      },
    ],
    message_content: true,
  };
  await page.route(`**/api/guilds/${guild}/community-v2`, (route) => route.fulfill({ json: data }));
  await page.route(`**/api/guilds/${guild}/community-v2/confessions/7/review`, (route) => {
    expect(route.request().postDataJSON()).toEqual({ approve: true });
    return route.fulfill({ status: 500, json: { error: { code: "internal", message: "Could not save." } } });
  });
  await page.goto(`/g/${guild}/confessions`);
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await openRow(page, "REVIEW QUEUE");
  await page.getByRole("button", { name: "Approve", exact: true }).click();
  await expect(page.getByRole("status")).toBeVisible();
  await expect(page.getByRole("button", { name: "Approve", exact: true })).toBeEnabled();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await overflow(page)).toBeLessThanOrEqual(0);
  await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
});
