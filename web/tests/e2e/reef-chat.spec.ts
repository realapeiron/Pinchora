// Lead runs against the assigned real Postgres/Redis services and dev-login server.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { devLogin } from "./helpers";
const HOST = "770000000000000091",
  GUEST = "770000000000000092";
test.setTimeout(90_000);
test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "reef", "seed_reef_multiplayer", "--", "--ignored"],
    { cwd: fileURLToPath(new URL("../../..", import.meta.url)), stdio: "inherit" },
  );
});
async function enable(page: Page) {
  const button = page.getByRole("button", { name: "Enable chat", exact: true });
  if (await button.isVisible()) {
    await page.getByRole("checkbox", { name: /I am 13 or older/ }).check();
    await button.click();
  }
  await expect(page.getByLabel("Chat transcript")).toBeVisible();
}
test("two contexts chat, report with receipt, moderate and block", async ({ browser, baseURL }) => {
  const a = await browser.newContext({ baseURL, locale: "en-US" }),
    b = await browser.newContext({ baseURL, locale: "en-US" });
  try {
    const host = await a.newPage(),
      guest = await b.newPage();
    await devLogin(host, HOST, [], "Reef host");
    await devLogin(guest, GUEST, [], "Reef guest");
    await host.goto("/me/house?play=1");
    await guest.goto(`/house/${HOST}?play=1`);
    for (const page of [host, guest]) {
      await expect(page.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
      await enable(page);
    }
    const text = `Hello reef ${Date.now().toString(36)}`;
    await guest.getByLabel("Message", { exact: true }).fill(text);
    await guest.getByRole("button", { name: "Send", exact: true }).click();
    const line = host.getByLabel("Chat transcript").getByRole("article").filter({ hasText: text });
    await expect(line).toContainText("delivered");
    await expect(host.locator(".reef-chat-bubble").filter({ hasText: text })).toBeVisible();
    await line.getByRole("button", { name: "Report", exact: true }).click();
    await expect(host.getByRole("form", { name: "Report message" })).toContainText(text);
    await host.getByRole("button", { name: "Submit report" }).click();
    await expect(host.locator(".reef-chat")).toContainText("Report receipt");
    const queue = await a.newPage();
    await queue.goto("/reef/moderation");
    await expect(queue.getByRole("article").filter({ hasText: text })).toBeVisible();
    await line.getByRole("button", { name: "Block", exact: true }).click();
    await expect(guest.locator(".reef-status")).toContainText("You can no longer enter this house.");
    await expect(host.getByLabel("Chat transcript")).not.toContainText(text);
    await guest.reload();
    await expect(guest.locator(".reef-canvas")).toHaveCount(0);
  } finally {
    await a.close();
    await b.close();
  }
});
