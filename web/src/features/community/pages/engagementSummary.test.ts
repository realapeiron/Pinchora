import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type {
  CommunityEngagementData,
  Counting,
  MailSettings,
  QotdSettings,
} from "../api/communityEngagement";
import { engagementSummary } from "./engagementSummary";

const qotd = { channel_id: null } as QotdSettings;
const counting = { channel_id: "", current: 0 } as Counting;
const mail = { enabled: false } as MailSettings;
const data = { forms: [], threads: [] } as unknown as CommunityEngagementData;

test("community pack summaries handle off and empty sections", async () => {
  await setLocale("en");
  expect(engagementSummary(data, qotd, counting, mail)).toEqual({
    qotd: "Off",
    counting: "Off",
    forms: "No application forms yet",
    modmail: "Off",
  });
});

test("community pack summaries use draft settings and list counts", async () => {
  await setLocale("en");
  expect(
    engagementSummary(
      {
        ...data,
        forms: [{}, {}] as CommunityEngagementData["forms"],
        threads: [{}] as CommunityEngagementData["threads"],
      },
      { ...qotd, channel_id: "1" },
      { ...counting, channel_id: "2", current: 42 },
      { ...mail, enabled: true },
    ),
  ).toEqual({
    qotd: "Channel selected",
    counting: "Count 42",
    forms: "2 application forms",
    modmail: "1 open thread",
  });
});
