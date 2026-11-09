import { describe, expect, it } from "vitest";
import { setLocale } from "../../../i18n";
import { contentLocked, moveAction, words, type AutoRule } from "./automod";
describe("AutoMod editing", () => {
  it("reorders a chain without mutating it or dropping actions", () => {
    const chain = ["delete", "warn", "log"].map((kind) => ({ kind, secs: 0, role: null }));
    expect(moveAction(chain, 1, -1).map((a) => a.kind)).toEqual(["warn", "delete", "log"]);
    expect(chain.map((a) => a.kind)).toEqual(["delete", "warn", "log"]);
    expect(moveAction(chain, 0, -1)).toEqual(chain);
    expect(moveAction(chain, 9, -1)).toEqual(chain);
  });
  it("locks content-dependent rules until content or a native mirror is available", () => {
    const rule = { kind: "caps", discord_rule_id: null } as AutoRule;
    expect(contentLocked(rule, false)).toBe(true);
    expect(contentLocked(rule, true)).toBe(false);
    expect(contentLocked({ ...rule, kind: "spam" }, false)).toBe(false);
    expect(contentLocked({ ...rule, kind: "mentions" }, false)).toBe(false);
    expect(contentLocked({ ...rule, kind: "bad_words", discord_rule_id: "123" }, false)).toBe(false);
  });
  it("uses the active language with named values", async () => {
    await setLocale("tr");
    expect(words({ "ui.used": "{count}/{max} kullanılıyor" })("ui.used", { count: 4, max: 30 })).toBe(
      "4/30 kullanılıyor",
    );
    await setLocale("en");
  });
});
