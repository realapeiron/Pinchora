import { expect, it } from "vitest";

import { legacyGuildTarget } from "./LegacyGuildRedirect";

it("redirects the old community URL within the same guild and preserves search and hash", () => {
  expect(legacyGuildTarget("123", "community-engagement", "?tab=welcome", "#editor")).toBe(
    "/g/123/community-engagement?tab=welcome#editor",
  );
});

it("redirects the old moderation URL within the same guild and preserves search and hash", () => {
  expect(legacyGuildTarget("456", "moderation-desk", "?queue=appeals", "#case")).toBe(
    "/g/456/moderation-desk?queue=appeals#case",
  );
});
