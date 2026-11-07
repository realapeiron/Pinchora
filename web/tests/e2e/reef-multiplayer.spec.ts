// Real sessions and a real room actor. Run with the regular Playwright config, not mocked Reef fixtures.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { devLogin } from "./helpers";

test.setTimeout(90_000);

const HOST = "770000000000000091";
const GUEST = "770000000000000092";
test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "reef", "seed_reef_multiplayer", "--", "--ignored"],
    { cwd: fileURLToPath(new URL("../../..", import.meta.url)), stdio: "inherit" },
  );
});
async function walk(page: Page, x: number, y: number) {
  await page.getByLabel("Tile X", { exact: true }).fill(String(x));
  await page.getByLabel("Tile Y", { exact: true }).fill(String(y));
  await page.getByRole("button", { name: "Walk to tile", exact: true }).click();
  await expect(page.locator(".reef-canvas")).toHaveAttribute("data-tile", `${x},${y}`);
  await expect(page.locator(".reef-canvas")).toHaveAttribute("data-state", "idle");
}
test("two players see saved characters, walk, contend for a seat and reconnect", async ({
  browser,
  baseURL,
}) => {
  const hostContext = await browser.newContext({ baseURL, viewport: { width: 1280, height: 800 } });
  const guestContext = await browser.newContext({ baseURL, viewport: { width: 1280, height: 800 } });
  try {
    const host = await hostContext.newPage();
    const guest = await guestContext.newPage();
    await devLogin(host, HOST, [], "Reef host");
    await devLogin(guest, GUEST, [], "Reef guest");
    await host.goto("/me/house?play=1");
    await guest.goto(`/house/${HOST}?play=1`);
    for (const page of [host, guest]) {
      await expect(page.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
      await expect(page.locator(".reef-canvas")).toHaveAttribute("data-players", "2");
    }
    await expect(host.locator(`[data-player-id="${GUEST}"]`)).toHaveText("Reef guest");
    await expect(guest.locator(`[data-player-id="${HOST}"]`)).toHaveText("Reef host");
    await walk(host, 1, 7);
    await host.getByRole("button", { name: "Sit", exact: true }).last().click();
    await expect(host.locator(".reef-canvas")).toHaveAttribute("data-state", "sit");
    await walk(guest, 1, 7);
    await guest.getByRole("button", { name: "Sit", exact: true }).last().click();
    await expect(guest.locator(".reef-status")).toContainText("Someone is already sitting there.");
    await walk(host, 0, 7);
    await guest.getByRole("button", { name: "Sit", exact: true }).last().click();
    await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-state", "sit");

    // Reload closes the old connection and gets a new one-use ticket and snapshot.
    await guest.reload();
    await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-network", "connected");
    await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-players", "2");

    // A second tab for the same session becomes the sole controller; the old tab stays a preview.
    const replacement = await guestContext.newPage();
    await replacement.goto(`/house/${HOST}?play=1`);
    await expect(replacement.locator(".reef-canvas")).toHaveAttribute("data-network", "connected", {
      timeout: 15_000,
    });
    await expect(guest.locator(".reef-canvas")).toHaveAttribute("data-network", "replaced");
    await expect(guest.locator(".reef-status")).toContainText("This account is playing in another tab.");
    await expect(guest.getByRole("button", { name: "Take over here" })).toBeVisible();
    await expect(host.locator(".reef-canvas")).toHaveAttribute("data-players", "2");
  } finally {
    await hostContext.close();
    await guestContext.close();
  }
});
