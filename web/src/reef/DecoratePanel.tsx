import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Content, type Catalog } from "../iso/content";
import type { RoomJson } from "../iso/scene";
import { canPlace } from "../iso/rules";
import { api } from "../lib/api";
import { housePath, type HouseTarget } from "../features/house/api/house";
import { RoomQueue, sendOperation, type EditOp } from "../features/house/api/houseEdit";
import { t } from "./i18n";
import { locale } from "../i18n";
import type { Point } from "./manifest";

export interface Ghost {
  item_id: string;
  x: number;
  y: number;
  flipped: boolean;
  valid: boolean;
}
export interface EditBridge {
  ghost: Ghost | null;
  pick: (tile: Point | null, placement: string | null, hover: boolean) => void;
  key: (key: string) => boolean;
}
export function DecoratePanel({
  room,
  target,
  bridge,
  onSaved,
  onClose,
}: {
  room: RoomJson;
  target: HouseTarget;
  bridge: { current: EditBridge | null };
  onSaved: () => void;
  onClose: () => void;
}) {
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
    queryFn: () => api<{ items: { item_id: string; quantity: number }[] }>(housePath(target, "/inventory")),
  });
  const surfaces = useQuery({
    queryKey: [housePath(target), "reef-surfaces"],
    queryFn: () =>
      api<{
        wallpapers: { id: string; name: string; name_tr?: string; owned: boolean }[];
        floors: { id: string; name: string; name_tr?: string; owned: boolean }[];
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
  return (
    <aside className="reef-space-sheet reef-decorate" aria-label={t("Decorate")}>
      <div className="reef-drawer-heading">
        <h2>{t("Decorate")}</h2>
        <button type="button" onClick={onClose} disabled={pending}>
          {t("Done")}
        </button>
      </div>
      <p>{t("Choose a piece, position it, then confirm. R rotates; Enter places; Escape cancels.")}</p>
      <label>
        {t("Inventory")}
        <select
          value={hand}
          disabled={pending}
          onChange={(e) => {
            setHand(e.target.value);
            setSelected("");
            setFlipped(false);
          }}
        >
          <option value="">{t("Choose an item")}</option>
          {inventory.data?.items.map((i) => (
            <option key={i.item_id} value={i.item_id}>
              {catalog.data?.item(i.item_id)
                ? catalog.data.itemName(catalog.data.item(i.item_id)!)
                : i.item_id}{" "}
              ×{i.quantity}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Furniture and actions")}
        <select
          value={selected}
          disabled={pending}
          onChange={(e) => {
            setSelected(e.target.value);
            setHand("");
            const p = confirmed.placements.find((p) => p.id === e.target.value);
            if (p) {
              setTile([p.x, p.y]);
              setFlipped(p.flipped);
            }
          }}
        >
          <option value="">{t("Choose an item")}</option>
          {confirmed.placements.map((p) => (
            <option key={p.id} value={p.id}>
              {catalog.data?.item(p.item_id)
                ? catalog.data.itemName(catalog.data.item(p.item_id)!)
                : p.item_id}{" "}
              ({p.x}, {p.y})
            </option>
          ))}
        </select>
      </label>
      <div className="reef-space-actions">
        <label>
          {t("Position X")}
          <input
            type="number"
            min={0}
            max={room.width - 1}
            value={tile[0]}
            onChange={(e) => setTile([Number(e.target.value), tile[1]])}
          />
        </label>
        <label>
          {t("Position Y / wall level")}
          <input
            type="number"
            min={0}
            max={room.depth - 1}
            value={tile[1]}
            onChange={(e) => setTile([tile[0], Number(e.target.value)])}
          />
        </label>
      </div>
      <p role="status">{reason ? t(reason) : item ? t("Ready to place") : t("Choose an item")}</p>
      <div className="reef-space-actions">
        <button
          type="button"
          disabled={!item || pending}
          aria-pressed={flipped}
          onClick={() => setFlipped(!flipped)}
        >
          {t("Rotate")}
        </button>
        <button type="button" disabled={!item || !!reason || pending || !changed} onClick={place}>
          {t("Confirm placement")}
        </button>
        <button
          type="button"
          disabled={!piece || pending}
          onClick={() => {
            send({ type: "pickup", id: selected });
            setSelected("");
          }}
        >
          {t("Store")}
        </button>
        <button
          type="button"
          disabled={!confirmed.undo?.length || pending}
          onClick={() => send({ type: "undo" })}
        >
          {t("Undo")}
        </button>
      </div>
      {(target.kind === "own" || target.kind === "plaza") && (
        <>
          <label>
            {t("Surfaces")}
            <select value={surface} onChange={(e) => setSurface(e.target.value)}>
              <option value="">{t("Choose a surface")}</option>
              {(["wallpapers", "floors"] as const).flatMap(
                (kind) =>
                  surfaces.data?.[kind]
                    .filter((s) => s.owned)
                    .map((s) => (
                      <option key={`${kind}:${s.id}`} value={`${kind}:${s.id}`}>
                        {locale() === "tr" ? (s.name_tr ?? s.name) : s.name}
                      </option>
                    )) ?? [],
              )}
            </select>
          </label>
          <button
            type="button"
            disabled={
              !surface ||
              pending ||
              surface === `wallpapers:${room.wallpaper}` ||
              surface === `floors:${room.floor}`
            }
            onClick={() => {
              const [k, id] = surface.split(":");
              send(k === "wallpapers" ? { type: "surface", wallpaper: id } : { type: "surface", floor: id });
            }}
          >
            {t("Apply surface")}
          </button>
        </>
      )}
      <details>
        <summary>{t("Clear room")}</summary>
        <p>{t("Store every piece in this room?")}</p>
        <button
          type="button"
          disabled={pending || !confirmed.placements.length}
          onClick={() => send({ type: "clear" })}
        >
          {t("Confirm clear")}
        </button>
      </details>
      <p role="status">{t(message)}</p>
      {inventory.isError && <p role="alert">{t("This request could not be completed.")}</p>}
    </aside>
  );
}
