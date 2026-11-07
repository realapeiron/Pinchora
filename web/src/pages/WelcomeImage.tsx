import { translate as spaTranslate } from "../i18n";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useOutletContext } from "react-router";

import { CardCanvas } from "../components/card/CardCanvas";
import { CanvasProperties, Dialog, LayerProperties, StickerPicker } from "../components/card/CardProperties";
import { type AtlasIndex, CardSvg } from "../components/card/CardSvg";
import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PreviewFrame } from "../components/PreviewFrame";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type Me, type Overview, api, apiUpload } from "../lib/api";
import {
  type CardDesign,
  type CardKind,
  type CardUpload,
  type CardView,
  type Cards,
  type Delivery,
  type Fill,
  type History,
  type Layer,
  CARD,
  DELIVERY_LABELS,
  commit,
  costShare,
  designCost,
  fitDesign,
  heavyMessage,
  history,
  layerLabel,
  newId,
  newLayer,
  push,
  redo,
  replace,
  undo,
  unknownPlaceholders,
} from "../lib/card";
import { cardsKey, useAtlasIndex, useCards, useDiscord, useMe } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

import "../styles/card-editor.css";

/** One image in the editor: its design, how it is sent, and whether it is the server's own design. */
interface Side {
  design: CardDesign;
  delivery: Delivery;
  custom: boolean;
}

type Draft = Record<CardKind, Side>;

const KINDS: { kind: CardKind; label: string }[] = [
  { kind: "welcome", label: "Welcome" },
  { kind: "goodbye", label: "Goodbye" },
];

/** The member number the canvas shows, as the bot's preview does. */
const SAMPLE_MEMBERS = 1234;
const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2];

const sideOf = (view: CardView): Side => ({
  design: view.design,
  delivery: view.delivery,
  custom: view.custom,
});
const same = (a: Side, b: Side) => JSON.stringify(a) === JSON.stringify(b);

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const list = window.matchMedia(query);
    const update = () => setMatches(list.matches);
    list.addEventListener("change", update);
    return () => list.removeEventListener("change", update);
  }, [query]);
  return matches;
}

/** What the browser can tell is wrong with a design before the server sees it. */
function problems(design: CardDesign): string[] {
  const layers = design.layers.flatMap((layer, i) => {
    if (layer.type !== "text") return [];
    const name = `Layer ${i + 1} (Text)`;
    if (layer.text.trim() === "") return [`${name}: write something, or delete the layer.`];
    const unknown = unknownPlaceholders(layer.text);
    return unknown.length ? [`${name}: ${unknown[0]} isn't a placeholder.`] : [];
  });
  const avatars = design.layers.filter((l) => l.type === "avatar").length;
  return [
    ...layers,
    ...(avatars > CARD.max_avatars ? [`A design has at most ${CARD.max_avatars} avatars.`] : []),
    ...(designCost(design.layers) > CARD.layer_cost_max ? [heavyMessage(design.layers)] : []),
  ];
}

export function WelcomeImage() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const cards = useCards(guild);
  const atlas = useAtlasIndex();
  const me = useMe();
  if (cards.isPending || me.isPending) return <Loading label="Opening the editor" />;
  if (cards.isError) return <Problem error={cards.error} />;
  if (me.isError) return <Problem error={me.error} />;
  return (
    <Editor
      key={guild}
      guild={guild}
      server={data.guild.name}
      cards={cards.data}
      me={me.data}
      atlas={atlas.data}
    />
  );
}

