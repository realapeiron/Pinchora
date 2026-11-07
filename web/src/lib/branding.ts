import { api, apiUpload } from "./api";

export interface Branding {
  nickname: string | null;
  avatar_upload_id: string | null;
  banner_upload_id: string | null;
  bio: string | null;
  embed_color: string | null;
  applied_at: string | null;
  last_error: string | null;
  avatar_supported: boolean | null;
  banner_supported: boolean | null;
  bio_supported: boolean | null;
  avatar_url: string | null;
  banner_url: string | null;
}

export type BrandingDraft = Pick<
  Branding,
  "nickname" | "avatar_upload_id" | "banner_upload_id" | "bio" | "embed_color"
>;
export const brandingPath = (guild: string) => `/api/guilds/${guild}/branding`;
export const getBranding = (guild: string) => api<Branding>(brandingPath(guild));
export const saveBranding = (guild: string, draft: BrandingDraft) =>
  api<Branding>(brandingPath(guild), "PUT", draft);
export const applyBranding = (guild: string) => api<Branding>(`${brandingPath(guild)}/apply`, "POST");
export const resetBranding = (guild: string) =>
  api<{ branding: Branding; discord_failed: string[] }>(`${brandingPath(guild)}/reset`, "POST");
export const uploadBrandingImage = (guild: string, file: File) =>
  apiUpload<{ id: string; url: string }>(`/api/guilds/${guild}/cards/uploads`, file);
