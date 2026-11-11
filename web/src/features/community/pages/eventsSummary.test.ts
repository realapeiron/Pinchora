import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { Event } from "../api/events";
import { eventsSummary } from "./eventsSummary";

test("event summaries handle empty and populated calendars", async () => {
  await setLocale("en");
  expect(eventsSummary("UTC", [], [])).toEqual({
    clock: "UTC",
    upcoming: "No events yet",
    history: "No past events",
  });
  expect(
    eventsSummary("Europe/Istanbul", [{ id: "1" }, { id: "2" }] as Event[], [{ id: "3" }] as Event[]),
  ).toEqual({
    clock: "Europe/Istanbul",
    upcoming: "2 upcoming events",
    history: "1 past event",
  });
});
