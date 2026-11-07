import { expect, test } from "vitest";
import { nicknamePreview } from "./roblox";
test("nickname preview preserves graphemes and the Discord limit", () => {
  expect(nicknamePreview("{roblox-name}", "", "a\u0301".repeat(20))).toBe("a\u0301".repeat(16));
  expect(nicknamePreview("{roblox-name}", "", "👩‍👩‍👧‍👦".repeat(5))).toBe("👩‍👩‍👧‍👦".repeat(4));
});
