import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "tests/reef",
  outputDir: "test-results/reef",
  workers: 1,
  timeout: 45000,
  use: {
    baseURL: "http://127.0.0.1:18194",
    browserName: "chromium",
    locale: "en-US",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "phone",
      use: {
        viewport: { width: 390, height: 844 },
        deviceScaleFactor: 2.625,
        hasTouch: true,
        isMobile: true,
      },
    },
    {
      name: "desktop",
      testIgnore: /perf\.spec\.ts/,
      use: { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1, hasTouch: false },
    },
  ],
  webServer: [
    {
      command: "npx vite preview --host 127.0.0.1 --port 18194",
      url: "http://127.0.0.1:18194",
      reuseExistingServer: false,
    },
    {
      command: "npx vite --host 127.0.0.1 --port 18195",
      url: "http://127.0.0.1:18195",
      reuseExistingServer: false,
    },
  ],
});
