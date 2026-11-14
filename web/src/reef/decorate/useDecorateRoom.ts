import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Content, type Catalog } from "../../iso/content";
import type { RoomJson } from "../../iso/scene";
import { canPlace } from "../../iso/rules";
import { api } from "../../lib/api";
import { housePath, type HouseTarget } from "../../features/house/api/house";
import { RoomQueue, sendOperation, type EditOp } from "../../features/house/api/houseEdit";
import type { EditBridge } from "../DecoratePanel";
import type { Point } from "../manifest";

export type InventoryRow = { item_id: string; quantity: number };
export type Surface = { id: string; name: string; name_tr?: string; owned: boolean };

export function useDecorateRoom(
  room: RoomJson,
  target: HouseTarget,
  bridge: { current: EditBridge | null },
  onSaved: () => void,
) {
  const [confirmed, setConfirmed] = useState(room);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  const [hand, setHand] = useState("");
  const [selected, setSelected] = useState("");
  const [tile, setTile] = useState<Point>([0, 0]);
  const [flipped, setFlipped] = useState(false);
  const [surface, setSurface] = useState("");
  const queue = useRef<RoomQueue | null>(null);
  const catalog = useQuery({
    queryKey: ["reef-edit-catalog"],
    queryFn: async () => new Content(await api<Catalog>("/api/house/content")),
    staleTime: Infinity,
  });
  const inventory = useQuery({
    queryKey: [housePath(target), "reef-inventory"],
    queryFn: () => api<{ items: InventoryRow[] }>(housePath(target, "/inventory")),
  });
  const surfaces = useQuery({
    queryKey: [housePath(target), "reef-surfaces"],
    queryFn: () =>
      api<{
        wallpapers: Surface[];
        floors: Surface[];
      }>(housePath(target, "/surfaces")),
    enabled: target.kind === "own" || target.kind === "plaza",
  });
  const saved = useRef(onSaved);
  saved.current = onSaved;
  useEffect(() => {
    const q = new RoomQueue(
      room,
      (expected, op, revision, request) => sendOperation(target, room.idx, expected, op, revision, request),
      () => api<RoomJson>(housePath(target, `/rooms/${room.idx}`)),
      (next, busy, error) => {
        setConfirmed(next);
        setPending(busy);
        setMessage(error ?? (busy ? "Saving…" : "Saved"));
        if (!busy) {
          void inventory.refetch();
          saved.current();
        }
      },
    );
    queue.current = q;
    return () => {
      q.dispose();
      bridge.current = null;
    };
    // One serialized queue for this mounted room, shared with the existing house editor.
  }, []);
  useEffect(() => {
    if (!queue.current?.pending) {
      queue.current!.room = room;
      setConfirmed(room);
    }
  }, [room]);
  const piece = confirmed.placements.find((p) => p.id === selected);
  const item = hand || piece?.item_id || "";
  const reason =
    item && catalog.data
      ? canPlace(
          catalog.data,
          room.width,
          room.depth,
          confirmed.placements.filter((p) => p.id !== selected),
          item,
          ...tile,
          flipped,
        )
      : null;
  const changed = !piece || piece.x !== tile[0] || piece.y !== tile[1] || piece.flipped !== flipped;
  const send = (op: EditOp) => {
    if (!pending) queue.current?.enqueue(op);
  };
  const place = () => {
    if (!item || reason || pending || !changed) return;
    send(
      piece
        ? { type: "move", id: piece.id, x: tile[0], y: tile[1], flipped }
        : { type: "place", item_id: item, x: tile[0], y: tile[1], flipped },
    );
    setHand("");
    setSelected("");
  };
  bridge.current = {
    ghost: item ? { item_id: item, x: tile[0], y: tile[1], flipped, valid: !reason } : null,
    pick: (p, id, hover) => {
      if (hover) {
        if (item && p) setTile((old) => (old[0] === p[0] && old[1] === p[1] ? old : p));
        return;
      }
      if (item && p) {
        setTile(p);
        return;
      }
      if (id) {
        const piece = confirmed.placements.find((p) => p.id === id);
        if (piece) {
          setSelected(id);
          setTile([piece.x, piece.y]);
          setFlipped(piece.flipped);
        }
      }
    },
    key: (key) => {
      const steps: Record<string, Point> = {
        ArrowLeft: [-1, 0],
        ArrowRight: [1, 0],
        ArrowUp: [0, -1],
        ArrowDown: [0, 1],
      };
      if (steps[key]) {
        const d = steps[key]!;
        setTile(([x, y]) => [Math.max(0, x + d[0]), Math.max(0, y + d[1])]);
        return true;
      }
      if (key === "Enter") {
        place();
        return true;
      }
      if (key.toLowerCase() === "r") {
        setFlipped((f) => !f);
        return true;
      }
      if (key === "Escape") {
        setHand("");
        setSelected("");
        return true;
      }
      return false;
    },
  };
  return {
    confirmed,
    pending,
    message,
    hand,
    setHand,
    selected,
    setSelected,
    tile,
    setTile,
    flipped,
    setFlipped,
    surface,
    setSurface,
    catalog,
    inventory,
    surfaces,
    piece,
    item,
    reason,
    changed,
    send,
    place,
  };
}
