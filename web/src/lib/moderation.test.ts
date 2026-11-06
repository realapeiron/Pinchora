import { describe, expect, it } from "vitest";

import type { LogSettings, ModerationSettings, StaffEntry } from "./api";
import { draftLadder, logDraft, logsPatch, moderationPatch, staffLine } from "./moderation";

const logs: LogSettings = {
  channel_id: "1",
  categories: [
    {
      key: "messages",
      label: "Messages",
      channel_id: null,
      events: [
        { key: "message_edit", label: "Message edited", enabled: true, audit_log: false },
        { key: "message_delete", label: "Message deleted", enabled: true, audit_log: false },
      ],
    },
  ],
  ignored_channels: [],
  ignored_roles: ["5"],
  ignored_users: [],
  ignore_bots: false,
  max_ignored: 50,
};

const entry = (over: Partial<StaffEntry>): StaffEntry => ({
  key: "c:1",
  actor_id: "9",
  actor_name: "Mod",
  source: "command",
  action: "ban",
  target_id: "4",
  target_name: "Mia",
  case_number: 1,
  reason: null,
  details: null,
  created_at: "2026-01-01T00:00:00Z",
  ...over,
});

describe("the log draft", () => {
  it("sends only what changed", () => {
    const draft = logDraft(logs);
    expect(logsPatch(logs, draft)).toEqual({ patch: {}, count: 0 });
    draft.events.message_edit = false;
    draft.category_channels.messages = "7";
    draft.ignored_roles = [];
    draft.ignore_bots = true;
    expect(logsPatch(logs, draft)).toEqual({
      patch: {
        category_channels: { messages: "7" },
        events: { message_edit: false },
        ignored_roles: [],
        ignore_bots: true,
      },
      count: 4,
    });
  });
});

describe("the moderation draft", () => {
  const saved: ModerationSettings = {
    log_channel_id: null,
    dm_members: true,
    ladder: [
      { warns: 3, action: "timeout", duration: "1h", duration_secs: 3600, label: "3 warns → 1h timeout" },
    ],
    max_rungs: 10,
  };
  it("counts the channel, the messages and the ladder", () => {
    expect(moderationPatch(saved, null, true, draftLadder(saved)).count).toBe(0);
    const ladder = [...draftLadder(saved), { warns: 5, action: "kick" as const, duration: " " }];
    const { patch, count } = moderationPatch(saved, "8", false, ladder);
    expect(count).toBe(3);
    expect(patch.ladder?.[1]).toEqual({ warns: 5, action: "kick", duration: null });
  });
});

describe("staff lines", () => {
  it("say what was done to whom", () => {
    expect(staffLine(entry({}))).toBe("banned Mia");
    expect(staffLine(entry({ action: "server_update", target_name: null }))).toBe(
      "changed the server settings",
    );
    expect(staffLine(entry({ source: "web", action: "logs.set", target_name: null }))).toBe(
      "changed the server log",
    );
    expect(staffLine(entry({ source: "web", action: "module.set", target_name: "pets" }))).toBe(
      "changed a module: pets",
    );
    expect(staffLine(entry({ source: "web", action: "brand.new", target_name: null }))).toBe(
      "changed the settings (brand.new)",
    );
  });
});
