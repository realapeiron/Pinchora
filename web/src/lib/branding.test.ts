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
  it("never presents partial apply or reset as success", () => {
    expect(brandingResultMessage(true)).toContain("could not be applied");
    expect(brandingResultMessage(true, true)).toContain("previous values were kept");
    expect(brandingResultMessage(false)).toBe("Branding applied to Discord.");
    expect(brandingResultMessage(false, true)).toBe("Branding reset.");
  });
});
