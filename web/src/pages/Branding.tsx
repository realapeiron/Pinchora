import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";

import { t } from "../i18n";
import { PixelButton } from "../components/PixelButton";
import type { Overview } from "../lib/api";
import {
  applyBranding,
  brandingDraft,
  brandingResultMessage,
  getBranding,
  resetBranding,
  saveBranding,
  uploadBrandingImage,
  type Branding,
  type BrandingDraft,
} from "../lib/branding";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/branding.css";

const defaultDraft: BrandingDraft = {
  nickname: null,
  avatar_upload_id: null,
  banner_upload_id: null,
  bio: null,
  embed_color: null,
};

function refusedFields(codes: string): string {
  const names: Record<string, string> = {
    nickname: t("Server nickname"),
    avatar: t("Avatar"),
    banner: t("Banner"),
    bio: t("Bio"),
  };
  return codes
    .split(",")
    .map((code) => names[code] ?? code)
    .join(", ");
}

export function BrandingPage() {
  const { guild } = useOutletContext<Overview>();
  const [saved, setSaved] = useState<Branding | null>(null);
  const [draft, setDraft] = useState<BrandingDraft>(defaultDraft);
  const [avatar, setAvatar] = useState<string | null>(null);
  const [banner, setBanner] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [notice, setNotice] = useState("");
  const [resetting, setResetting] = useState(false);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  const setField = <K extends keyof BrandingDraft>(key: K, value: BrandingDraft[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  useEffect(() => {
    let current = true;
    getBranding(guild.id)
      .then((value) => {
        if (!current) return;
        setSaved(value);
        setDraft({
          nickname: value.nickname,
          avatar_upload_id: value.avatar_upload_id,
          banner_upload_id: value.banner_upload_id,
          bio: value.bio,
          embed_color: value.embed_color,
        });
        setAvatar(value.avatar_url);
        setBanner(value.banner_url);
      })
      .catch((cause: Error) => current && setError(cause))
      .finally(() => current && setLoading(false));
    return () => {
      current = false;
    };
  }, [guild.id]);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setNotice("");
    try {
      await action();
    } catch (cause) {
      setError(cause as Error);
    } finally {
      setBusy(false);
    }
  };
  const upload = (kind: "avatar" | "banner", file: File | undefined) => {
    if (!file) return;
    void run(async () => {
      const result = await uploadBrandingImage(guild.id, file);
      setField(kind === "avatar" ? "avatar_upload_id" : "banner_upload_id", result.id);
      (kind === "avatar" ? setAvatar : setBanner)(result.url);
      setNotice(t("Image uploaded. Save changes to keep it."));
    });
  };
  if (loading) return <Loading />;
  if (!saved) return <Problem error={error ?? new Error(t("Branding could not load."))} />;
  const color = /^#[0-9a-fA-F]{6}$/.test(draft.embed_color ?? "") ? draft.embed_color! : "#FF6B4A";
  const wait = saved.applied_at
    ? Math.max(0, 600 - Math.floor((now - new Date(saved.applied_at).getTime()) / 1000))
    : 0;
  return (
    <div className="settings-page branding-page">
      <PageHead
        title={t("Server branding")}
        lead={t(
          "Set how Pinchora appears in this server. Preview your changes before applying them to Discord.",
        )}
      />
      <div className="branding-grid">
        <section className="branding-editor" aria-label={t("Branding settings")}>
          <h2>{t("Profile")}</h2>
          <label>
            {t("Server nickname")}
            <input
              maxLength={32}
              value={draft.nickname ?? ""}
              onChange={(event) => setField("nickname", event.target.value || null)}
              placeholder="Pinchora"
            />
          </label>
          <label>
            {t("Bio")}
            <textarea
              maxLength={190}
              rows={3}
              value={draft.bio ?? ""}
              onChange={(event) => setField("bio", event.target.value || null)}
              placeholder={t("A little about this bot in your server")}
            />
          </label>
          <div className="branding-images">
            <label>
              {t("Avatar")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                onChange={(event) => upload("avatar", event.target.files?.[0])}
              />
            </label>
            <label>
              {t("Banner")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                onChange={(event) => upload("banner", event.target.files?.[0])}
              />
            </label>
          </div>
          <p className="branding-hint">
            {t("Images use the welcome card upload limits and are converted to PNG.")}
          </p>
          <h2>{t("Messages")}</h2>
          <label>
            {t("Embed colour")}
            <span className="branding-color-row">
              <input
                type="color"
                value={color}
                onChange={(event) => setField("embed_color", event.target.value)}
              />
              <input
                aria-label={t("Hex colour")}
                maxLength={7}
                value={draft.embed_color ?? ""}
                onChange={(event) => setField("embed_color", event.target.value || null)}
                placeholder={t("Hex colour")}
              />
            </span>
          </label>
          <p className="branding-hint">
            {t("This colour is used on Pinchora embeds throughout this server.")}
          </p>
          {saved.last_error && (
            <p className="branding-warning" role="status">
              {t("Discord refused part of the last apply: {detail}", {
                detail: refusedFields(saved.last_error),
              })}
            </p>
          )}
          {(saved.avatar_supported === false ||
            saved.banner_supported === false ||
            saved.bio_supported === false) && (
            <p className="branding-warning">
              {t(
                "Some profile fields are unavailable to this app. The colour and available fields still work.",
              )}
            </p>
          )}
          {error && (
            <p className="branding-warning" role="alert">
              {error.message}
            </p>
          )}
          {notice && <p role="status">{notice}</p>}
          <div className="branding-actions">
            <PixelButton
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const value = await saveBranding(guild.id, draft);
                  setSaved(value);
                  setNotice(t("Changes saved."));
                })
              }
            >
              {t("Save changes")}
            </PixelButton>
            <PixelButton
              variant="secondary"
              disabled={busy || wait > 0}
              onClick={() =>
                void run(async () => {
                  await saveBranding(guild.id, draft);
                  const value = await applyBranding(guild.id);
                  setSaved(value);
                  setDraft(brandingDraft(value));
                  setAvatar(value.avatar_url);
                  setBanner(value.banner_url);
                  setNotice(brandingResultMessage(Boolean(value.last_error)));
                })
              }
            >
              {wait > 0
                ? t("Apply in {minutes} min", { minutes: Math.ceil(wait / 60) })
                : t("Apply to Discord")}
            </PixelButton>
            <PixelButton variant="ghost" disabled={busy || wait > 0} onClick={() => setResetting(true)}>
              {t("Reset branding")}
            </PixelButton>
          </div>
          {resetting && (
            <div className="branding-reset" role="group" aria-label={t("Confirm reset")}>
              <p>{t("Clear the saved profile and colour for this server?")}</p>
              <PixelButton
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await resetBranding(guild.id);
                    setSaved(result.branding);
                    setDraft(brandingDraft(result.branding));
                    setAvatar(result.branding.avatar_url);
                    setBanner(result.branding.banner_url);
                    setResetting(false);
                    setNotice(brandingResultMessage(result.discord_failed.length > 0, true));
                  })
                }
              >
                {t("Confirm reset")}
              </PixelButton>
              <PixelButton variant="ghost" onClick={() => setResetting(false)}>
                {t("Cancel")}
              </PixelButton>
            </div>
          )}
        </section>
        <aside className="branding-preview" aria-label={t("Live preview")}>
          <h2>{t("Live preview")}</h2>
          <p>{t("Last accepted nickname: {name}", { name: saved.active_nickname ?? "Pinchora" })}</p>
          <p>{t("Last accepted bio: {bio}", { bio: saved.active_bio ?? "—" })}</p>
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
              <span>{t("Bot")}</span>
              <p>{draft.bio || t("A little about this bot in your server")}</p>
            </div>
          </div>
          <div className="branding-embed" style={{ borderLeftColor: color }}>
            <strong>{t("Welcome to {server}", { server: guild.name })}</strong>
            <p>{t("Your server's colour appears on every Pinchora embed.")}</p>
          </div>
        </aside>
      </div>
    </div>
  );
}
