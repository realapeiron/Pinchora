import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { TicketSettings, TicketsOverview } from "../api/tickets";
import { ticketSettingsSummary, ticketsSummary } from "./ticketsSummary";

const settings: TicketSettings = {
  open_category_id: null,
  closed_category_id: null,
  log_channel_id: null,
  auto_close_hours: 0,
  dm_transcript: false,
  rating_enabled: false,
  naming: "ticket-{number}",
  mode: "auto",
  max_open_per_user: 1,
};
const empty = {
  settings,
  panels: [],
  summary: { week: { open_now: 0 }, month: { open_now: 0 } },
} as unknown as TicketsOverview;

test("ticket summaries handle empty lists and off settings", async () => {
  await setLocale("en");
  expect(ticketsSummary(empty)).toEqual({ panels: "No ticket panels yet", queue: "No open tickets" });
  expect(ticketSettingsSummary(settings)).toEqual({
    placement: "No category",
    transcripts: "Off",
    ratings: "Off",
    limits: "1 per member · 0 h",
  });
});

test("ticket summaries use saved panel and queue counts and draft settings", async () => {
  await setLocale("en");
  expect(
    ticketsSummary({
      ...empty,
      panels: [{}, {}],
      summary: { ...empty.summary, week: { ...empty.summary.week, open_now: 6 } },
    } as TicketsOverview),
  ).toEqual({
    panels: "2 ticket panels",
    queue: "6 open tickets",
  });
  expect(
    ticketSettingsSummary({
      ...settings,
      open_category_id: "10",
      dm_transcript: true,
      rating_enabled: true,
      max_open_per_user: 3,
      auto_close_hours: 24,
    }),
  ).toEqual({
    placement: "Category selected",
    transcripts: "On",
    ratings: "On",
    limits: "3 per member · 24 h",
  });
});
