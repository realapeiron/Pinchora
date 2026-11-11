import { t } from "../../../i18n/dashboard";
import type { BrandingDraft } from "../api/branding";

export function brandingSummary(value: BrandingDraft) {
  return {
    nickname: value.nickname || t("No custom name"),
    bio: value.bio?.trim() ? value.bio.trim().slice(0, 45) : t("No bio"),
    images:
      value.avatar_upload_id && value.banner_upload_id
        ? t("Avatar and banner")
        : value.avatar_upload_id
          ? t("Avatar selected")
          : value.banner_upload_id
            ? t("Banner selected")
            : t("Default images"),
    colour: value.embed_color || t("Default colour"),
  };
}
