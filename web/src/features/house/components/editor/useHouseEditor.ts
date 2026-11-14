import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Art } from "../../../../iso/art";
import { screenToTile, levelTop } from "../../../../iso/projection";
import { canPlace } from "../../../../iso/rules";
import type { RoomJson } from "../../../../iso/scene";
import { api } from "../../../../lib/api";
import { housePath, type HouseOverview, type HouseTarget } from "../../api/house";
import { RoomCursor, RoomQueue, sendOperation, type EditOp, type Piece } from "../../api/houseEdit";
import { locale } from "../../../../i18n";
import { useEditorScene } from "./useEditorScene";
import type { Stock, Surface } from "./editorResponses";

export function useHouseEditor({
  target,
  initial,
  art,
  house,
  onExit,
}: {
  target: HouseTarget;
  initial: RoomJson;
  art: Art;
  house: HouseOverview;
  onExit: () => void;
}) {
  const queries = useQueryClient();
  const [room, setRoom] = useState(initial);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  const [hand, setHand] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [flipped, setFlipped] = useState(false);
  const cursorStore = useMemo(() => new RoomCursor(), []);
  useEffect(() => () => cursorStore.dispose(), [cursorStore]);
  const [wall, setWall] = useState<"floor" | "right" | "left">("floor");
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [rarity, setRarity] = useState("");
  const [moving, setMoving] = useState<Piece | null>(null);
  const gesture = useRef<{ item: Piece | null; start: [number, number] } | null>(null);
  const [redo, setRedo] = useState<{ op: EditOp; piece?: Piece }[]>([]);
  const applied = useRef<{ op: EditOp; piece?: Piece }[]>([]);

  const [confirm, setConfirm] = useState<{ label: string; op: EditOp } | null>(null);
  const [notesOpen, setNotesOpen] = useState(false);
  const base = housePath(target);
  const inventory = useQuery({
    queryKey: [base, "inventory"],
    queryFn: () => api<Stock>(housePath(target, "/inventory")),
  });
  const surfaces = useQuery({
    queryKey: [base, "surfaces"],
    queryFn: () => api<{ wallpapers: Surface[]; floors: Surface[] }>(housePath(target, "/surfaces")),
    enabled: target.kind === "own",
  });
  const notes = useQuery({
    queryKey: [base, "guestbook"],
    queryFn: () =>
      api<{ notes: { id: string; author_name: string; body: string }[] }>(housePath(target, "/guestbook")),
    enabled: notesOpen && target.kind === "own",
  });
  const queue = useRef<RoomQueue | null>(null);
  useEffect(() => {
    const q = new RoomQueue(
      initial,
      (expected, op, revision, request) =>
        sendOperation(target, initial.idx, expected, op, revision, request),
      () => api<RoomJson>(housePath(target, `/rooms/${initial.idx}`)),
      (next, busy, error, ids) => {
        setSelected((id) => {
          const mapped = id ? (ids?.[id] ?? id) : null;
          return next.placements.some((p) => p.id === mapped) ? mapped : null;
        });
        if (ids) {
          setMoving((piece) => (piece && ids[piece.id] ? { ...piece, id: ids[piece.id]! } : piece));
        }
        setRoom(next);
        setPending(busy);
        if (error) {
          setMessage(error);
          setHand(null);
          setMoving(null);
          gesture.current = null;
          applied.current = [];
          setRedo([]);
        } else setMessage(busy ? "Saving…" : "Saved");
        if (!busy) {
          void queries.invalidateQueries({ queryKey: [base] });
          void queries.invalidateQueries({ queryKey: [housePath(target, `/rooms/${initial.idx}`)] });
        }
      },
    );
    queue.current = q;
    return () => {
      q.dispose();
      queue.current = null;
    };
    // The parent keys this editor by scope and room; each room owns its queue.
  }, []);
  useEffect(() => {
    const q = queue.current;
    if (q && !q.pending && initial.fingerprint !== q.room.fingerprint) {
      applied.current = [];
      setRedo([]);
      setSelected(null);
      setHand(null);
      setMoving(null);
      gesture.current = null;
      q.room = initial;
      setRoom(initial);
      setMessage("This room changed somewhere else (Discord?), reloaded.");
    } else if (q && !q.pending) {
      q.room = initial;
      setRoom(initial);
    }
  }, [initial]);
  const choose = (id: string) => {
    setHand(id);
    setSelected(null);
    setMoving(null);
    setFlipped(false);
  };
  const send = (op: EditOp, replay = false) => {
    setMessage("");
    if (["place", "move", "flip", "pickup", "clear"].includes(op.type)) {
      applied.current.push({
        op,
        piece: "id" in op ? room.placements.find((p) => p.id === op.id) : undefined,
      });
      applied.current = applied.current.slice(-10);
      if (!replay) setRedo([]);
    }
    void queries.cancelQueries({ queryKey: [housePath(target, `/rooms/${initial.idx}`)] });
    queue.current?.enqueue(op);
  };
  const undo = () => {
    if (pending || !room.undo?.length) return;
    const last = applied.current.pop();
    if (last) setRedo((stack) => [...stack, last]);
    send({ type: "undo" });
  };
  const reapply = () => {
    if (pending || !redo.length) return;
    const entry = redo[redo.length - 1]!;
    let op = entry.op;
    if ("id" in op && entry.piece) {
      const p = room.placements.find(
        (p) =>
          p.item_id === entry.piece!.item_id &&
          p.x === entry.piece!.x &&
          p.y === entry.piece!.y &&
          p.flipped === entry.piece!.flipped,
      );
      if (!p) {
        setRedo([]);
        return;
      }
      op = { ...op, id: p.id };
    }
    setRedo((stack) => stack.slice(0, -1));
    send(op, true);
  };
  const { scene, drawn, ox, oy, held, others, activeWall } = useEditorScene(
    art,
    room,
    selected,
    moving,
    hand,
    flipped,
    wall,
  );
  const point = (px: number, py: number) => {
    const x = px - ox,
      y = py - oy;
    const tile: [number, number] | null =
      activeWall === "floor"
        ? screenToTile(x, y, room.width, room.depth)
        : [Math.floor((activeWall === "right" ? x : -1 - x) / 32), Math.floor((y - levelTop(x)) / 32)];
    const next: [number, number] = tile ?? [-1, -1];
    cursorStore.move(next[0], next[1]);
  };

  const place = () => {
    if (!held) return;
    const [x, y] = cursorStore.get();
    const why = canPlace(art.content, room.width, room.depth, others, held, x, y, flipped);
    if (why) {
      setMessage(why);
      return;
    }
    send(
      moving
        ? { type: "move", id: moving.id, x, y, flipped }
        : { type: "place", item_id: held, x, y, flipped },
    );
    setMoving(null);
    setHand(null);
  };
  const turn = () => {
    if (held) setFlipped(!flipped);
    else if (selected && !pending) send({ type: "flip", id: selected });
  };
  const pickup = () => {
    if (selected && !pending) {
      send({ type: "pickup", id: selected });
      setSelected(null);
    }
  };
  const surfaceName = (s: Surface) => (locale() === "tr" ? (s.name_tr ?? s.name) : s.name);
  const categoryName = (key: string) => {
    const f = art.content.catalog.furniture.find((f) => f.category === key);
    return locale() === "tr" ? (f?.category_name_tr ?? key) : (f?.category_name ?? key);
  };
  const rarityName = (key: string) =>
    locale() === "tr"
      ? (art.content.catalog.furniture.find((f) => f.rarity === key)?.rarity_name_tr ?? key)
      : key;
  const categories = [...new Set(art.content.catalog.furniture.map((f) => f.category))].sort();
  const rarities = [...new Set(art.content.catalog.furniture.map((f) => f.rarity))];
  const stock =
    inventory.data?.items.filter((p) => {
      const item = art.content.item(p.item_id);
      return (
        item &&
        (!category || item.base.category === category) &&
        (!rarity || item.base.rarity === rarity) &&
        art.content.itemName(item).toLocaleLowerCase().includes(search.toLocaleLowerCase())
      );
    }) ?? [];
  return {
    target,
    art,
    house,
    onExit,
    room,
    pending,
    message,
    hand,
    setHand,
    selected,
    setSelected,
    flipped,
    setFlipped,
    cursorStore,
    wall,
    setWall,
    search,
    setSearch,
    category,
    setCategory,
    rarity,
    setRarity,
    setMoving,
    gesture,
    redo,
    confirm,
    setConfirm,
    notesOpen,
    setNotesOpen,
    inventory,
    surfaces,
    notes,
    choose,
    send,
    undo,
    reapply,
    scene,
    drawn,
    ox,
    oy,
    held,
    others,
    activeWall,
    point,
    place,
    turn,
    pickup,
    surfaceName,
    categoryName,
    rarityName,
    categories,
    rarities,
    stock,
  };
}

export type HouseEditorModel = ReturnType<typeof useHouseEditor>;
