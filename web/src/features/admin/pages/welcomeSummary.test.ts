import { expect, test } from "vitest";
import type { GuildInfo, Settings } from "../../../lib/api";
import { welcomeSummary } from "./welcomeSummary";

const settings = {
  welcome_enabled: true,
  welcome_channel_id: "1",
  goodbye_channel_id: "2",
  goodbye_message: "See you soon, {user}!",
  join_role_id: "3",
} as Settings;
const info = {
  channels: [
    { id: "1", name: "welcome", type: 0 },
    { id: "2", name: "farewell", type: 0 },
  ],
  roles: [{ id: "3", name: "New" }],
} as GuildInfo;

test("collapsed Welcome rows summarise the current channel, format, role and goodbye text", () => {
  expect(welcomeSummary(settings, info, "text + card")).toEqual({
    welcome: "#welcome · text + card",
    role: "@New",
    goodbye: "#farewell · See you soon, {user}!",
  });
  expect(
    welcomeSummary(
      {
        ...settings,
        welcome_enabled: false,
        welcome_channel_id: null,
        join_role_id: null,
        goodbye_channel_id: null,
      },
      info,
    ),
  ).toEqual({
    welcome: "Off",
    role: "Off",
    goodbye: "Off",
  });
});
