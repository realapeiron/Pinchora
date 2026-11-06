import { describe, expect, it } from "vitest";

import type { AuditEntry } from "./api";
import { type Names, describeChange, initials, showValue, targetLabel, timeAgo } from "./text";

const now = new Date("2026-11-06T12:00:00Z");

function entry(after: Record<string, unknown> | null): AuditEntry {
  return {
    id: "1",
    actor_id: "2",
    actor_name: "Ana",
    source: "web",
    action: "settings.patch",
    target: null,
    before: null,
    after,
    created_at: now.toISOString(),
  };
}

describe("text", () => {
  it("says how long ago", () => {
    expect(timeAgo("2026-11-06T11:59:30Z", now)).toBe("just now");
    expect(timeAgo("2026-11-06T11:55:00Z", now)).toBe("5 min ago");
    expect(timeAgo("2026-11-06T09:00:00Z", now)).toBe("3 h ago");
    expect(timeAgo("2026-11-04T12:00:00Z", now)).toBe("2 d ago");
    expect(timeAgo("2026-01-02T12:00:00Z", now)).toBe("2026-01-02");
  });

  it("describes changes", () => {
    expect(describeChange(entry({ leveling_enabled: false }))).toBe("turned leveling off");
    expect(describeChange(entry({ leveling_enabled: true, levelup_mode: "dm" }))).toBe(
      "turned leveling on, changed level-up mode",
    );
    expect(describeChange(entry({ welcome_channel_id: "7010" }))).toBe("changed the welcome channel");
    expect(describeChange(entry(null))).toBe("made a change (settings.patch)");
    expect(describeChange(entry({ a: 1, b: 2, c: 3, d: 4 }))).toBe("changed a, changed b and 2 more");
    expect(describeChange({ ...entry({ level: 5 }), action: "level_roles.put" })).toBe("set a level role");
  });

  it("describes custom commands and auto-responders", () => {
    const tag = (action: string, target: string, after: Record<string, unknown> | null) => ({
      ...entry(after),
      action,
      target,
    });
    expect(describeChange(tag("command.create", "rules", { name: "rules" }))).toBe(
      "wrote the custom command /tag rules",
    );
    expect(describeChange(tag("command.delete", "rules", null))).toBe("deleted /tag rules");
    expect(describeChange(tag("command.update", "rules", { enabled: false }))).toBe("turned /tag rules off");
    expect(describeChange(tag("command.update", "rules", { responses: ["x"], cooldown_secs: 5 }))).toBe(
      "changed /tag rules: the replies, the cooldown",
    );
    expect(describeChange(tag("responder.create", "Welcome", {}))).toBe("wrote the auto-responder “Welcome”");
    expect(describeChange(tag("responder.update", "Welcome", { enabled: true }))).toBe(
      "turned the auto-responder “Welcome” on",
    );
    expect(describeChange(tag("commands.import", "", { created: ["a", "b"], replaced: ["c"] }))).toBe(
      "imported custom commands (2 new, 1 replaced)",
    );
    expect(describeChange(tag("responders.import", "", { created: ["a"] }))).toBe(
      "imported auto-responders (1 new)",
    );
  });

  it("shows the lists of a command change as words", () => {
    const names: Names = {
      channel: (id) => (id === "7" ? "#general" : "a deleted channel"),
      role: (id) => (id === "5" ? "@Fan" : "a deleted role"),
      module: (key) => key,
    };
    expect(showValue("allow_roles", ["5", "6"], names)).toBe("@Fan, a deleted role");
    expect(showValue("deny_channels", ["7"], names)).toBe("#general");
    expect(showValue("allow_roles", [], names)).toBe("none");
    expect(showValue("responses", ["Hi", "Hello"], names)).toBe("“Hi” · “Hello”");
    expect(
      showValue(
        "actions",
        [
          { kind: "give_points", amount: 5 },
          { kind: "dm", text: "x" },
        ],
        names,
      ),
    ).toBe("gives 5 points, sends a direct message");
    expect(showValue("embed", { title: "x" }, names)).toBe("an embed");
    expect(targetLabel({ ...entry({}), action: "command.update", target: "rules" })).toBeNull();
  });

  const names: Names = {
    channel: (id) => (id === "7010" ? "#welcome" : "a deleted channel"),
    role: (id) => (id === "55" ? "@Regulars" : "a deleted role"),
    module: (key) => (key === "pets" ? "Pets" : key),
  };

  it("shows ids as names and flags as words", () => {
    expect(showValue("welcome_channel_id", "7010", names)).toBe("#welcome");
    expect(showValue("join_role_id", "55", names)).toBe("@Regulars");
    expect(showValue("log_members", true, names)).toBe("on");
    expect(showValue("welcome_message", null, names)).toBe("—");
    expect(showValue("channels", ["7010", "9"], names)).toBe("#welcome, a deleted channel");
    expect(showValue("channels", [], names)).toBe("none");
    expect(showValue("channel_mode", "except", names)).toBe("all but some channels");
    expect(showValue("points_name", "coins", names)).toBe("coins");
    expect(showValue("level", 5, names)).toBe("5");
  });

  it("names what a row was done to", () => {
    const row = (action: string, target: string | null) => ({ ...entry(null), action, target });
    expect(targetLabel(row("module.set", "pets"), names)).toBe("Pets");
    expect(targetLabel(row("level_roles.put", "55"), names)).toBe("@Regulars");
    expect(targetLabel(row("xp_rules.put", "channel:7010"), names)).toBe("#welcome");
    expect(targetLabel(row("xp_rules.put", "role:55"), names)).toBe("@Regulars");
    expect(targetLabel(row("settings.patch", null), names)).toBeNull();
  });

  it("describes module changes in words", () => {
    const module = (after: Record<string, unknown>) => ({
      ...entry(after),
      action: "module.set",
      target: "pets",
    });
    expect(describeChange(module({ enabled: false }), names)).toBe("turned Pets off");
    expect(describeChange(module({ channel_mode: "only", channels: ["1", "2"] }), names)).toBe(
      "limited Pets to 2 channels",
    );
    expect(describeChange(module({ channel_mode: "except", channels: ["1"] }), names)).toBe(
      "switched Pets off in 1 channel",
    );
    expect(describeChange(module({ channel_mode: "all", channels: [] }), names)).toBe(
      "opened Pets to all channels",
    );
    expect(describeChange(module({ enabled: true, channel_mode: "all", channels: [] }), names)).toBe(
      "turned Pets on and opened Pets to all channels",
    );
  });

  it("makes initials", () => {
    expect(initials("Pinchora Reef")).toBe("PR");
    expect(initials("cafe")).toBe("CA");
    expect(initials("")).toBe("?");
  });
});
