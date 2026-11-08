import { test, expect } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";

test("workflow editor: build, reorder, dry run, variables and persisted history", async ({ page }) => {
  const guild = `${Date.now()}51`;
  const user = `${Date.now()}52`;
  await devLogin(page, user, [{ id: guild, name: "Workflow Reef", owner: true, permissions: "0" }]);
  const csrf = ((await (await page.request.get("/api/me")).json()) as { csrf: string }).csrf;
  const module = await page.request.put(`/api/guilds/${guild}/modules/birthdays`, {
    data: { enabled: false },
    headers: { "x-csrf-token": csrf, origin: new URL(page.url()).origin },
  });
  expect(module.ok()).toBeTruthy();
  await page.goto(`/g/${guild}/workflows`);
  await expect(page.getByRole("heading", { name: "Workflows", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Workflows", exact: true })).toHaveCount(1);
  await expect(page.getByText(/Message Content is off/)).toBeVisible();
  await page.getByRole("button", { name: "New workflow", exact: true }).click();
  await page.getByLabel("Workflow name").fill("welcome");
  await expect(page.getByLabel("Trigger", { exact: true }).locator('option[value="birthday"]')).toHaveCount(
    0,
  );
  await page.getByLabel("Message", { exact: true }).fill("Hello {user}");
  await page.getByRole("button", { name: "Add action", exact: true }).click();
  await page.getByLabel("Message", { exact: true }).nth(1).fill("Second action");
  await page.getByRole("button", { name: "Move up", exact: true }).last().focus();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Message", { exact: true }).first()).toHaveValue("Second action");
  await page.getByRole("button", { name: "Add condition", exact: true }).click();
  await page.locator(".wf-columns").getByRole("button", { name: "Remove", exact: true }).first().click();
  await expect(page.locator(".wf-step legend").filter({ hasText: /Condition/ })).toHaveCount(0);
  const saving = page.waitForRequest(
    (request) => request.method() === "POST" && request.url().endsWith(`/guilds/${guild}/workflows`),
  );
  await page.getByRole("button", { name: "Save workflow", exact: true }).click();
  expect(Object.keys((await saving).postDataJSON() as object).sort()).toEqual([
    "actions",
    "conditions",
    "enabled",
    "name",
    "trigger",
  ]);
  await expect(page.getByText("Workflow saved.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Save workflow", exact: true })).toBeDisabled();
  await page.getByLabel("Message", { exact: true }).first().fill("Changed");
  await expect(page.getByRole("button", { name: "Save workflow", exact: true })).toBeEnabled();
  await page.getByLabel("Message", { exact: true }).first().fill("Second action");
  await expect(page.getByRole("button", { name: "Save workflow", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Test run", exact: true }).click();
  await expect(page.getByRole("region", { name: "Playback" })).toBeVisible();
  await page.getByRole("button", { name: /Action 2/ }).click();
  await expect(page.getByText("No runs in the last seven days.")).toBeVisible();
  await page.getByLabel("Variable key").fill("visits");
  await page.getByLabel("Variable value").fill("1");
  await page.getByRole("button", { name: "Save variable", exact: true }).click();
  await expect(page.getByText("{var.server.visits}", { exact: true })).toBeVisible();
  await page.getByLabel("Variable key").fill("visits");
  await page.getByLabel("Variable value").fill("1");
  await expect(page.getByRole("button", { name: "Save variable", exact: true })).toBeDisabled();
  await page.getByLabel("Scope", { exact: true }).selectOption("member");
  await page.getByLabel("Subject ID (blank: you)").fill(`${user}1`);
  await page.getByLabel("Variable key").fill("visits");
  await page.getByLabel("Variable value").fill("4");
  await page.getByRole("button", { name: "Save variable", exact: true }).click();
  await expect(page.getByText("{var.member.visits}", { exact: true })).toBeVisible();
  const rows = (await (await page.request.get(`/api/guilds/${guild}/workflows`)).json()) as {
    items: { id: string }[];
  };
  const workflow = rows.items[0]!.id;
  // Synthetic completed history exercises playback; no live workflow is executed by this browser test.
  await page.route(`**/api/guilds/${guild}/workflows/${workflow}/runs`, (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: "1",
            started_at: "2026-11-08T12:00:00Z",
            outcome: "completed",
            steps: [
              { phase: "trigger", index: 0, status: "passed", detail: { kind: "command" } },
              { phase: "action", index: 0, status: "done", detail: { kind: "reply", text: "Recorded step" } },
            ],
          },
        ],
      },
    }),
  );
  await page.reload();
  await page.getByRole("button", { name: "welcome", exact: true }).click();
  await page
    .locator("summary")
    .filter({ hasText: /Completed/ })
    .click();
  await page.getByRole("button", { name: "Action 1 · Done", exact: true }).click();
  await expect(page.locator("pre").filter({ hasText: "Recorded step" })).toBeVisible();
  await shot(page, "workflows-desktop");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel("Workflow name").fill("W".repeat(40));
  await page.getByLabel("Variable value").fill("x".repeat(200));
  await overflow(page);
  await shot(page, "workflows-mobile");
  await page.reload();
  await page.getByRole("button", { name: "welcome", exact: true }).click();
  await expect(page.getByLabel("Message", { exact: true }).first()).toHaveValue("Second action");
  await page.getByRole("button", { name: "Delete workflow", exact: true }).click();
  await expect(page.getByText("No workflows yet. Create one to get started.")).toBeVisible();
});

test("Turkish workflow editor has localized controls and intent notice", async ({ page }) => {
  const guild = `${Date.now()}61`;
  const user = `${Date.now()}62`;
  await devLogin(page, user, [{ id: guild, name: "İş Akışı Resifi", owner: true, permissions: "0" }]);
  await page.goto(`/g/${guild}/workflows`);
  await page.getByRole("button", { name: "TR", exact: true }).click();
  await expect(page.getByRole("heading", { name: "İş akışları", exact: true })).toBeVisible();
  await expect(page.getByText(/Mesaj İçeriği kapalı/)).toBeVisible();
  await page.getByRole("button", { name: "Yeni iş akışı", exact: true }).click();
  await page.getByLabel("İş akışı adı").fill("deneme");
  await page.getByLabel("Mesaj", { exact: true }).fill("Merhaba {user}");
  await page.getByRole("button", { name: "Deneme çalıştırması", exact: true }).click();
  await expect(page.getByRole("region", { name: "Adım adım izleme" })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await overflow(page);
});
