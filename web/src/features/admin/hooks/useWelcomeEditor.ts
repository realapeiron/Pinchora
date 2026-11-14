import { useMediaQuery } from "./useMediaQuery";
import { createWelcomeOperations } from "../pages/welcome/welcomeOperations";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { t as deferredText } from "../../../i18n/dashboard";
import { type Me, api } from "../../../lib/api";
import { type AtlasIndex } from "../../../components/card/CardSvg";
import {
  type CardDesign,
  type CardKind,
  type Cards,
  type Fill,
  type History,
  type Layer,
  CARD,
  commit,
  costShare,
  designCost,
  fitDesign,
  history,
  newId,
  push,
  replace,
} from "../../../lib/card";
import { useDiscord } from "../../../lib/queries";
import { useWelcomeInteractions } from "./useWelcomeInteractions";
import {
  type Draft,
  type Side,
  KINDS,
  SAMPLE_MEMBERS,
  sideOf,
  same,
  problems,
} from "../pages/welcome/welcomeDesign";

export function useWelcomeEditor({
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
    boost: sideOf(cards.boost),
    level_up: sideOf(cards.level_up),
  }));
  const [hist, setHist] = useState<History<Draft>>(() => history(saved));
  const [kind, setKind] = useState<CardKind>("welcome");
  const [selected, setSelected] = useState<string | null>(null);
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [fitZoom, setFitZoom] = useState(0.5);
  const [snapping, setSnapping] = useState(true);
  const [safeArea, setSafeArea] = useState(false);
  const [revisions, setRevisions] = useState<
    { id: number; created_at: string; actor_id: string; design: CardDesign | null }[]
  >([]);
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
  const refreshRevisions = useCallback(
    async (selected: CardKind) => {
      const rows = await api<typeof revisions>(`/api/guilds/${guild}/cards/${selected}/history`);
      setRevisions(rows);
    },
    [guild],
  );
  useEffect(() => {
    void refreshRevisions(kind).catch(() => setRevisions([]));
  }, [kind, refreshRevisions]);
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
  const setDesign = (next: CardDesign, merge?: string) =>
    setSide({ design: fitDesign(next), custom: true }, merge && `${kind}:${merge}`);
  const setLayer = (next: Layer, merge?: string) =>
    setDesign(
      { ...design, layers: design.layers.map((l) => (l.id === next.id ? next : l)) },
      merge && `${next.id}:${merge}`,
    );

  const addLayer = (next: Layer) => {
    if (design.layers.length >= CARD.max_layers) {
      setToast(deferredText("A design has at most {arg1} layers.", { arg1: CARD.max_layers }));
      return;
    }
    if (
      next.type === "avatar" &&
      design.layers.filter((l) => l.type === "avatar").length >= CARD.max_avatars
    ) {
      setToast(deferredText("A design has at most {arg1} avatars.", { arg1: CARD.max_avatars }));
      return;
    }
    if (designCost([...design.layers, next]) > CARD.layer_cost_max) {
      setToast(
        deferredText("That would make the image too heavy to draw quickly. Shrink or hide a layer first."),
      );
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

  const { zoomBy } = useWelcomeInteractions({
    layer,
    dialog,
    setHist,
    duplicate,
    removeLayer,
    setLayer,
    setSelected,
    viewport,
    design,
    setFitZoom,
    scale,
    setZoom,
  });

  const { save, upload, sendTest } = createWelcomeOperations({
    client,
    guild,
    draft,
    saved,
    kind,
    setSaving,
    setErrors,
    setSaved,
    setHist,
    refreshRevisions,
    setToast,
    setUploading,
    setDesign,
    design,
    testChannel,
    setTesting,
    side,
    discord,
  });

  const reset = () => {
    setHist((h) => push(h, saved));
    setErrors([]);
    setSelected(null);
  };

  const imageOff = side.delivery === "text_only";
  const ordered = [...design.layers].reverse();
  return {
    guild,
    cards,
    me,
    atlas,
    discord,
    simple,
    saved,
    setSaved,
    hist,
    setHist,
    kind,
    setKind,
    selected,
    setSelected,
    zoom,
    setZoom,
    snapping,
    setSnapping,
    safeArea,
    setSafeArea,
    revisions,
    dialog,
    setDialog,
    saving,
    errors,
    setErrors,
    toast,
    setToast,
    uploading,
    testChannel,
    setTestChannel,
    testing,
    showBot,
    setShowBot,
    clearToast,
    refreshRevisions,
    viewport,
    draft,
    side,
    design,
    layer,
    uploads,
    uploadUrls,
    fill,
    changed,
    local,
    scale,
    share,
    setSide,
    setDesign,
    setLayer,
    addLayer,
    removeLayer,
    duplicate,
    restack,
    onLive,
    onCommit,
    zoomBy,
    save,
    upload,
    sendTest,
    reset,
    imageOff,
    ordered,
  };
}

export type WelcomeModel = ReturnType<typeof useWelcomeEditor>;
