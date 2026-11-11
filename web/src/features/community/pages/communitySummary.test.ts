import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { Data, Personal } from "./communityToolsShared";
import {
  anniversarySummary,
  communitySummary,
  confessionSettingsSummary,
  personalSummary,
} from "./communitySummary";

const empty: Data = {
  giveaways: [],
  polls: [],
  suggestions: [],
  autopost: [],
  anniversary: { channel_id: null, enabled: false },
  confession_settings: { review_channel_id: null, publish_channel_id: null, enabled: false },
  confessions: [],
  message_content: false,
};
const personal: Personal = { reminders: [], checklist: [], highlights: [] };

test("community list pages summarize empty and populated data", async () => {
  await setLocale("en");
  expect(communitySummary(empty)).toMatchObject({
    giveaways: "No giveaways yet.",
    polls: "No polls yet.",
    suggestions: "No suggestions yet.",
    channels: "No channel rules yet.",
    queue: "Queue is clear.",
    reports: "No reports",
    availability: "Needs Message Content intent",
    anniversary: "Off",
    confessionSettings: "Off",
  });
  const full = communitySummary({
    ...empty,
    giveaways: [
      { id: "1", prize: "Pearl", status: "running", ends_at: "", starts_at: null, winners: 1, entries: 2 },
    ],
    polls: [
      {
        id: "1",
        question: "Which?",
        status: "open",
        options: ["A"],
        multiple: false,
        anonymous: false,
        ends_at: "",
      },
    ],
    suggestions: [
      { id: "1", number: 1, body: "Idea", status: "open", staff_note: null, thread_id: null, up: 2, down: 0 },
    ],
    autopost: [{ channel_id: "1", thread: true, crosspost: false }],
    anniversary: { channel_id: "1", enabled: true },
    confession_settings: { review_channel_id: "1", publish_channel_id: "2", enabled: true },
    confessions: [{ id: "1", body: "Note", status: "pending", created_at: "", reports: 1 }],
    message_content: true,
  });
  expect(full).toMatchObject({
    giveaways: "1 giveaway",
    polls: "1 poll",
    suggestions: "1 suggestion",
    channels: "1 channel rule",
    queue: "1 pending",
    reports: "1 reported",
    availability: "Available",
    anniversary: "Channel selected",
    confessionSettings: "On",
  });
  expect(anniversarySummary(null, true)).toBe("No channel");
  expect(confessionSettingsSummary(false)).toBe("Off");
});

test("personal reminder, checklist and highlight summaries handle empty and populated lists", async () => {
  await setLocale("en");
  expect(personalSummary(personal)).toEqual({
    reminders: "No reminders yet",
    checklist: "No tasks yet",
    highlights: "No words followed",
  });
  expect(
    personalSummary({
      reminders: [{ id: "1", body: "Check", interval_minutes: 60, next_at: "" }],
      checklist: [{ id: "1", body: "Task", done: false }],
      highlights: ["reef"],
    }),
  ).toEqual({ reminders: "1 reminder", checklist: "1 task", highlights: "1 word" });
});
