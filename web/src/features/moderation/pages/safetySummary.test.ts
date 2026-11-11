import { expect, test } from "vitest";
import type { SafetyConfig, SafetyData } from "../api/verification";
import { safetySummary } from "./safetySummary";

const config = {
  verification: { enabled: false, mode: "captcha", message: "", exempt_roles: [], exempt_users: [] },
  raid: { enabled: false, join_threshold: 5, window_secs: 30, action: "alert" },
} as unknown as SafetyConfig;
const data = { joins: [], events: [] } as unknown as SafetyData;
const words = (key: string) => key;

test("Safety summary handles off and empty data", () => {
  expect(safetySummary(config, data, words)).toEqual({
    gate: "Off",
    avatar: "Off",
    names: "Off",
    dm: "Off",
    raid: "Off",
    similarNames: "Off",
    similarAvatars: "Off",
    lock: "Off",
    invites: "Off",
    message: "No message",
    exemptions: "0 roles · 0 users",
    joins: "0 recent joins",
    events: "0 raid events",
  });
});

test("Safety summary follows current checks, message and activity", () => {
  const value = {
    verification: {
      ...config.verification,
      enabled: true,
      block_default_avatar: true,
      block_name_patterns: true,
      dm_on_block: true,
      message: "Welcome",
      exempt_roles: ["1"],
      exempt_users: ["2"],
    },
    raid: {
      ...config.raid,
      enabled: true,
      similar_names: true,
      similar_avatars: true,
      lock_channels: true,
      pause_invites: true,
    },
  };
  expect(safetySummary(value, { ...data, joins: [{}], events: [{}, {}] } as SafetyData, words)).toMatchObject(
    {
      gate: "captcha",
      avatar: "On",
      names: "On",
      dm: "On",
      raid: "5 / 30s · alert",
      similarNames: "On",
      similarAvatars: "On",
      lock: "On",
      invites: "On",
      message: "Welcome",
      exemptions: "1 roles · 1 users",
      joins: "1 recent joins",
      events: "2 raid events",
    },
  );
});
