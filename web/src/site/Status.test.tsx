import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { StatusPage, type PublicStatus } from "./Status";

const status: PublicStatus = {
  servers: 1234,
  approximate_members: 54321,
  players: 28000,
  channels: 8000,
  shards: [{ id: 3, total: 8, latency_ms: 42, guilds: 100 }],
  uptime_seconds: 3660,
  version: "e814163 · 2026-11-07",
  rust_version: "rustc 1.91.0",
  rss_mb: 480,
  postgres_mb: 1200,
  redis_mb: 56,
  commands_24h: 12000,
  commands_total: 300000,
  interactions_per_minute: 21,
  active_games: 17,
  open_tickets: 9,
  invite_url: "/invite",
  support_url: "",
  website_url: "https://pinchora.test",
  status_url: "/status",
};

describe("status page", () => {
  it.each([
    ["en", "1,234", "Approx. members"],
    ["tr", "1.234", "Yaklaşık üye"],
  ])("shows the %s status with localized numbers", (lang, number, label) => {
    const previous = Object.getOwnPropertyDescriptor(globalThis, "document");
    Object.assign(globalThis, { document: { documentElement: { lang } } });
    const client = new QueryClient();
    client.setQueryData(["/api/public/status"], status);
    const html = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <StatusPage />
      </QueryClientProvider>,
    );
    expect(html).toContain(number);
    expect(html).toContain(label);
    expect(html).toContain("42 ms");
    expect(html).toContain("/invite");
    if (previous) Object.defineProperty(globalThis, "document", previous);
    else Reflect.deleteProperty(globalThis, "document");
  });
});