function Editor({
  guild,
  server,
  cards,
  me,
  atlas,
}: {
  guild: string;
  server: string;
  cards: Cards;
  me: Me;
  atlas: AtlasIndex | undefined;
}) {
  const client = useQueryClient();
  const discord = useDiscord(guild);
  const simple = useMediaQuery("(max-width: 767px)");
  const [saved, setSaved] = useState<Draft>(() => ({
    welcome: sideOf(cards.welcome),
    goodbye: sideOf(cards.goodbye),
  }));
  const [hist, setHist] = useState<History<Draft>>(() => history(saved));
  const [kind, setKind] = useState<CardKind>("welcome");
  const [selected, setSelected] = useState<string | null>(null);
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [fitZoom, setFitZoom] = useState(0.5);
  const [snapping, setSnapping] = useState(true);
  const [dialog, setDialog] = useState<null | "templates" | "sticker" | "swap-sticker">(null);
  const [saving, setSaving] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const [toast, setToast] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [testChannel, setTestChannel] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [showBot, setShowBot] = useState(false);
  const lastEdit = useRef<{ key: string; at: number } | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const viewport = useRef<HTMLDivElement>(null);

  const draft = hist.present;
  const side = draft[kind];
  const design = side.design;
  const layer = design.layers.find((l) => l.id === selected) ?? null;
  const uploads = cards.uploads;
  const uploadUrls = useMemo(() => Object.fromEntries(uploads.map((u) => [u.id, u.url])), [uploads]);
  const fill: Fill = {
    user: me.user.global_name ?? me.user.username,
    username: me.user.username,
    server,
    members: SAMPLE_MEMBERS,
  };
  const changed = KINDS.filter(({ kind: k }) => !same(draft[k], saved[k])).length;
  const local = problems(design);
  const scale = zoom === "fit" ? fitZoom : zoom;
  const share = costShare(design.layers);

  // /Edits

  /** Applies a change to the draft; edits under the same `merge` key within a second are one undo step. */
  const apply = useCallback((next: Draft, merge?: string) => {
    const now = Date.now();
    const last = lastEdit.current;
    const joined = merge && last && last.key === merge && now - last.at < 1000;
    lastEdit.current = merge ? { key: merge, at: now } : null;
    setHist((h) => (joined ? replace(h, next) : push(h, next)));
    setErrors([]);
  }, []);

  const setSide = (next: Partial<Side>, merge?: string) =>
    apply({ ...draft, [kind]: { ...side, ...next } }, merge);
  // Whatever changes, no layer ends up bigger than the canvas.
  const setDesign = (next: CardDesign, merge?: string) =>
    setSide({ design: fitDesign(next), custom: true }, merge && `${kind}:${merge}`);
  const setLayer = (next: Layer, merge?: string) =>
    setDesign(
      { ...design, layers: design.layers.map((l) => (l.id === next.id ? next : l)) },
      merge && `${next.id}:${merge}`,
    );

  const addLayer = (next: Layer) => {
    if (design.layers.length >= CARD.max_layers) {
      setToast(`A design has at most ${CARD.max_layers} layers.`);
      return;
    }
    if (
      next.type === "avatar" &&
      design.layers.filter((l) => l.type === "avatar").length >= CARD.max_avatars
    ) {
      setToast(`A design has at most ${CARD.max_avatars} avatars.`);
      return;
    }
    if (designCost([...design.layers, next]) > CARD.layer_cost_max) {
      setToast("That would make the image too heavy to draw quickly. Shrink or hide a layer first.");
      return;
    }
    setDesign({ ...design, layers: [...design.layers, next] });
    setSelected(next.id);
  };
  const removeLayer = (id: string) => {
    setDesign({ ...design, layers: design.layers.filter((l) => l.id !== id) });
    setSelected(null);
  };
  const duplicate = (source: Layer) => {
    const copy = {
      ...source,
      id: newId(design, source.type),
      x: source.x + 16,
      y: source.y + 16,
      locked: false,
    };
    addLayer(copy);
  };
  /** Moves a layer up (`+1`, towards the top) or down the stack. */
  const restack = (id: string, by: number) => {
    const from = design.layers.findIndex((l) => l.id === id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= design.layers.length) return;
    const layers = [...design.layers];
    const [moved] = layers.splice(from, 1);
    if (!moved) return;
    layers.splice(to, 0, moved);
    setDesign({ ...design, layers });
  };

  // A drag shows as it goes and becomes one undo step when it ends.
  // The draft as the drag found it; a click that moved nothing leaves no step.
  const dragFrom = useRef<Draft | null>(null);
  const onLive = (next: CardDesign) =>
    setHist((h) => {
      dragFrom.current ??= h.present;
      return replace(h, { ...h.present, [kind]: { ...h.present[kind], design: next, custom: true } });
    });
  const onCommit = () => {
    const from = dragFrom.current;
    dragFrom.current = null;
    if (from) setHist((h) => commit(h, from));
    setErrors([]);
  };

  // /Keyboard: undo, redo, nudge, delete, duplicate.

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (target.closest("input, textarea, select, [contenteditable]") || dialog) return;
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setHist((h) => (event.shiftKey ? redo(h) : undo(h)));
      } else if (mod && event.key.toLowerCase() === "y") {
        event.preventDefault();
        setHist(redo);
      } else if (layer && mod && event.key.toLowerCase() === "d") {
        event.preventDefault();
        duplicate(layer);
      } else if (layer && (event.key === "Delete" || event.key === "Backspace")) {
        event.preventDefault();
        removeLayer(layer.id);
      } else if (layer && !layer.locked && event.key.startsWith("Arrow")) {
        event.preventDefault();
        const step = event.shiftKey ? 10 : 1;
        const moves: Record<string, [number, number]> = {
          ArrowLeft: [-step, 0],
          ArrowRight: [step, 0],
          ArrowUp: [0, -step],
          ArrowDown: [0, step],
        };
        const [dx, dy] = moves[event.key] ?? [0, 0];
        setLayer({ ...layer, x: layer.x + dx, y: layer.y + dy }, "nudge");
      } else if (event.key === "Escape") {
        setSelected(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // /Zoom: "fit" follows the space the canvas has.

  useLayoutEffect(() => {
    const box = viewport.current;
    if (!box) return;
    const measure = () => setFitZoom(Math.min(1, Math.max(0.1, (box.clientWidth - 8) / design.width)));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [design.width]);

  const zoomBy = (direction: 1 | -1) => {
    const now = scale;
    const next =
      direction > 0 ? ZOOMS.find((z) => z > now + 0.001) : [...ZOOMS].reverse().find((z) => z < now - 0.001);
    if (next) setZoom(next);
  };

  // /Saving, uploading, testing

  const save = async () => {
    setSaving(true);
    setErrors([]);
    const kinds = KINDS.map((k) => k.kind).filter((k) => !same(draft[k], saved[k]));
    try {
      const views = await Promise.all(
        kinds.map((k) =>
          api<CardView>(`/api/guilds/${guild}/cards/${k}`, "PUT", {
            design: draft[k].custom ? draft[k].design : null,
            delivery: draft[k].delivery,
          }),
        ),
      );
      const next = { ...saved };
      for (const view of views) next[view.kind] = sideOf(view);
      setSaved(next);
      setHist((h) =>
        replace(h, { ...h.present, ...Object.fromEntries(views.map((v) => [v.kind, sideOf(v)])) }),
      );
      client.setQueryData<Cards>(cardsKey(guild), (old) =>
        old ? { ...old, ...Object.fromEntries(views.map((v) => [v.kind, v])) } : old,
      );
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
      setToast("Saved. The next member who joins gets it.");
    } catch (error) {
      setErrors(
        error instanceof ApiError
          ? error.errors.map((e) => e.message).concat(error.errors.length ? [] : [error.message])
          : ["The design could not be saved."],
      );
    } finally {
      setSaving(false);
    }
  };

  const upload = async (file: File) => {
    if (file.size > CARD.upload_max_bytes) {
      setErrors([`A picture is at most ${CARD.upload_max_bytes / 1024 / 1024} MB.`]);
      return;
    }
    setUploading(true);
    try {
      const made = await apiUpload<CardUpload>(`/api/guilds/${guild}/cards/uploads`, file);
      client.setQueryData<Cards>(cardsKey(guild), (old) =>
        old ? { ...old, uploads: [made, ...old.uploads] } : old,
      );
      setDesign({ ...design, background: { type: "upload", id: made.id } });
    } catch (error) {
      setErrors([error instanceof ApiError ? error.message : "The picture could not be uploaded."]);
    } finally {
      setUploading(false);
    }
  };

  const sendTest = async () => {
    if (!testChannel) return;
    setTesting(true);
    try {
      await api(`/api/guilds/${guild}/cards/${kind}/test`, "POST", {
        design,
        delivery: side.delivery,
        channel_id: testChannel,
      });
      const name = discord.data?.channels.find((c) => c.id === testChannel)?.name;
      setToast(`Test posted${name ? ` in #${name}` : ""}.`);
    } catch (error) {
      setErrors(error instanceof ApiError ? [error.message] : ["The test could not be sent."]);
    } finally {
      setTesting(false);
    }
  };

  const reset = () => {
    setHist((h) => push(h, saved));
    setErrors([]);
    setSelected(null);
  };

  const imageOff = side.delivery === "text_only";
  const ordered = [...design.layers].reverse();

  return (
    <div className="settings-page card-editor">
      <PageHead
        title="WELCOME IMAGE"
        lead="Design the picture new members get, and the one that says goodbye. Drag things around on the canvas; Try it, further down, shows the picture the bot really draws."
      />

      <div className="card-toolbar" role="toolbar" aria-label="Editor">
        <div className="segmented" role="tablist" aria-label="Which image">
          {KINDS.map((k) => (
            <button
              key={k.kind}
              type="button"
              role="tab"
              aria-selected={kind === k.kind}
              className={`segment ${kind === k.kind ? "is-checked" : ""}`}
              onClick={() => {
                setKind(k.kind);
                setSelected(null);
              }}
            >
              {k.label}
              {!same(draft[k.kind], saved[k.kind]) && <span className="dot" aria-label="unsaved" />}
            </button>
          ))}
        </div>
        <label className="card-send">
          <span className="pixel-label">SEND</span>
          <select
            className="pixel-input pixel-select"
            aria-label="How the image is sent"
            value={side.delivery}
            onChange={(event) => setSide({ delivery: event.target.value as Delivery })}
          >
            {cards[kind].deliveries.map((d) => (
              <option key={d} value={d}>
                {DELIVERY_LABELS[d]}
              </option>
            ))}
          </select>
        </label>
        <span className="card-toolbar-gap" />
        <PixelButton variant="secondary" className="button-small" onClick={() => setDialog("templates")}>
          Templates
        </PixelButton>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => setHist(undo)}
          disabled={hist.past.length === 0}
          aria-label={spaTranslate("Undo")}
        >
          {spaTranslate("↶ Undo")}
        </PixelButton>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => setHist(redo)}
          disabled={hist.future.length === 0}
          aria-label="Redo"
        >
          ↷ Redo
        </PixelButton>
      </div>

      {imageOff && (
        <p className="notice">
          This {kind} goes out as text only, so the image is not sent. Pick another way to send it to use the
          design.
        </p>
      )}

      <div className={`card-workspace ${simple ? "is-simple" : ""}`}>
        <div className="card-center">
          <div className="card-zoom" role="group" aria-label="Zoom">
            <PixelButton
              variant="ghost"
              className="button-small"
              onClick={() => zoomBy(-1)}
              aria-label={spaTranslate("Zoom out")}
            >
              −
            </PixelButton>
            <span className="pixel-label card-zoom-value">{Math.round(scale * 100)}%</span>
            <PixelButton
              variant="ghost"
              className="button-small"
              onClick={() => zoomBy(1)}
              aria-label={spaTranslate("Zoom in")}
            >
              +
            </PixelButton>
            <PixelButton
              variant="ghost"
              className="button-small"
              onClick={() => setZoom("fit")}
              aria-pressed={zoom === "fit"}
            >
              Fit
            </PixelButton>
            <label className="card-snap">
              <input
                type="checkbox"
                checked={snapping}
                onChange={(event) => setSnapping(event.target.checked)}
              />{" "}
              Snap
            </label>
            <span className="muted fine card-size">
              {design.width}×{design.height}
            </span>
          </div>
          <div className="card-viewport" ref={viewport}>
            <CardCanvas
              design={design}
              zoom={scale}
              selected={selected}
              snapping={snapping}
              fill={fill}
              avatarUrl={me.user.avatar_url}
              uploads={uploadUrls}
              atlas={atlas}
              onSelect={setSelected}
              onLive={onLive}
              onCommit={onCommit}
            />
          </div>
        </div>
        <div className="card-aside">
          <div className="card-layers">
            <Panel
              title="LAYERS"
              aside={
                <span className="counter">
                  {design.layers.length}/{CARD.max_layers}
                </span>
              }
            >
              <div className="layer-add">
                <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "text"))}>
                  + Text
                </PixelButton>
                <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "avatar"))}>
                  + Avatar
                </PixelButton>
                <PixelButton className="button-small" onClick={() => setDialog("sticker")}>
                  + Sticker
                </PixelButton>
                <PixelButton className="button-small" onClick={() => addLayer(newLayer(design, "shape"))}>
                  + Shape
                </PixelButton>
              </div>
              <div className={`cost-meter ${share > 100 ? "is-over" : share >= 80 ? "is-high" : ""}`}>
                <span className="cost-label">
                  <span className="pixel-label">DRAW COST</span>
                  <span>{share}%</span>
                </span>
                <span
                  className="cost-bar"
                  role="meter"
                  aria-label="How much of a card's drawing allowance the visible layers use"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.min(100, share)}
                >
                  <span style={{ width: `${Math.min(100, share)}%` }} />
                </span>
              </div>
              <ol className="layer-list" aria-label="Layers, top first">
                {ordered.map((l) => {
                  const index = design.layers.indexOf(l);
                  return (
                    <li
                      key={l.id}
                      className={`layer-row ${l.id === selected ? "is-selected" : ""} ${l.hidden ? "is-hidden" : ""}`}
                    >
                      <button
                        type="button"
                        className="layer-icon"
                        aria-label={l.hidden ? `Show ${layerLabel(l)}` : `Hide ${layerLabel(l)}`}
                        aria-pressed={!l.hidden}
                        onClick={() => setLayer({ ...l, hidden: !l.hidden })}
                      >
                        {l.hidden ? "◌" : "●"}
                      </button>
                      <button
                        type="button"
                        className="layer-icon layer-lock"
                        aria-label={l.locked ? `Unlock ${layerLabel(l)}` : `Lock ${layerLabel(l)}`}
                        aria-pressed={l.locked}
                        onClick={() => setLayer({ ...l, locked: !l.locked })}
                      >
                        {l.locked ? "🔒" : "🔓"}
                      </button>
                      <button
                        type="button"
                        className="layer-name"
                        onClick={() => setSelected(l.id)}
                        aria-current={l.id === selected || undefined}
                      >
                        <span className="layer-kind pixel-label">
                          {l.type === "sprite" ? "STICKER" : l.type.toUpperCase()}
                        </span>
                        <span className="layer-label">{layerLabel(l)}</span>
                      </button>
                      <span className="layer-moves">
                        <button
                          type="button"
                          className="layer-icon"
                          aria-label={`Move ${layerLabel(l)} up`}
                          disabled={index === design.layers.length - 1}
                          onClick={() => restack(l.id, 1)}
                        >
                          ▲
                        </button>
                        <button
                          type="button"
                          className="layer-icon"
                          aria-label={`Move ${layerLabel(l)} down`}
                          disabled={index === 0}
                          onClick={() => restack(l.id, -1)}
                        >
                          ▼
                        </button>
                      </span>
                    </li>
                  );
                })}
                {design.layers.length === 0 && (
                  <li className="empty-row muted">
                    Nothing on the canvas yet. Add a layer or pick a template.
                  </li>
                )}
              </ol>
              {layer && (
                <div className="layer-actions">
                  <PixelButton variant="secondary" className="button-small" onClick={() => duplicate(layer)}>
                    {spaTranslate("Duplicate")}
                  </PixelButton>
                  <PixelButton variant="ghost" className="button-small" onClick={() => removeLayer(layer.id)}>
                    {spaTranslate("Delete")}
                  </PixelButton>
                </div>
              )}
            </Panel>
          </div>
          <div className="card-props">
            <Panel title="PROPERTIES">
              {layer ? (
                <LayerProperties
                  layer={layer}
                  canvas={design}
                  onChange={setLayer}
                  onPickSticker={() => setDialog("swap-sticker")}
                  simple={simple}
                />
              ) : (
                <CanvasProperties
                  design={design}
                  onChange={setDesign}
                  atlas={atlas}
                  uploads={uploads}
                  onUpload={upload}
                  uploading={uploading}
                  simple={simple}
                />
              )}
            </Panel>
          </div>
        </div>
      </div>

      {(errors.length > 0 || local.length > 0) && (
        <Panel title="FIX THIS FIRST" tone="accent">
          <ul className="card-errors" role="alert">
            {[...local, ...errors].map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        </Panel>
      )}

      <Panel
        title="TRY IT"
        aside={
          <PixelButton
            variant="secondary"
            className="button-small"
            aria-expanded={showBot}
            aria-controls="card-bot-preview"
            onClick={() => setShowBot((on) => !on)}
          >
            {showBot ? "Hide the bot's picture" : "Show the bot's picture"}
          </PixelButton>
        }
      >
        <div className="card-test">
          <Field
            label="Post a test in"
            htmlFor="card-test-channel"
            hint="With you as the member, pinging nobody. A direct message is tried out here too."
          >
            <ChannelPicker
              id="card-test-channel"
              info={discord.data}
              value={testChannel}
              onChange={setTestChannel}
            />
          </Field>
          <PixelButton onClick={sendTest} disabled={!testChannel || testing || local.length > 0}>
            {testing ? "Sending…" : "Send test"}
          </PixelButton>
        </div>
        {showBot && (
          <div className="card-bot-preview" id="card-bot-preview">
            <h3 className="pixel-label">WHAT THE BOT POSTS</h3>
            <PreviewFrame
              path={`/api/guilds/${guild}/preview/card`}
              body={local.length === 0 ? { design } : null}
              alt={`The ${kind} image the bot draws`}
              width={design.width}
              height={design.height}
              delay={1500}
            />
            <p className="muted fine">
              Drawn by the bot with you as the member, from the design above, at most twenty times a minute
              for the whole server.{" "}
              {side.custom ? "" : "This is the classic card from /config welcome-card until you change it."}
            </p>
          </div>
        )}
      </Panel>

      {dialog === "templates" && (
        <Dialog title="TEMPLATES" onClose={() => setDialog(null)}>
          <p className="muted">Start from one of these; your current design stays one undo away.</p>
          <div className="template-grid">
            {cards.templates.map((t) => (
              <button
                key={t.key}
                type="button"
                className="template"
                onClick={() => {
                  setDesign(t[kind]);
                  setSelected(null);
                  setDialog(null);
                }}
              >
                <span className="template-frame">
                  <CardSvg
                    design={t[kind]}
                    fill={fill}
                    avatarUrl={me.user.avatar_url}
                    uploads={uploadUrls}
                    atlas={atlas}
                    width={240}
                    className="template-art"
                  />
                </span>
                <span className="template-name">{t.name}</span>
              </button>
            ))}
          </div>
        </Dialog>
      )}
      {(dialog === "sticker" || dialog === "swap-sticker") && (
        <StickerPicker
          atlas={atlas}
          onClose={() => setDialog(null)}
          onPick={(sprite) => {
            if (dialog === "swap-sticker" && layer?.type === "sprite") {
              const k = Math.max(1, Math.round(layer.h / sprite.h));
              setLayer({ ...layer, sprite: sprite.id, w: sprite.w * k, h: sprite.h * k });
            } else {
              addLayer(newLayer(design, "sprite", sprite));
            }
            setDialog(null);
          }}
        />
      )}

      <SaveBar
        count={changed}
        saving={saving}
        blocked={local.length > 0}
        error={local.length > 0 ? "Fix the marked layers first" : null}
        onSave={save}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </div>
  );
}
