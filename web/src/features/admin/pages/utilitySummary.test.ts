import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { UtilityList } from "../api/utility";
import { utilitySummary } from "./utilitySummary";

test("message rows describe empty and populated lists", async () => {
  await setLocale("en");
  const empty = { timezone: "UTC", sticky: [], scheduled: [] } as unknown as UtilityList;
  expect(utilitySummary(empty)).toEqual({
    timezone: "UTC",
    sticky: "No sticky messages",
    scheduled: "No scheduled messages",
  });
  expect(utilitySummary({ ...empty, sticky: [{}], scheduled: [{}, {}] } as UtilityList)).toEqual({
    timezone: "UTC",
    sticky: "1 sticky message",
    scheduled: "2 scheduled messages",
  });
});
