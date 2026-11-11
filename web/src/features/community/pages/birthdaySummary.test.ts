import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { GuildInfo } from "../../../lib/api";
import type { BirthdaySettings } from "../api/birthdays";
import { birthdaySummary } from "./birthdaySummary";

const settings: BirthdaySettings = {
  enabled: false,
  channel_id: null,
  role_id: null,
  message: "Hello {user}",
  announce_hour: 9,
  server_tz: "UTC",
  shells_gift: 0,
  allow_age: false,
  badge: false,
};
const info = {
  channels: [{ id: "1", name: "party", type: 0 }],
  roles: [{ id: "2", name: "Cake" }],
} as GuildInfo;

test("birthday rows summarize off and empty settings", async () => {
  await setLocale("en");
  expect(birthdaySummary(settings, info, 0)).toMatchObject({
    announcement: "Off",
    role: "Off",
    zone: "UTC",
    gift: "No gift",
    message: "Hello {user}",
    age: "Off",
    badge: "Off",
    upcoming: "No shared birthdays in the next 30 days.",
  });
  expect(birthdaySummary({ ...settings, server_tz: "", message: "" }, undefined, 0)).toMatchObject({
    zone: "No time zone",
    message: "No message",
  });
});

test("birthday rows summarize current draft values", async () => {
  await setLocale("en");
  expect(
    birthdaySummary(
      {
        ...settings,
        enabled: true,
        channel_id: "1",
        role_id: "2",
        shells_gift: 100,
        allow_age: true,
        badge: true,
      },
      info,
      2,
    ),
  ).toMatchObject({
    announcement: "#party",
    role: "@Cake",
    gift: "100 shells",
    age: "On",
    badge: "On",
    upcoming: "2 shared birthdays",
  });
});
