import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "tests/reef",
  outputDir: "test-results/reef",
  workers: 1,
  timeout: 45000,
  use: {
    baseURL: "http://127.0.0.1:18194",
    browserName: "chromium",
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2.625,
    hasTouch: true,
    trace: "retain-on-failure",
  },
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
