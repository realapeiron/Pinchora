import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { SaveBar } from "../../../../../components/SaveBar";
import { Loading } from "../../../../../components/States";
import { t } from "../../../../../i18n";
import { api } from "../../../../../lib/api";
import { characterKey, usePlayerPreview, type CharacterData } from "../../../api/webPlayer";
import { LookPanel } from "./LookPanel";
import { AccessoriesPanel } from "./AccessoriesPanel";
import { OutfitsPanel } from "./OutfitsPanel";

export function Editor({ data }: { data: CharacterData }) {
  const cache = useQueryClient();
  const [draft, setDraft] = useState(data.spec);
  const [saved, setSaved] = useState(data.spec);
  const [history, setHistory] = useState<string[]>([]);
  const [tab, setTab] = useState("look");
  const [notice, setNotice] = useState("");
  const [names, setNames] = useState<Record<number, string>>({});
  const preview = usePlayerPreview(`/api/me/character/preview.png?spec=${encodeURIComponent(draft)}&scale=4`);
  const edit = (next: string) => {
    if (next === draft) return;
    setHistory((h) => [...h.slice(-29), draft]);
    setDraft(next);
    setNotice("");
  };
  const save = useMutation({
    mutationFn: (submitted: string) => api<{ spec: string }>("/api/me/character", "PUT", { spec: submitted }),
    onSuccess: (v, submitted) => {
      setDraft((current) => (current === submitted ? v.spec : current));
      setSaved(v.spec);
      setNotice(t("Character saved."));
      void cache.invalidateQueries({ queryKey: ["player"] });
    },
  });
  const random = useMutation({
    mutationFn: () => api<{ spec: string }>("/api/me/character/randomise", "POST"),
    onSuccess: (v) => edit(v.spec),
  });
  const buy = useMutation({
    mutationFn: (id: string) => api(`/api/me/character/accessories/${id}/buy`, "POST"),
    onSuccess: () => {
      setNotice(t("Added to your collection."));
      void cache.invalidateQueries({ queryKey: ["player"] });
    },
  });
  const outfit = useMutation({
    mutationFn: ({ slot, remove }: { slot: number; remove: boolean }) =>
      api(
        `/api/me/character/outfits/${slot}`,
        remove ? "DELETE" : "PUT",
        remove
          ? {}
          : {
              spec: draft,
              name: (
                names[slot] ??
                data.outfits.find((o) => o.slot === slot)?.name ??
                t("Outfit {slot}", { slot })
              ).trim(),
            },
      ),
    onSuccess: () => {
      setNotice(t("Outfit collection updated."));
      void cache.invalidateQueries({ queryKey: ["player"] });
    },
  });
  const error = save.error ?? random.error ?? buy.error ?? outfit.error;
  return (
    <>
      <div className="player-studio">
        <aside className="player-preview">
          <Panel title={t("Live preview")}>
            <div className="player-character-stage">
              {preview.src ? <img src={preview.src} alt={t("Character preview")} /> : <Loading />}
            </div>
            <p className="muted" role="status">
              {preview.pending ? t("Updating preview…") : t("Your draft character")}
            </p>
            {preview.error && (
              <p role="alert" className="notice notice-heart">
                {preview.error.message}
                <PixelButton variant="secondary" onClick={() => void preview.retry()}>
                  {t("Retry preview")}
                </PixelButton>
              </p>
            )}
            <div className="player-actions">
              <PixelButton variant="secondary" onClick={() => random.mutate()} disabled={random.isPending}>
                {t("Randomise")}
              </PixelButton>
              <PixelButton
                variant="ghost"
                disabled={!history.length}
                onClick={() => {
                  const prev = history.at(-1);
                  if (prev) {
                    setDraft(prev);
                    setHistory(history.slice(0, -1));
                  }
                }}
              >
                {t("Undo")}
              </PixelButton>
            </div>
            <p>{t("{count} shells available", { count: data.shells })}</p>
          </Panel>
        </aside>
        <div className="player-controls">
          <nav className="player-tabs" aria-label={t("Character sections")}>
            {[
              ["look", t("Look & colours")],
              ["accessories", t("Accessories")],
              ["outfits", t("Outfits")],
            ].map(([key, label]) => (
              <button type="button" key={key} aria-pressed={tab === key} onClick={() => setTab(key!)}>
                {label}
              </button>
            ))}
          </nav>
          {tab === "look" && <LookPanel catalog={data.catalog} draft={draft} edit={edit} />}
          {tab === "accessories" && (
            <AccessoriesPanel
              catalog={data.catalog}
              shells={data.shells}
              draft={draft}
              edit={edit}
              buyPending={buy.isPending}
              onBuy={(id) => buy.mutate(id)}
            />
          )}
          {tab === "outfits" && (
            <OutfitsPanel
              outfits={data.outfits}
              draft={draft}
              names={names}
              setNames={setNames}
              edit={edit}
              outfitPending={outfit.isPending}
              saveBlocked={preview.pending || !!preview.error}
              onOutfit={(slot, remove) => outfit.mutate({ slot, remove })}
            />
          )}
          {error && (
            <p role="alert" className="notice notice-heart">
              {error.message}
            </p>
          )}
          <p role="status" className="player-status">
            {notice}
          </p>
        </div>
      </div>
      <SaveBar
        count={characterKey(draft) === characterKey(saved) ? 0 : 1}
        saving={save.isPending}
        blocked={preview.pending || !!preview.error}
        error={save.error?.message}
        onSave={() => save.mutate(draft)}
        onReset={() => {
          setDraft(saved);
          setHistory([]);
        }}
      />
    </>
  );
}
