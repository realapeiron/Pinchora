import { describe, expect, it } from "vitest";
import {
  auditDiff,
  auditParams,
  canOpenPage,
  canonicalGrants,
  diffText,
  EMPTY_FILTERS,
  type AccessOverview,
} from "./server-tools";

describe("dashboard delegation", () => {
  it("fails closed until the server supplies current access metadata", () => {
    expect(canOpenPage(undefined, "roles")).toBe(false);
    expect(canOpenPage({} as AccessOverview, "access")).toBe(false);
  });
  const access = { access: { full: false, areas: ["roles"] } } as AccessOverview;
  it("keeps the shell and granted area, denies unknown, adjacent and privileged pages", () => {
    expect(canOpenPage(access, "")).toBe(true);
    expect(canOpenPage(access, "roles")).toBe(true);
    for (const page of ["access", "backup", "command-control", "role", "audit", "new-feature", "leveling"])
      expect(canOpenPage(access, page)).toBe(false);
    expect(canOpenPage({ access: { full: true, areas: null } } as AccessOverview, "access")).toBe(true);
  });
  it("strips read-only properties from access payloads", () => {
    const row = { role_id: "123", areas: ["roles"], name: "Read-only", full: true };
    expect(canonicalGrants([row])).toEqual([{ role_id: "123", areas: ["roles"] }]);
  });
  it("opens Message Studio only for configuration delegates or full managers", () => {
    const configuration = { access: { full: false, areas: ["configuration"] } } as AccessOverview;
    for (const path of ["message-studio", "message-studio/greeting"]) {
      expect(canOpenPage(configuration, path)).toBe(true);
      expect(canOpenPage(access, path)).toBe(false);
      expect(canOpenPage({ access: { full: true, areas: null } } as AccessOverview, path)).toBe(true);
    }
    expect(canOpenPage(configuration, "message-studio-extra")).toBe(false);
  });
  it("compares grants without ordering noise and keeps snowflakes as strings", () => {
    const id = "999999999999999999";
    expect(canonicalGrants([{ role_id: id, areas: ["roles", "audit", "roles"] }])).toEqual([
      { role_id: id, areas: ["audit", "roles"] },
    ]);
  });
});
describe("audit filters and nested diffs", () => {
  const names = {
    channel: (id: string) => (id === "7001" ? "#general" : `channel ${id}`),
    role: (id: string) => (id === "9001" ? "@Regulars" : `role ${id}`),
    module: (key: string) => key,
  };
  it("resolves Discord IDs in changed fields, including nested settings", () => {
    const lines = auditDiff(
      { channels: [], settings: { welcome_channel_id: null } },
      { channels: ["7001"], settings: { welcome_channel_id: "7001" } },
    );
    expect(lines.map((line) => diffText(line.after, line.path, names))).toEqual(["#general", "#general"]);
    expect(diffText("9001", "join_role_id", names)).toBe("@Regulars");
    expect(diffText(["9001"], "allow_roles", names)).toBe("@Regulars");
    expect(diffText(["7001"], "deny_channels", names)).toBe("#general");
    expect(diffText("only", "channel_mode", names)).toBe("only some channels");
    expect(diffText(false, "enabled", names)).toBe("off");
  });
  it("retains structured details, missing values and unknown Discord IDs", () => {
    const grants = [{ role_id: "9001", areas: ["roles"] }];
    expect(diffText(grants, "roles", names)).toBe(JSON.stringify(grants, null, 2));
    expect(diffText({ title: "Hello" }, "embed", names)).toContain('"title": "Hello"');
    expect(diffText(undefined, "channels", names)).toBe("—");
    expect(diffText(null, "channel_id", names)).toBe("—");
    expect(diffText([], "channels", names)).toBe("none");
    expect(diffText("7777", "channel_id", names)).toBe("channel 7777");
    expect(diffText("7001", "message", names)).toBe("7001");
  });
  it("includes the last calendar day across month and year boundaries in UTC", () => {
    const params = auditParams({
      ...EMPTY_FILTERS,
      from: "2026-12-01",
      until: "2026-12-31",
      text: "  100%_literal  ",
    });
    expect(params.get("from")).toBe("2026-12-01T00:00:00.000Z");
    expect(params.get("until")).toBe("2027-01-01T00:00:00.000Z");
    expect(params.get("text")).toBe("100%_literal");
    expect(params.has("actor")).toBe(false);
  });
  it("shows nested changes, deleted fields and arrays without unchanged noise", () => {
    expect(
      auditDiff({ a: { keep: 1, remove: 2 }, roles: ["one"] }, { a: { keep: 1 }, roles: ["two"] }),
    ).toEqual([
      { path: "a.remove", before: 2, after: undefined },
      { path: "roles", before: ["one"], after: ["two"] },
    ]);
    expect(auditDiff({ value: { nested: true } }, { value: "disabled" })).toEqual([
      { path: "value", before: { nested: true }, after: "disabled" },
    ]);
    expect(auditDiff(null, { enabled: true })).toEqual([{ path: "enabled", before: undefined, after: true }]);
  });
});
it("requires the economy grant for the economy dashboard", () => {
  const grant = (area: string) => ({ access: { full: false, areas: [area] } }) as AccessOverview;
  expect(canOpenPage(grant("economy"), "economy")).toBe(true);
  expect(canOpenPage(grant("configuration"), "economy")).toBe(false);
  expect(canOpenPage(grant("economy"), "economy-extra")).toBe(false);
});

it.each([
  ["community", "community-pack", "community-engagement"],
  ["safety", "mod-parity", "moderation-desk"],
])("preserves %s delegation across the legacy and renamed URLs", (area, legacy, current) => {
  const grant = (areas: string[]) => ({ access: { full: false, areas } }) as AccessOverview;
  for (const page of [legacy, current, `${current}/details`]) {
    expect(canOpenPage(grant([area]), page)).toBe(true);
    expect(canOpenPage(grant([area === "safety" ? "community" : "safety"]), page)).toBe(false);
    expect(canOpenPage(grant([]), page)).toBe(false);
    expect(canOpenPage(undefined, page)).toBe(false);
  }
  expect(canOpenPage(grant([area]), `${current}-extra`)).toBe(false);
});
