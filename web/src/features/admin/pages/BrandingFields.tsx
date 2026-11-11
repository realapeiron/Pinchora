import { t as translate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { SettingsRow } from "../../../components/SettingsRow";
import type { BrandingDraft } from "../api/branding";
import { brandingSummary } from "./brandingSummary";

export function BrandingFields({
  draft,
  color,
  avatar,
  banner,
  guildName,
  activeNickname,
  activeBio,
  setField,
  upload,
}: {
  draft: BrandingDraft;
  color: string;
  avatar: string | null;
  banner: string | null;
  guildName: string;
  activeNickname: string | null;
  activeBio: string | null;
  setField: <K extends keyof BrandingDraft>(key: K, value: BrandingDraft[K]) => void;
  upload: (kind: "avatar" | "banner", file: File | undefined) => void;
}) {
  const summary = brandingSummary(draft);
  return (
    <div className="settings-detail-list branding-rows" aria-label={translate("Branding settings")}>
      <SettingsRow
        title={translate("Server nickname")}
        description={t("Set the bot name shown in this server.")}
        summary={summary.nickname}
        initialOpen
      >
        <div className="settings-detail-fields branding-editor">
          <label>
            {translate("Server nickname")}
            <input
              maxLength={32}
              value={draft.nickname ?? ""}
              onChange={(event) => setField("nickname", event.target.value || null)}
              placeholder="Pinchora"
            />
          </label>
        </div>
        <div className="settings-detail-preview branding-preview" aria-label={translate("Live preview")}>
          <h2>{translate("Live preview")}</h2>
          <p>{translate("Last accepted nickname: {name}", { name: activeNickname ?? "Pinchora" })}</p>
          <div className="branding-profile-body">
            <strong>{draft.nickname || "Pinchora"}</strong>
            <span>{translate("Bot")}</span>
          </div>
        </div>
      </SettingsRow>
      <SettingsRow title={translate("Bio")} description={t("Edit the bot biography.")} summary={summary.bio}>
        <div className="settings-detail-fields branding-editor">
          <label>
            {translate("Bio")}
            <textarea
              maxLength={190}
              rows={3}
              value={draft.bio ?? ""}
              onChange={(event) => setField("bio", event.target.value || null)}
              placeholder={translate("A little about this bot in your server")}
            />
          </label>
        </div>
        <div className="settings-detail-preview branding-preview" aria-label={translate("Live preview")}>
          <h2>{translate("Live preview")}</h2>
          <p>{translate("Last accepted bio: {bio}", { bio: activeBio ?? "—" })}</p>
          <p>{draft.bio || translate("A little about this bot in your server")}</p>
        </div>
      </SettingsRow>
      <SettingsRow
        title={t("Avatar and banner")}
        description={t("Choose the bot's profile images.")}
        summary={summary.images}
      >
        <div className="settings-detail-fields branding-editor">
          <div className="branding-images">
            <label>
              {translate("Avatar")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                onChange={(event) => upload("avatar", event.target.files?.[0])}
              />
            </label>
            <label>
              {translate("Banner")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                onChange={(event) => upload("banner", event.target.files?.[0])}
              />
            </label>
          </div>
          <p className="branding-hint">
            {translate("Images use the welcome card upload limits and are converted to PNG.")}
          </p>
        </div>
        <div className="settings-detail-preview branding-preview" aria-label={translate("Live preview")}>
          <h2>{translate("Live preview")}</h2>
          <div className="branding-profile">
            <div
              className="branding-banner"
              style={banner ? { backgroundImage: `url(${banner})` } : { backgroundColor: color }}
            />
            <div
              className="branding-avatar"
              style={avatar ? { backgroundImage: `url(${avatar})` } : { backgroundColor: color }}
            >
              {" "}
              {!avatar && "P"}{" "}
            </div>
            <div className="branding-profile-body">
              <strong>{draft.nickname || "Pinchora"}</strong>
              <span>{translate("Bot")}</span>
              <p>{draft.bio || translate("A little about this bot in your server")}</p>
            </div>
          </div>
        </div>
      </SettingsRow>
      <SettingsRow
        title={translate("Embed colour")}
        description={t("Set the colour used on server messages.")}
        summary={summary.colour}
      >
        <div className="settings-detail-fields branding-editor">
          <label>
            {translate("Embed colour")}
            <span className="branding-color-row">
              <input
                type="color"
                value={color}
                onChange={(event) => setField("embed_color", event.target.value)}
              />
              <input
                aria-label={translate("Hex colour")}
                maxLength={7}
                value={draft.embed_color ?? ""}
                onChange={(event) => setField("embed_color", event.target.value || null)}
                placeholder={translate("Hex colour")}
              />
            </span>
          </label>
          <p className="branding-hint">
            {translate("This colour is used on Pinchora embeds throughout this server.")}
          </p>
        </div>
        <div className="settings-detail-preview branding-preview" aria-label={translate("Live preview")}>
          <h2>{translate("Live preview")}</h2>
          <div className="branding-embed" style={{ borderLeftColor: color }}>
            <strong>{translate("Welcome to {server}", { server: guildName })}</strong>
            <p>{translate("Your server's colour appears on every Pinchora embed.")}</p>
          </div>
        </div>
      </SettingsRow>
    </div>
  );
}
