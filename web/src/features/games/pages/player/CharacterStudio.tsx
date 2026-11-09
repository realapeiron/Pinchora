import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { SaveBar } from "../../../../components/SaveBar";
import { t } from "../../../../i18n";
import { api } from "../../../../lib/api";
import {
  changeCharacterColor,
  characterKey,
  outfitChanged,
  usePlayerStudio,
  usePlayerPreview,
  wearAccessory,
  type CharacterData,
} from "../../api/webPlayer";
import { PageHead } from "../../../../components/PageHead";
import { Loading, Problem } from "../../../../components/States";
import { Field } from "./ProfileStudio";

export function CharacterStudio() {
  const query = usePlayerStudio<CharacterData>("character");
  return (
    <>
      <PageHead
        title={t("Character")}
        lead={t("A look for every adventure. Nothing changes until you save.")}
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
function Editor({ data }: { data: CharacterData }) {
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
  const [base = "", ...worn] = draft.split("+");
  const parts = base.split(".");
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
          {tab === "look" && (
            <Panel title={t("Look & colours")}>
              <Field label={t("Look")}>
                <select
                  value={parts[0]}
                  onChange={(e) => edit(changeCharacterColor(draft, "look", e.target.value))}
                >
                  {data.catalog.looks.map((look) => (
                    <option key={look.id} value={look.id}>
                      {look.name}
                    </option>
                  ))}
                </select>
              </Field>
              {data.catalog.channels.map((channel) => {
                const index = ["look", "skin", "hair", "top", "bottom", "eyes", "shoes"].indexOf(channel.id);
                const color = parts[index] ?? "0";
                return (
                  <fieldset className="player-colour" key={channel.id}>
                    <legend>{channel.name}</legend>
                    <div className="player-swatches">
                      {channel.swatches.map((sw) => (
                        <button
                          type="button"
                          key={sw.id}
                          aria-label={t("{channel} colour {number}", {
                            channel: channel.name,
                            number: sw.id + 1,
                          })}
                          aria-pressed={color === String(sw.id)}
                          onClick={() => edit(changeCharacterColor(draft, channel.id, String(sw.id)))}
                        >
                          <svg viewBox="0 0 32 32" aria-hidden="true">
                            <rect width="32" height="32" fill={sw.value} />
                          </svg>
                        </button>
                      ))}
                    </div>
                    <Field label={t("Custom {channel} colour", { channel: channel.name })}>
                      <input
                        maxLength={7}
                        placeholder={t("Hex colour (#rrggbb)")}
                        value={color.startsWith("#") ? color : ""}
                        onChange={(e) => edit(changeCharacterColor(draft, channel.id, e.target.value || "0"))}
                      />
                    </Field>
                  </fieldset>
                );
              })}
              <Field label={t("Start from a preset")}>
                <select
                  value=""
                  onChange={(e) => {
                    const preset = data.catalog.presets.find((p) => p.id === e.target.value);
                    if (preset) edit(preset.spec);
                  }}
                >
                  <option value="">{t("Choose a preset")}</option>
                  {data.catalog.presets.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </Field>
            </Panel>
          )}
          {tab === "accessories" && (
            <Panel title={t("Accessory wardrobe")}>
              <p className="muted">{t("One accessory per slot. Locked items explain how to unlock them.")}</p>
              <div className="player-collection">
                {data.catalog.accessories.map((a) => {
                  const current = worn.find((v) => v.split("*")[0] === a.id);
                  return (
                    <article key={a.id} className={`player-collectible ${current ? "is-picked" : ""}`}>
                      <h3>{a.name}</h3>
                      {a.owned ? (
                        <>
                          <PixelButton
                            variant="secondary"
                            aria-pressed={!!current}
                            onClick={() =>
                              edit(
                                wearAccessory(draft, a.slot, current ? null : a.id, data.catalog.accessories),
                              )
                            }
                          >
                            {current ? t("Remove") : t("Wear")}
                          </PixelButton>
                          {current && a.tintable && (
                            <Field label={t("Accessory tint")}>
                              <select
                                value={current.split("*")[1] ?? ""}
                                onChange={(e) =>
                                  edit(
                                    wearAccessory(
                                      draft,
                                      a.slot,
                                      a.id,
                                      data.catalog.accessories,
                                      e.target.value,
                                    ),
                                  )
                                }
                              >
                                <option value="">{t("Original colour")}</option>
                                {data.catalog.tints.map((v) => (
                                  <option key={v.id} value={v.id}>
                                    {v.value}
                                  </option>
                                ))}
                              </select>
                            </Field>
                          )}
                        </>
                      ) : a.buyable ? (
                        <PixelButton
                          disabled={buy.isPending || data.shells < a.price}
                          onClick={() => buy.mutate(a.id)}
                        >
                          {t("Buy · {price} shells", { price: a.price })}
                        </PixelButton>
                      ) : (
                        <p className="muted">{a.unlock}</p>
                      )}
                    </article>
                  );
                })}
              </div>
            </Panel>
          )}
          {tab === "outfits" && (
            <Panel title={t("Saved outfits")}>
              <p className="muted">
                {t("Keep three favourite looks. Loading an outfit changes your draft.")}
              </p>
              <div className="player-outfits">
                {[1, 2, 3].map((slot) => {
                  const savedOutfit = data.outfits.find((o) => o.slot === slot);
                  return (
                    <section key={slot}>
                      <h3>{t("Outfit {slot}", { slot })}</h3>
                      {savedOutfit && (
                        <img
                          src={`/api/me/character/preview.png?spec=${encodeURIComponent(savedOutfit.spec)}&scale=2`}
                          alt={savedOutfit.name}
                        />
                      )}
                      <Field label={t("Outfit name {slot}", { slot })}>
                        <input
                          maxLength={32}
                          value={names[slot] ?? savedOutfit?.name ?? ""}
                          onChange={(e) => setNames((p) => ({ ...p, [slot]: e.target.value }))}
                        />
                      </Field>
                      <div className="player-actions">
                        <PixelButton
                          variant="secondary"
                          disabled={
                            outfit.isPending ||
                            preview.pending ||
                            !!preview.error ||
                            !outfitChanged(
                              savedOutfit,
                              draft,
                              names[slot] ?? savedOutfit?.name ?? t("Outfit {slot}", { slot }),
                            )
                          }
                          onClick={() => outfit.mutate({ slot, remove: false })}
                        >
                          {t("Save to slot")}
                        </PixelButton>
                        {savedOutfit && (
                          <>
                            <PixelButton
                              variant="secondary"
                              disabled={draft === savedOutfit.spec}
                              onClick={() => edit(savedOutfit.spec)}
                            >
                              {t("Load outfit")}
                            </PixelButton>
                            <PixelButton
                              variant="ghost"
                              disabled={outfit.isPending}
                              onClick={() => outfit.mutate({ slot, remove: true })}
                            >
                              {t("Delete")}
                            </PixelButton>
                          </>
                        )}
                      </div>
                    </section>
                  );
                })}
              </div>
            </Panel>
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
