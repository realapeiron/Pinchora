import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { devLogin, overflow, shot } from "./helpers";
// The lead runs Chromium. The existing house fixture supplies the room and atlas;
// this response extension exercises the new surface without a mutation endpoint.
test.beforeAll(() => {
  execFileSync(
    "cargo",
    ["test", "-p", "pinchora-web", "--test", "house", "seed_demo_house", "--", "--ignored"],
    {
      cwd: fileURLToPath(new URL("../../..", import.meta.url)),
      stdio: "inherit",
    },
  );
});
for (const width of [1280, 390]) {
  test(`reef aquarium and garden at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await devLogin(page, "770000000000000001", []);
    await page.route("**/api/house", async (route) => {
      const response = await route.fetch();
      const body = await response.json();
      body.reef_loop = {
        name: "Mavi Yuva",
        fish: [
          {
            slot: 1,
            id: "toaststripe_minnow",
            name: "Toaststripe Minnow",
            name_tr: "Kızarmış Çizgili Balık",
            size_cm: 7,
          },
        ],
        garden: { seaweed: 12, planted_at: null, watered_at: null },
      };
      await route.fulfill({ response, json: body });
    });
    await page.goto("/me/house?mode=decor");
    await expect(page.getByRole("heading", { name: "Mavi Yuva" })).toBeVisible();
    await expect(page.locator(".reef-home-fish canvas")).toBeVisible();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    await shot(page, `reef-loop-${width}`);
  });
}
