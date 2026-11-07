import { fileURLToPath } from "node:url";

import { defineConfig } from "@playwright/test";

// Runs the real Rust server (`pinchora web`, debug build) against the test Postgres and Redis, with
// Discord replaced by tests/mock-discord.ts and dev login switched on. Build first:
//   cargo build -p pinchora && npm run build
//   PINCHORA_TEST_DATABASE_URL=... PINCHORA_TEST_REDIS_URL=... npm run e2e

const port = Number(process.env.E2E_PORT ?? 18190);
const mockPort = Number(process.env.E2E_MOCK_PORT ?? 18191);
const opsPort = Number(process.env.E2E_OPS_PORT ?? 18192);
const feedPort = Number(process.env.E2E_FEED_PORT ?? 18193);
const database = process.env.PINCHORA_TEST_DATABASE_URL;
const redis = process.env.PINCHORA_TEST_REDIS_URL;
if (!database || !redis) {
  throw new Error("Set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL to run the e2e tests.");
}
const binary =
  process.env.PINCHORA_BIN ?? fileURLToPath(new URL("../target/debug/pinchora", import.meta.url));
const dist = fileURLToPath(new URL("./dist", import.meta.url));

export default defineConfig({
  testDir: "tests/e2e",
  outputDir: "test-results",
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1280, height: 800 },
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
  webServer: [
    {
      command: "node --experimental-strip-types tests/mock-discord.ts",
      url: `http://127.0.0.1:${mockPort}/health`,
      env: { MOCK_PORT: String(mockPort) },
      reuseExistingServer: false,
    },
    {
      command: `"${binary}" migrate && exec "${binary}" web`,
      url: `http://127.0.0.1:${port}/api/me`,
      reuseExistingServer: false,
      timeout: 120_000,
      env: {
        PINCHORA_CONFIG: "/nonexistent/pinchora.toml",
        PINCHORA_DISCORD__TOKEN: "e2e-bot-token",
        PINCHORA_DISCORD__APPLICATION_ID: "1",
        PINCHORA_DISCORD__HTTP_PROXY: `127.0.0.1:${mockPort}`,
        PINCHORA_DATABASE__URL: database,
        PINCHORA_REDIS__URL: redis,
        PINCHORA_OPS__BIND: `127.0.0.1:${opsPort}`,
        PINCHORA_LOG__FILTER: "warn,pinchora_web=info",
        PINCHORA_WEB__BIND: `127.0.0.1:${port}`,
        PINCHORA_WEB__PUBLIC_URL: `http://127.0.0.1:${port}`,
        PINCHORA_WEB__DEV_LOGIN: "true",
        PINCHORA_WEB__DISCORD_API_BASE: `http://127.0.0.1:${mockPort}/api/v10`,
        PINCHORA_WEB__STATIC_DIR: dist,
        PINCHORA_FEEDS__DEV_ENDPOINT: `127.0.0.1:${feedPort}`,
      },
    },
  ],
});
