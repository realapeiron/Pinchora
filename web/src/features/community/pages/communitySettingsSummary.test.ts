import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { Community, GuildInfo } from "../../../lib/api";
import { birthdaySettingsSummary, communitySettingsSummary } from "./communitySettingsSummary";

const empty = {
  suggestions: { channel_id: null },
  critters: { enabled: false, channel_id: null },
  surge: { enabled: false, channel_id: null, every_hours: 24 },
} as Community;

test("community settings summarize empty and disabled sections", async () => {
  await setLocale("en");
  expect(communitySettingsSummary(empty)).toEqual({
    suggestions: "No channel",
    critters: "Off",
    surge: "Off",
  });
  expect(birthdaySettingsSummary(false)).toBe("Off");
});

test("community settings summarize enabled draft values", async () => {
  await setLocale("en");
  expect(
    communitySettingsSummary({
      ...empty,
      suggestions: { channel_id: "1" },
      critters: { enabled: true, channel_id: "2" },
      surge: { ...empty.surge, enabled: true, channel_id: "3", every_hours: 12 },
    }),
  ).toEqual({
    suggestions: "Channel selected",
    critters: "Channel selected",
    surge: "Channel selected · Every 12 hours",
  });
  expect(birthdaySettingsSummary(true)).toBe("On");
  expect(communitySettingsSummary({ ...empty, critters: { enabled: true, channel_id: null } }).critters).toBe(
    "Anywhere busy",
  );
  expect(
    communitySettingsSummary({ ...empty, suggestions: { channel_id: "1" } }, {
      channels: [{ id: "1", name: "general", type: 0 }],
    } as GuildInfo).suggestions,
  ).toBe("#general");
});
