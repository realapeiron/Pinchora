// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";
import { setLocale } from "../../../i18n";
import { NotificationBell, relativeTime, type NotificationItem } from "./NotificationBell";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const at = new Date(Date.now() - 3 * 3600_000).toISOString();
const item: NotificationItem = {
  id: "ticket:1",
  kind: "ticket",
  at,
  link: "/g/1/tickets",
  data: { number: 1, opener_id: "2" },
};
let root: ReturnType<typeof createRoot> | undefined;
let host: HTMLDivElement;
let client: QueryClient;
async function render(items: NotificationItem[], unread: number, error = false, pending = false) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_path: string, options?: RequestInit) =>
      pending && !options?.method
        ? new Promise<Response>(() => {})
        : new Response(
            JSON.stringify(
              error
                ? { error: { code: "failure", message: "failed" } }
                : options?.method === "POST"
                  ? { read_at: at }
                  : { items, unread, read_at: unread ? null : at },
            ),
            { status: error ? 500 : 200 },
          ),
    ),
  );
  await act(async () =>
    root!.render(
      <MemoryRouter>
        <QueryClientProvider client={client}>
          <NotificationBell guild="1" />
        </QueryClientProvider>
      </MemoryRouter>,
    ),
  );
  await act(async () => {
    await Promise.resolve();
  });
}
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  host?.remove();
  client?.clear();
  vi.unstubAllGlobals();
  root = undefined;
  await setLocale("en");
});

it("shows accessible badge values", async () => {
  for (const count of [0, 3, 12, 25]) {
    if (root) {
      await act(async () => root!.unmount());
      host.remove();
      client.clear();
      root = undefined;
    }
    await render(count ? [item] : [], count);
    await vi.waitFor(() =>
      expect(host.querySelector("button")?.getAttribute("aria-label")).toBe(
        count ? `Notifications, ${count} unread` : "Notifications",
      ),
    );
    expect(host.querySelector(".notification-badge")?.textContent).toBe(
      count ? (count > 9 ? "9+" : String(count)) : undefined,
    );
  }
});

it("marks items read and returns focus on Escape", async () => {
  await render([item], 1);
  const button = host.querySelector("button")!;
  await act(async () => button.click());
  expect(host.querySelector('[role="dialog"]')).not.toBeNull();
  expect(document.activeElement).toBe(host.querySelector('[role="dialog"]'));
  await act(async () => host.querySelector<HTMLButtonElement>(".notification-head button")!.click());
  await act(async () => {
    await Promise.resolve();
  });
  await vi.waitFor(() => expect(button.getAttribute("aria-label")).toBe("Notifications"));
  await act(async () =>
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })),
  );
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("keeps focus in the panel while loading and on error", async () => {
  await render([], 0, false, true);
  await act(async () => host.querySelector("button")!.click());
  const panel = host.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(document.activeElement).toBe(panel);
  expect(panel.getAttribute("aria-labelledby")).toBe(panel.querySelector("strong")?.id);
  const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
  document.dispatchEvent(tab);
  expect(tab.defaultPrevented).toBe(true);
  await act(async () => {
    client.setQueryData(["/api/guilds/1/notifications"], { items: [item], unread: 1, read_at: null });
  });
  await vi.waitFor(() => expect(panel.querySelector(".notification-item")).not.toBeNull());
  expect(document.activeElement).toBe(panel);
  await act(async () => root!.unmount());
  host.remove();
  client.clear();
  root = undefined;
  await render([], 0, true);
  await act(async () => host.querySelector("button")!.click());
  expect(document.activeElement).toBe(host.querySelector('[role="dialog"]'));
});

it("keeps the members summary read and marks only event times", async () => {
  const members: NotificationItem = {
    id: "members:1",
    kind: "members",
    at: new Date().toISOString(),
    link: "/g/1/stats",
    data: { count: 3 },
  };
  await render([members, item], 1);
  await act(async () => host.querySelector("button")!.click());
  expect(host.querySelectorAll(".notification-new")).toHaveLength(1);
  expect(host.querySelector(".notification-item")?.classList.contains("is-unread")).toBe(false);
  await act(async () => host.querySelector<HTMLButtonElement>(".notification-head button")!.click());
  expect(vi.mocked(fetch).mock.calls.find(([, options]) => options?.method === "POST")?.[1]?.body).toContain(
    at,
  );
  await act(async () => root!.unmount());
  host.remove();
  client.clear();
  root = undefined;
  await render([members], 0);
  await act(async () => host.querySelector("button")!.click());
  expect(host.querySelector<HTMLButtonElement>(".notification-head button")?.disabled).toBe(true);
  expect(host.querySelector(".notification-new")).toBeNull();
});

it("updates the bell and open list on a language switch", async () => {
  await setLocale("en");
  await render([item], 1);
  await act(async () => host.querySelector("button")!.click());
  await act(async () => {
    await setLocale("tr");
  });
  expect(host.querySelector(".notification-trigger")?.getAttribute("aria-label")).toBe(
    "Bildirimler, 1 okunmamış",
  );
  const panel = host.querySelector('[role="dialog"]');
  expect(document.getElementById(panel?.getAttribute("aria-labelledby") ?? "")?.textContent).toBe(
    "Bildirimler",
  );
  expect(host.textContent).toContain("Tümünü okundu say");
});

it("shows empty and error states", async () => {
  await render([], 0);
  await act(async () => host.querySelector("button")!.click());
  expect(host.textContent).toContain("Nothing new in the last 7 days.");
  await act(async () => root!.unmount());
  host.remove();
  client.clear();
  root = undefined;
  await render([], 0, true);
  await act(async () => host.querySelector("button")!.click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("Could not load notifications.");
});

it("formats relative time in English and Turkish", () => {
  expect(relativeTime(at, Date.parse(at) + 3 * 3600_000, "en")).toBe("3 hours ago");
  expect(relativeTime(at, Date.parse(at) + 3 * 3600_000, "tr")).toBe("3 saat önce");
});

it("shows known display names and never shows raw member ids", async () => {
  const discordId = "17945356274558";
  const ticket: NotificationItem = {
    ...item,
    data: { number: 1, opener_id: discordId, opener_name: "Aylin" },
  };
  const modmail: NotificationItem = {
    ...item,
    id: "modmail:2",
    kind: "modmail",
    data: { user_id: discordId, user_name: "Deniz" },
  };
  await render([ticket, modmail], 2);
  await act(async () => host.querySelector("button")!.click());
  expect(host.textContent).toContain("Opened by Aylin");
  expect(host.textContent).toContain("Member Deniz");
  expect(host.textContent).not.toContain(discordId);
  await act(async () => root!.unmount());
  host.remove();
  client.clear();
  root = undefined;
  await render(
    [
      { ...ticket, data: { number: 1, opener_id: discordId } },
      { ...modmail, data: { user_id: discordId } },
    ],
    2,
  );
  await act(async () => host.querySelector("button")!.click());
  expect(host.textContent).not.toContain("Opened by");
  expect(host.textContent).not.toContain("Member ");
  expect(host.textContent).not.toContain(discordId);
});
