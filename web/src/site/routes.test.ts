import { describe, expect, it } from "vitest";

import { FEATURES } from "./features";
import { PUBLIC_ROUTES, isAppPath, publicRoute } from "./routes";

describe("public routes", () => {
  it("starts with the front page and lists every page once", () => {
    expect(PUBLIC_ROUTES[0]?.path).toBe("/");
    const paths = PUBLIC_ROUTES.map((r) => r.path);
    expect(new Set(paths).size).toBe(paths.length);
    for (const f of FEATURES) expect(paths).toContain(`/features/${f.slug}`);
    for (const page of ["/commands", "/privacy", "/terms"]) expect(paths).toContain(page);
  });

  it("gives every page a title and description that fit a search result", () => {
    for (const route of PUBLIC_ROUTES) {
      expect(route.title.length, route.path).toBeGreaterThan(10);
      expect(route.title.length, `${route.path} title`).toBeLessThanOrEqual(65);
      expect(route.description.length, `${route.path} description`).toBeGreaterThanOrEqual(60);
      expect(route.description.length, `${route.path} description`).toBeLessThanOrEqual(170);
    }
    const titles = PUBLIC_ROUTES.map((r) => r.title);
    expect(new Set(titles).size, "no two pages share a title").toBe(titles.length);
  });

  it("finds a page with or without a trailing slash", () => {
    expect(publicRoute("/commands/")?.path).toBe("/commands");
    expect(publicRoute("/")?.path).toBe("/");
    expect(publicRoute("/nothing")).toBeUndefined();
  });

  it("knows the dashboard's pages from the site's", () => {
    for (const path of ["/servers", "/login", "/g/123", "/g/123/leveling", "/me/house", "/house/42"]) {
      expect(isAppPath(path), path).toBe(true);
    }
    for (const route of PUBLIC_ROUTES) expect(isAppPath(route.path), route.path).toBe(false);
    expect(isAppPath("/server-tools")).toBe(false);
  });
});
