import { expect, test } from "@playwright/test";
test("60 furnishings, 16-person room and 32-person plaza under 4x CPU throttle", async ({
  page,
  context,
}) => {
  test.setTimeout(90000);
  const cdp = await context.newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
  await page.goto("http://127.0.0.1:18195/tests/reef/perf.html");
  await expect(page.locator("body")).toHaveAttribute("data-done", "true", { timeout: 80000 });
  const results = JSON.parse((await page.locator("#result").textContent())!) as {
    avatars: number;
    cpuP95: number;
    fps: number;
    cacheBytes: number;
  }[];
  console.log(`REEF 4x CPU emulation: ${JSON.stringify(results)}`);
  for (const result of results) {
    expect(result.cpuP95).toBeLessThanOrEqual(16.7);
    expect(result.cacheBytes).toBeLessThanOrEqual(16 * 1024 * 1024);
  }
});
