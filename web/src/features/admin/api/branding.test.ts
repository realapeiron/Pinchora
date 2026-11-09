import { describe, expect, it } from "vitest";
import { brandingDraft, brandingResultMessage, type Branding } from "./branding";

describe("branding outcomes", () => {
  it("keeps refused reset fields from the server response", () => {
    const partial = {
      nickname: null,
      bio: "Kept",
      avatar_upload_id: "old",
      banner_upload_id: null,
      embed_color: null,
    };
    expect(brandingDraft(partial as Branding)).toEqual(partial);
  });
  it("excludes accepted profile state and capability metadata from saves", () => {
    const editable = {
      nickname: "Reef guide",
      bio: null,
      avatar_upload_id: null,
      banner_upload_id: null,
      embed_color: "#31B8D4",
    };
    const response = {
      ...editable,
      active_nickname: "Old name",
      avatar_supported: false,
      applied_at: "2026-11-08T00:00:00Z",
      avatar_url: "/uploads/avatar.png",
    };
    expect(brandingDraft(response)).toEqual(editable);
  });
  it("never presents partial apply or reset as success", () => {
    expect(brandingResultMessage(true)).toContain("could not be applied");
    expect(brandingResultMessage(true, true)).toContain("previous values were kept");
    expect(brandingResultMessage(false)).toBe("Branding applied to Discord.");
    expect(brandingResultMessage(false, true)).toBe("Branding reset.");
  });
});
