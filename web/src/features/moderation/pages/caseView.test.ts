import { expect, test } from "vitest";
import type { Case, Rung } from "../../../lib/api";
import { caseFromSearch, kindCounts, nextWarningRung, searchWithCase, visibleKinds } from "./caseView";

const entry = (kind: string, active: boolean): Case => ({
  number: 4,
  kind,
  label: kind,
  on_channel: false,
  target_id: "1",
  target_name: "Mia",
  moderator_id: "2",
  moderator_name: "Mod",
  reason: null,
  duration: null,
  expires_at: null,
  details: null,
  active,
  dm_sent: null,
  created_at: "2025-01-01T00:00:00Z",
  updated_at: null,
});
const ladder: Rung[] = [
  { warns: 3, action: "timeout", duration: "1h", duration_secs: 3600, label: "1h timeout" },
  { warns: 5, action: "kick", duration: null, duration_secs: null, label: "kick" },
];

test("case URL state preserves other query parameters and rejects invalid numbers", () => {
  expect(caseFromSearch("?q=spam&case=142")).toBe(142);
  expect(caseFromSearch("?case=0")).toBeNull();
  expect(caseFromSearch("?case=abc")).toBeNull();
  expect(searchWithCase("q=spam&case=1", 142)).toBe("q=spam&case=142");
  expect(searchWithCase("q=spam&case=142", null)).toBe("q=spam");
});

test("kind totals come from server counts", () => {
  expect(
    kindCounts([
      { key: "warn", label: "Warn", count: 4 },
      { key: "ban", label: "Ban", count: 2 },
    ]),
  ).toBe(6);
});

test("warning hint counts active warnings and chooses the next rung", () => {
  expect(nextWarningRung(entry("warn", true), [entry("warn", true), entry("warn", false)], ladder)).toEqual({
    active: 2,
    rung: ladder[0],
  });
  expect(nextWarningRung(entry("warn", true), [entry("warn", true)], undefined)).toBeNull();
  expect(nextWarningRung(entry("ban", true), [], ladder)).toBeNull();
  expect(nextWarningRung(entry("warn", true), Array(4).fill(entry("warn", true)), ladder)).toBeNull();
});

test("kind tabs show kinds with cases and keep the chosen one when it has none", () => {
  const kinds = [
    { key: "warn", label: "Warn", count: 2 },
    { key: "note", label: "Note", count: 0 },
    { key: "ban", label: "Ban", count: 1 },
  ];
  expect(visibleKinds(kinds, "").map((kind) => kind.key)).toEqual(["warn", "ban"]);
  expect(visibleKinds(kinds, "note").map((kind) => kind.key)).toEqual(["warn", "note", "ban"]);
});
