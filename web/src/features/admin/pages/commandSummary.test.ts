import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { ControlData } from "../api/commandControl";
import { commandSummary, commandExtrasSummary } from "./commandSummary";

test("command families and extras show empty and disabled counts", async () => {
  await setLocale("en");
  const empty = {
    config: { rules: {}, aliases: {} },
    commands: [],
    permissions: null,
    permission_names: {},
    strings: {},
  } as ControlData;
  expect(commandSummary(empty, [])).toBe("0 commands · 0 off");
  expect(commandExtrasSummary(empty)).toEqual({
    aliases: "No aliases",
    permissions: "Permissions unavailable",
  });
  const commands = [
    { path: "admin ban", description: "", permissions: "0", ephemeral_supported: false, protected: false },
  ];
  const data = {
    ...empty,
    config: { rules: { "admin ban": { enabled: false } }, aliases: { b: "admin ban" } },
    permissions: [],
  } as unknown as ControlData;
  expect(commandSummary(data, commands)).toBe("1 command · 1 off");
  expect(commandExtrasSummary(data)).toEqual({ aliases: "1 alias", permissions: "0 permissions" });
});
