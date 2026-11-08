import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Panel } from "../../components/Panel";
import { PixelButton } from "../../components/PixelButton";
import { SaveBar } from "../../components/SaveBar";
import { t, number } from "../../i18n";
import { api } from "../../lib/api";
import {
  usePlayerStudio,
  usePlayerPreview,
  profileChanges,
  reorderBadge,
  toggleBadge,
  type ProfileData,
  type ProfileDraft,
  type Choice,
} from "../../lib/webPlayer";
import { PageHead } from "../PageHead";
import { Loading, Problem } from "../States";

type Tab = "badges" | "background" | "style" | "title" | "showcase" | "bio";
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
  const cache = useQueryClient();
  const [draft, setDraft] = useState(data.profile);
  const [saved, setSaved] = useState(data.profile);
  const [tab, setTab] = useState<Tab>("badges");
  const [notice, setNotice] = useState("");
  const [dragged, setDragged] = useState<number | null>(null);
  useEffect(() => {
    if (tab === "badges") void api("/api/me/badges/seen", "POST").catch(() => {});
  }, [tab]);
  const preview = usePlayerPreview(
    `/api/me/preview/profile-card.png?draft=${encodeURIComponent(JSON.stringify(draft))}`,
  );
  const update = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => {
    setDraft((p) => ({ ...p, [key]: value }));
    setNotice("");
  };
  const save = useMutation({
    mutationFn: (submitted: ProfileDraft) => api<ProfileDraft>("/api/me/profile", "PUT", submitted),
    onSuccess: (p, submitted) => {
      setSaved(p);
      setDraft((current) => (profileChanges(current, submitted) === 0 ? p : current));
      setNotice(t("Profile saved. Your next /profile will show this card."));
      void cache.invalidateQueries({ queryKey: ["player"] });
      void cache.invalidateQueries({ queryKey: ["player-preview"] });
    },
  });
  const buy = useMutation({
    mutationFn: ({ kind, id }: { kind: string; id: string }) =>
      api(`/api/me/${kind}/${encodeURIComponent(id)}/buy`, "POST"),
    onSuccess: () => {
      setNotice(t("Added to your collection."));
      void cache.invalidateQueries({ queryKey: ["player"] });
    },
  });
  const pick = (id: string) => {
    setDraft((p) => ({ ...p, badges: toggleBadge(p.badges, id), badges_set: true }));
    setNotice("");
  };
  const move = (from: number, to: number) => {
    setDraft((p) => ({ ...p, badges: reorderBadge(p.badges, from, to), badges_set: true }));
    setNotice(t("Badge moved to position {position}.", { position: to + 1 }));
  };
  const tabs: [Tab, string][] = [
    ["badges", t("Badges")],
    ["background", t("Background")],
    ["style", t("Style")],
    ["title", t("Title")],
    ["showcase", t("Showcase")],
    ["bio", t("Bio")],
  ];
  const unownedBackground =
    !!draft.background && !data.backgrounds.find((b) => b.id === draft.background)?.owned;
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
            <Panel title={t("Your badge collection")} aside={<span>{draft.badges.length}/6</span>}>
              <label className="player-check">
                <input
                  type="checkbox"
                  checked={!draft.badges_set}
                  onChange={(e) =>
                    setDraft((p) => ({
                      ...p,
                      badges_set: !e.target.checked,
                      badges: e.target.checked ? [] : p.badges,
                    }))
                  }
                />
                {t("Automatically show my best badges")}
              </label>
              <p className="muted">
                {t(
                  "Choose up to six. Drag selected badges to reorder, or use the move buttons and Alt + arrow keys.",
                )}
              </p>
              <ol className="player-badge-order" aria-label={t("Badge order")}>
                {draft.badges.map((id, i) => {
                  const badge = data.badges.find((b) => b.id === id);
                  return (
                    <li
                      key={id}
                      draggable
                      onDragStart={() => setDragged(i)}
                      onDragEnd={() => setDragged(null)}
                      onDragOver={(e) => e.preventDefault()}
                      onDrop={(e) => {
                        e.preventDefault();
                        if (dragged !== null) move(dragged, i);
                        setDragged(null);
                      }}
                    >
                      <button
                        type="button"
                        className="player-badge-handle"
                        aria-label={t("Reorder {name}", { name: badge?.name ?? id })}
                        onKeyDown={(e) => {
                          if (!e.altKey) return;
                          if (e.key === "ArrowUp" && i > 0) {
                            e.preventDefault();
                            move(i, i - 1);
                          }
                          if (e.key === "ArrowDown" && i < draft.badges.length - 1) {
                            e.preventDefault();
                            move(i, i + 1);
                          }
                        }}
                      >
                        {i + 1}. {badge?.name ?? id}
                      </button>
                      <button
                        type="button"
                        disabled={i === 0}
                        onClick={() => move(i, i - 1)}
                        aria-label={t("Move {name} earlier", { name: badge?.name ?? id })}
                      >
                        {t("Earlier")}
                      </button>
                      <button
                        type="button"
                        disabled={i === draft.badges.length - 1}
                        onClick={() => move(i, i + 1)}
                        aria-label={t("Move {name} later", { name: badge?.name ?? id })}
                      >
                        {t("Later")}
                      </button>
                      <button
                        type="button"
                        onClick={() => pick(id)}
                        aria-label={t("Remove {name}", { name: badge?.name ?? id })}
                      >
                        {t("Remove")}
                      </button>
                    </li>
                  );
                })}
              </ol>
              <div className="player-collection">
                {data.badges.map((badge) => (
                  <article
                    key={badge.id}
                    className={`player-collectible ${draft.badges.includes(badge.id) && draft.badges_set ? "is-picked" : ""}`}
                  >
                    <img
                      src={`/api/me/art/badge/${badge.id}`}
                      alt=""
                      className="player-badge-art"
                      loading="lazy"
                    />
                    <h3>{badge.name}</h3>
                    <p className="muted">{badge.description}</p>
                    {badge.new && <span className="pixel-label">{t("New")}</span>}
                    {badge.progress !== null && badge.target !== null && (
                      <>
                        <progress
                          aria-label={badge.name}
                          max={badge.target}
                          value={Math.min(badge.progress, badge.target)}
                        />
                        <small>
                          {number(badge.progress)} / {number(badge.target)} {badge.unit}
                        </small>
                      </>
                    )}
                    {badge.earned ? (
                      <PixelButton
                        variant="secondary"
                        aria-pressed={draft.badges.includes(badge.id) && draft.badges_set}
                        disabled={!draft.badges.includes(badge.id) && draft.badges.length >= 6}
                        onClick={() => pick(badge.id)}
                      >
                        {draft.badges.includes(badge.id) ? t("Remove") : t("Select badge")}
                      </PixelButton>
                    ) : badge.price ? (
                      <PixelButton
                        variant="secondary"
                        disabled={buy.isPending || data.shells < badge.price}
                        onClick={() => buy.mutate({ kind: "badges", id: badge.id })}
                      >
                        {t("Buy · {price} shells", { price: badge.price })}
                      </PixelButton>
                    ) : (
                      <span className="muted">{t("Locked")}</span>
                    )}
                  </article>
                ))}
              </div>
            </Panel>
          )}
          {tab === "background" && (
            <Panel title={t("Card backgrounds")}>
              <PixelButton
                variant="secondary"
                aria-pressed={draft.background === null}
                onClick={() => update("background", null)}
              >
                {t("Plain background")}
              </PixelButton>
              <div className="player-collection">
                {data.backgrounds.map((bg) => (
                  <article
                    key={bg.id}
                    className={`player-collectible ${draft.background === bg.id ? "is-picked" : ""}`}
                  >
                    <img
                      className="player-background-art"
                      src={`/api/me/art/background/${bg.id}`}
                      alt=""
                      loading="lazy"
                    />
                    <h3>{bg.name}</h3>
                    <PixelButton
                      variant="secondary"
                      aria-pressed={draft.background === bg.id}
                      onClick={() => update("background", bg.id)}
                    >
                      {bg.owned ? t("Equip") : t("Try background")}
                    </PixelButton>
                    {!bg.owned && (
                      <PixelButton
                        disabled={buy.isPending || data.shells < bg.price}
                        onClick={() => buy.mutate({ kind: "backgrounds", id: bg.id })}
                      >
                        {t("Buy · {price} shells", { price: bg.price })}
                      </PixelButton>
                    )}
                  </article>
                ))}
              </div>
            </Panel>
          )}
          {tab === "style" && (
            <Panel title={t("Card style")}>
              <Field label={t("Accent colour")}>
                <input
                  value={draft.accent ?? ""}
                  placeholder={t("Background default")}
                  maxLength={7}
                  onChange={(e) => update("accent", e.target.value || null)}
                />
              </Field>
              <div className="player-swatches">
                {data.accents.map((a) => (
                  <button
                    type="button"
                    key={a.value}
                    title={a.name}
                    aria-label={a.name}
                    aria-pressed={draft.accent === a.value}
                    onClick={() => update("accent", a.value)}
                  >
                    <svg viewBox="0 0 32 32" aria-hidden="true">
                      <rect width="32" height="32" fill={a.value} />
                    </svg>
                  </button>
                ))}
              </div>
              <Field label={t("Panel opacity")}>
                <select
                  value={draft.panel_opacity}
                  onChange={(e) => update("panel_opacity", Number(e.target.value))}
                >
                  {data.opacities.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.name}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("Frame")}>
                <select value={draft.frame} onChange={(e) => update("frame", e.target.value)}>
                  {data.frames.map((f) => (
                    <option key={f.id} value={f.id} disabled={data.level < f.level}>
                      {f.name}
                      {data.level < f.level ? ` · ${t("Level {level}", { level: f.level })}` : ""}
                    </option>
                  ))}
                </select>
              </Field>
              <PixelButton
                variant="secondary"
                onClick={() => setDraft((p) => ({ ...p, accent: null, panel_opacity: 1, frame: "plain" }))}
              >
                {t("Reset style")}
              </PixelButton>
            </Panel>
          )}
          {tab === "title" && (
            <Panel title={t("Your title")}>
              <Picker
                label={t("Title")}
                value={draft.title}
                options={data.titles.filter((v) => v.owned)}
                onChange={(v) => update("title", v)}
              />
              <ul className="player-unlocks">
                {data.titles
                  .filter((v) => !v.owned)
                  .map((v) => (
                    <li key={v.id}>
                      <strong>{v.name}</strong>
                      <span className="muted">{v.description}</span>
                    </li>
                  ))}
              </ul>
            </Panel>
          )}
          {tab === "showcase" && (
            <Panel title={t("Showcase your favourites")}>
              <Picker
                label={t("Fish")}
                value={draft.showcase_fish}
                options={data.showcase.fish}
                onChange={(v) => update("showcase_fish", v)}
              />
              <Picker
                label={t("Pet")}
                value={draft.showcase_pet}
                options={data.showcase.pets}
                onChange={(v) => update("showcase_pet", v)}
              />
              <Picker
                label={t("Furniture")}
                value={draft.showcase_item}
                options={data.showcase.items}
                onChange={(v) => update("showcase_item", v)}
              />
              <label className="player-check">
                <input
                  type="checkbox"
                  checked={draft.showcase_house}
                  disabled={!data.showcase.house}
                  onChange={(e) => update("showcase_house", e.target.checked)}
                />
                {t("Show my house")}
              </label>
              {!data.showcase.house && (
                <p className="muted">{t("Open /house in Discord to start your home.")}</p>
              )}
            </Panel>
          )}
          {tab === "bio" && (
            <Panel title={t("About you")}>
              <Field label={t("Bio")}>
                <textarea rows={6} value={draft.bio} onChange={(e) => update("bio", e.target.value)} />
              </Field>
              <p className="muted">{[...draft.bio].length}/300</p>
            </Panel>
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
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="player-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Picker({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: Choice[];
  onChange: (value: string | null) => void;
}) {
  return (
    <Field label={label}>
      <select value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">{t("None")}</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.name}
          </option>
        ))}
      </select>
    </Field>
  );
}
