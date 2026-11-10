import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { NAV } from "./guildNav";
import { navModule } from "./navModules";

test("every sidebar route has a deliberate module mapping with a catalog key", () => {
  const source = readFileSync(
    new URL("../../../../../crates/pinchora-core/src/admin/modules/catalog.rs", import.meta.url),
    "utf8",
  );
  const keyBlock =
    source.split("pub const fn key(self) -> &'static str {")[2]?.split("pub fn parse")[0] ?? "";
  const keys = new Set([...keyBlock.matchAll(/Self::\w+ => "([^"]+)"/g)].map((match) => match[1]));
  const paths = NAV.flatMap((group) => group.pages.map((page) => page.to));
  expect(keys.size).toBe(55);
  for (const path of paths) {
    const key = navModule(path);
    if (key) expect(keys.has(key), `${key} must be in the module catalog`).toBe(true);
  }
  expect(navModule("commands")).toBe("custom_commands");
  expect(navModule("temp-voice")).toBe("temp_voice");
  expect(navModule("command-control")).toBe("command_control");
  expect(navModule("roles")).toBeUndefined();
});
