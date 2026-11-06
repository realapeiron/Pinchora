import { describe, expect, it } from "vitest";

import { type NavGroup, visibleNav } from "./nav";

const server: NavGroup = {
  group: "SERVER",
  pages: [
    { to: "", label: "Overview" },
    { to: "modules", label: "Modules" },
  ],
};
const safety: NavGroup = { group: "SAFETY", pages: [] };
const tools: NavGroup = { group: "TOOLS", pages: [{ to: "log", label: "Server log" }] };
const nav = [server, safety, tools];

describe("sidebar filter", () => {
  it("shows every page of every filled group when nothing is typed", () => {
    expect(visibleNav(nav, "")).toEqual([server, tools]);
    expect(visibleNav(nav, "   ")).toEqual([server, tools]);
  });

  it("hides a group no feature has added a page to", () => {
    expect(visibleNav(nav, "").map((g) => g.group)).not.toContain("SAFETY");
    expect(visibleNav(nav, "safety")).toEqual([]);
  });

  it("keeps the pages whose name contains the text, in any case", () => {
    expect(visibleNav(nav, "MOD")).toEqual([
      { group: "SERVER", pages: [{ to: "modules", label: "Modules" }] },
    ]);
    expect(visibleNav(nav, "log")).toEqual([tools]);
  });

  it("keeps a whole group when its name matches", () => {
    expect(visibleNav(nav, "server")).toEqual([server, tools]);
  });

  it("finds nothing for text that matches no page", () => {
    expect(visibleNav(nav, "zzz")).toEqual([]);
  });
});
