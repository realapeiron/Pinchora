import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { Counters } from "../api/stats";
import { statsSummary } from "./statsSummary";

test("stats rows describe private, public and counter states", async () => {
  await setLocale("en");
  expect(statsSummary(undefined, false)).toEqual({
    counters: "No counter channels",
    publicPage: "Off",
    card: "Card ready to preview",
  });
  expect(statsSummary({ counters: [{}, {}] } as Counters, true)).toEqual({
    counters: "2 counter channels",
    publicPage: "On",
    card: "Card ready to preview",
  });
  expect(statsSummary(undefined, false, 1).card).toBe("Card preview loaded");
});
