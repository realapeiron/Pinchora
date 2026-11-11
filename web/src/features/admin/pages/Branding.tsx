import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";

import { t } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import type { Overview } from "../../../lib/api";
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
} from "../api/branding";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/branding.css";
import { BrandingFields } from "./BrandingFields";

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
        setDraft(brandingDraft(value));
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
  const changed = JSON.stringify(brandingDraft(draft)) !== JSON.stringify(brandingDraft(saved));
  const color = /^#[0-9a-fA-F]{6}$/.test(draft.embed_color ?? "") ? draft.embed_color! : "#FF6B4A";
  const wait = saved.applied_at
    ? Math.max(0, 600 - Math.floor((now - new Date(saved.applied_at).getTime()) / 1000))
    : 0;
  return (
    <SettingsPage
      className="settings-page branding-page"
      title={t("Server branding")}
      lead={t(
        "Set how Pinchora appears in this server. Preview your changes before applying them to Discord.",
      )}
    >
      <BrandingFields
        draft={draft}
        color={color}
        avatar={avatar}
        banner={banner}
        guildName={guild.name}
        activeNickname={saved.active_nickname}
        activeBio={saved.active_bio}
        setField={setField}
        upload={upload}
      />
      <div className="branding-feedback">
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
            disabled={busy || !changed}
            onClick={() =>
              void run(async () => {
                const value = await saveBranding(guild.id, draft);
                setSaved(value);
                setDraft(brandingDraft(value));
                setAvatar(value.avatar_url);
                setBanner(value.banner_url);
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
                if (changed) await saveBranding(guild.id, draft);
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
      </div>
    </SettingsPage>
  );
}
