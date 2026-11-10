import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";
import { iconNames } from "../../../components/icons/map";
import { moduleIcons } from "./moduleIcons";

const root = new URL("../../../../../", import.meta.url);
const catalog = readFileSync(new URL("crates/pinchora-core/src/admin/modules/catalog.rs", root), "utf8");
const sprite = readFileSync(new URL("web/src/components/icons/sprite.svg", root), "utf8");

describe("overview module icons", () => {
  test("every server catalog key has a mapped icon present in the sprite", () => {
    const keyMethod = catalog.split("pub const fn key(self)").at(-1)!.split("pub fn parse")[0]!;
    const keys = [...keyMethod.matchAll(/Self::\w+ => "([^"]+)"/g)].map((match) => match[1]);
    expect(keys).toHaveLength(55);
    expect(Object.keys(moduleIcons).sort()).toEqual(keys.sort());
    for (const icon of Object.values(moduleIcons)) {
      expect(sprite).toContain(`id="px-${iconNames[icon as keyof typeof iconNames] ?? icon}"`);
    }
  });
});
