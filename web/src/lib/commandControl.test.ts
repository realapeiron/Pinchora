import { describe, expect, it } from "vitest";
import { defaultRule, filteredCommands, type ControlCommand } from "./commandControl";
const commands: ControlCommand[] = [
  {
    path: "play wordle",
    description: "Daily word puzzle",
    permissions: "0",
    protected: false,
    ephemeral_supported: false,
  },
  {
    path: "backup",
    description: "Export configuration",
    permissions: "32",
    protected: true,
    ephemeral_supported: false,
  },
];
describe("command control discovery", () => {
  it("searches command paths and descriptions without case or surrounding-space sensitivity", () => {
    expect(filteredCommands(commands, " WORDLE ").map((c) => c.path)).toEqual(["play wordle"]);
    expect(filteredCommands(commands, "configuration").map((c) => c.path)).toEqual(["backup"]);
    expect(filteredCommands(commands, "missing")).toEqual([]);
  });
  it("leaves built-in permissions and cooldowns authoritative by default", () => {
    expect(defaultRule()).toMatchObject({
      enabled: true,
      cooldown_secs: 0,
      delete_reply_secs: 0,
      ephemeral: false,
    });
    const first = defaultRule();
    first.allowed_roles.push("123456789012345678");
    expect(defaultRule().allowed_roles).toEqual([]);
  });
});
