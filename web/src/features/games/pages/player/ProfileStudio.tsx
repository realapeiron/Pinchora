import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { SaveBar } from "../../../../components/SaveBar";
import { t } from "../../../../i18n";
import { api } from "../../../../lib/api";
import { usePlayerStudio, profileChanges, type ProfileData } from "../../api/webPlayer";
import { PageHead } from "../../../../components/PageHead";
import { Loading, Problem } from "../../../../components/States";
import { useProfileEditor } from "./profile/useProfileEditor";
import { BadgesPanel } from "./profile/BadgesPanel";
import { BackgroundPanel } from "./profile/BackgroundPanel";
import { StylePanel } from "./profile/StylePanel";
import { TitlePanel } from "./profile/TitlePanel";
import { ShowcasePanel } from "./profile/ShowcasePanel";
import { BioPanel } from "./profile/BioPanel";

export function ProfileStudio() {
  const query = usePlayerStudio<ProfileData>("profile");
  return (
    <>
      <PageHead
        title={t("Profile studio")}
        lead={t("Make your card yours. Preview every change before saving.")}
      />
      {query.data ? (
        <Editor data={query.data} />
      ) : query.isPending ? (
        <Loading />
      ) : (
        <Problem error={query.error ?? new Error(t("Could not load your collection. Try again."))} />
      )}
      {query.data && query.isError && (
        <p role="alert" className="notice notice-heart">
          {query.error.message}
        </p>
      )}
    </>
  );
}
function Editor({ data }: { data: ProfileData }) {
  const model = useProfileEditor(data);
  const {
    draft,
    setDraft,
    saved,
    tab,
    setTab,
    notice,
    setNotice,
    preview,
    save,
    buy,
    tabs,
    unownedBackground,
  } = model;
  return (
    <>
      <div className="player-studio">
        <aside className="player-preview">
          <Panel title={t("Live preview")}>
            {preview.src ? (
              <img src={preview.src} alt={t("Profile card preview")} className="player-card" />
            ) : (
              <Loading />
            )}
            <p className="muted" role="status">
              {preview.pending
                ? t("Updating preview…")
                : t("Global profile · changes stay here until you save.")}
            </p>
            {preview.error && (
              <p role="alert" className="notice notice-heart">
                {preview.error.message}
                <PixelButton variant="secondary" onClick={() => void preview.retry()}>
                  {t("Retry preview")}
                </PixelButton>
              </p>
            )}
            <p>{t("{count} shells available", { count: data.shells })}</p>
            {unownedBackground && <p className="notice">{t("Buy this background before saving it.")}</p>}
          </Panel>
        </aside>
        <div className="player-controls">
          <nav className="player-tabs" aria-label={t("Profile sections")}>
            {tabs.map(([id, name]) => (
              <button
                type="button"
                key={id}
                aria-pressed={tab === id}
                onClick={() => {
                  setTab(id);
                  if (id === "badges") void api("/api/me/badges/seen", "POST").catch(() => {});
                }}
              >
                {name}
              </button>
            ))}
          </nav>
          {tab === "badges" && (
            <BadgesPanel
              model={{
                data: model.data,
                draft: model.draft,
                setDraft: model.setDraft,
                dragged: model.dragged,
                setDragged: model.setDragged,
                move: model.move,
                pick: model.pick,
                buy: model.buy,
              }}
            />
          )}
          {tab === "background" && (
            <BackgroundPanel
              model={{
                data: model.data,
                draft: model.draft,
                update: model.update,
                buy: model.buy,
              }}
            />
          )}
          {tab === "style" && (
            <StylePanel
              model={{
                data: model.data,
                draft: model.draft,
                update: model.update,
                setDraft: model.setDraft,
              }}
            />
          )}
          {tab === "title" && (
            <TitlePanel
              model={{
                data: model.data,
                draft: model.draft,
                update: model.update,
              }}
            />
          )}
          {tab === "showcase" && (
            <ShowcasePanel
              model={{
                data: model.data,
                draft: model.draft,
                update: model.update,
              }}
            />
          )}
          {tab === "bio" && (
            <BioPanel
              model={{
                draft: model.draft,
                update: model.update,
              }}
            />
          )}
          {buy.error && (
            <p role="alert" className="notice notice-heart">
              {buy.error.message}
            </p>
          )}
          <p role="status" className="player-status">
            {notice}
          </p>
        </div>
      </div>
      <SaveBar
        count={profileChanges(saved, draft)}
        saving={save.isPending}
        blocked={unownedBackground || [...draft.bio].length > 300 || !!preview.error || preview.pending}
        error={save.error?.message}
        onSave={() => save.mutate(draft)}
        onReset={() => {
          setDraft(saved);
          setNotice("");
        }}
      />
    </>
  );
}

export { Field, Picker } from "./profile/Fields";
