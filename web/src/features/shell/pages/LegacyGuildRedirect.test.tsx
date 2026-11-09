import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter, Navigate, Route, Routes } from "react-router";
import { expect, it, vi } from "vitest";
import { router } from "../../../router";

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return {
    ...actual,
    createBrowserRouter: (routes: unknown) => ({ routes }),
    Navigate: vi.fn(() => null),
  };
});

import { legacyGuildTarget } from "./LegacyGuildRedirect";

it("redirects the old community URL within the same guild and preserves search and hash", () => {
  expect(legacyGuildTarget("123", "community-engagement", "?tab=welcome", "#editor")).toBe(
    "/g/123/community-engagement?tab=welcome#editor",
  );
});

it("redirects the old moderation URL within the same guild and preserves search and hash", () => {
  expect(legacyGuildTarget("456", "moderation-desk", "?queue=appeals", "#case")).toBe(
    "/g/456/moderation-desk?queue=appeals#case",
  );
});

it.each([
  ["123", "community-pack", "community-engagement", "?tab=welcome", "#editor"],
  ["456", "mod-parity", "moderation-desk", "?queue=appeals", "#case"],
])("redirects the mounted %s/%s route once with replace", (guild, legacy, target, search, hash) => {
  const guildRoute = router.routes
    .flatMap((route) => route.children ?? [])
    .find((route) => route.path === "/g/:guildId");
  expect(guildRoute).toBeDefined();
  const oldRoute = guildRoute!.children!.find((route) => route.path === legacy)!;
  const newRoute = guildRoute!.children!.find((route) => route.path === target)!;
  expect(oldRoute.element).toBeDefined();
  expect(newRoute.lazy).toBeTypeOf("function");
  expect(newRoute.element).toBeUndefined();
  vi.mocked(Navigate).mockClear();
  renderToStaticMarkup(
    <MemoryRouter initialEntries={[`/g/${guild}/${legacy}${search}${hash}`]}>
      <Routes>
        <Route path={`/g/:guildId/${legacy}`} element={oldRoute.element} />
      </Routes>
    </MemoryRouter>,
  );
  expect(Navigate).toHaveBeenCalledTimes(1);
  expect(Navigate).toHaveBeenCalledWith(
    expect.objectContaining({ to: `/g/${guild}/${target}${search}${hash}`, replace: true }),
    undefined,
  );
});
