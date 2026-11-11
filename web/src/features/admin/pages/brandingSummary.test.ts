import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { BrandingDraft } from "../api/branding";
import { brandingSummary } from "./brandingSummary";

test("branding rows describe empty and populated profile values", async () => {
  await setLocale("en");
  const empty: BrandingDraft = {
    nickname: null,
    bio: null,
    avatar_upload_id: null,
    banner_upload_id: null,
    embed_color: null,
  };
  expect(brandingSummary(empty)).toEqual({
    nickname: "No custom name",
    bio: "No bio",
    images: "Default images",
    colour: "Default colour",
  });
  expect(
    brandingSummary({
      ...empty,
      nickname: "Reef",
      bio: "Our helper",
      avatar_upload_id: "a",
      banner_upload_id: "b",
      embed_color: "#123456",
    }),
  ).toEqual({ nickname: "Reef", bio: "Our helper", images: "Avatar and banner", colour: "#123456" });
});
