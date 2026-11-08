import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Art } from "../iso/art";
import { anchor, bounds, renderRoom } from "../iso/render";
import { buildScene, type RoomJson, type SceneItem } from "../iso/scene";
import { canPlace, contains, footprint, surfaceLift } from "../iso/rules";
import { MARGIN, screenToTile, levelTop } from "../iso/projection";
import { api } from "../lib/api";
import { housePath, type HouseOverview, type HouseTarget } from "../lib/house";
import { RoomQueue, sendOperation, type EditOp, type Piece } from "../lib/houseEdit";
import { translate as t, locale } from "../i18n";
import { PixelButton } from "./PixelButton";
import { RoomStage } from "./RoomStage";

type Stock = { items: { item_id: string; quantity: number }[]; placed: { item_id: string; room: number }[] };
type Surface = {
  name_tr?: string;
  id: string;
  name: string;
  price: number;
  currency: string;
  owned: boolean;
};

export function HouseEditor({
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
  const [cursor, setCursor] = useState<[number, number]>([0, 0]);
  const cursorRef = useRef(cursor);
  const [wall, setWall] = useState<"floor" | "right" | "left">("floor");
  const [hover, setHover] = useState<number | null>(null);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [rarity, setRarity] = useState("");
  const [moving, setMoving] = useState<Piece | null>(null);
  const gesture = useRef<{ item: Piece | null; start: [number, number] } | null>(null);
  const [redo, setRedo] = useState<{ op: EditOp; piece?: Piece }[]>([]);
  const applied = useRef<{ op: EditOp; piece?: Piece }[]>([]);
  const overlay = useRef<HTMLCanvasElement>(null);
  const frame = useRef(0);
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
      (expected, op) => sendOperation(target, initial.idx, expected, op),
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
  const completeScene = useMemo(
    () => buildScene(art.content, art, room, { edit: true, selected }),
    [art, room, selected],
  );
  const liftedIds = useMemo(() => {
    const ids = new Set<string>();
    if (!moving) return ids;
    ids.add(moving.id);
    const base = footprint(art.content, moving.item_id, moving.x, moving.y, moving.flipped);
    if (base?.layer === "floor")
      for (const p of room.placements) {
        const top = footprint(art.content, p.item_id, p.x, p.y, p.flipped);
        if (top?.layer === "surface" && contains(base.rect, top.rect)) ids.add(p.id);
      }
    return ids;
  }, [art, moving, room.placements]);
  const scene = useMemo(
    () => ({
      ...completeScene,
      items: completeScene.items.filter((i) => !i.info.placement || !liftedIds.has(i.info.placement)),
    }),
    [completeScene, liftedIds],
  );
  const box = useMemo(
    () =>
      bounds(
        completeScene,
        completeScene.items.map((i) => i.sprite),
      ),
    [completeScene],
  );
  const drawn = useMemo(() => renderRoom(scene, "all", box), [scene, box]);
  const ox = MARGIN - box[0],
    oy = MARGIN - box[1];
  const held = moving?.item_id ?? hand;
  const fp = held ? footprint(art.content, held, cursor[0], cursor[1], flipped) : null;
  const others = room.placements.filter((p) => !liftedIds.has(p.id));
  const reason = held
    ? canPlace(art.content, room.width, room.depth, others, held, cursor[0], cursor[1], flipped)
    : null;
  const activeWall = fp?.layer === "wall" ? (flipped ? "left" : "right") : held ? "floor" : wall;
  const point = (px: number, py: number) => {
    const x = px - ox,
      y = py - oy;
    const tile: [number, number] | null =
      activeWall === "floor"
        ? screenToTile(x, y, room.width, room.depth)
        : [Math.floor((activeWall === "right" ? x : -1 - x) / 32), Math.floor((y - levelTop(x)) / 32)];
    const next: [number, number] = tile ?? [-1, -1];
    cursorRef.current = next;
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => setCursor(next));
  };
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const ghostSprite = useMemo(() => {
    const sprite = held ? art.item(held) : null;
    if (!sprite) return null;
    const canvas = document.createElement("canvas");
    canvas.width = sprite.width;
    canvas.height = sprite.height;
    canvas.getContext("2d")?.putImageData(new ImageData(sprite.data, sprite.width, sprite.height), 0, 0);
    return { sprite, canvas };
  }, [art, held]);
  useEffect(() => {
    const ctx = overlay.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, drawn.image.width, drawn.image.height);
    const [x, y] = cursor;
    const w = fp?.rect.w ?? 1,
      d = fp?.rect.d ?? 1;
    const points =
      activeWall === "floor"
        ? [
            [32 * (x - y), 16 * (x + y)],
            [32 * (x + w - y), 16 * (x + w + y)],
            [32 * (x + w - y - d), 16 * (x + w + y + d)],
            [32 * (x - y - d), 16 * (x + y + d)],
          ]
        : (() => {
            const left = activeWall === "left";
            const a = (left ? -1 : 1) * 32 * x,
              b = (left ? -1 : 1) * 32 * (x + w);
            return [
              [a, levelTop(a) + 32 * y],
              [b, levelTop(b) + 32 * y],
              [b, levelTop(b) + 32 * (y + d)],
              [a, levelTop(a) + 32 * (y + d)],
            ];
          })();
    ctx.beginPath();
    points.forEach(([px, py], i) => (i ? ctx.lineTo(px! + ox, py! + oy) : ctx.moveTo(px! + ox, py! + oy)));
    ctx.closePath();
    ctx.fillStyle = reason ? "#ff4d6d55" : "#06d6a055";
    ctx.fill();
    ctx.strokeStyle = reason ? "#ff4d6d" : "#06d6a0";
    ctx.lineWidth = 3;
    ctx.stroke();
    if (ghostSprite && fp) {
      const { sprite, canvas: tmp } = ghostSprite;
      const item = { x, y, w, d, layer: fp.layer, lift: surfaceLift(art.content, others, x, y) } as SceneItem;
      const [sx, sy] = anchor(item, sprite);
      ctx.globalAlpha = 0.55;
      ctx.imageSmoothingEnabled = false;
      if (fp.layer === "wall") {
        ctx.save();
        const sign = flipped ? -1 : 1;
        ctx.transform(sign, 0.5, 0, 1, ox + sign * x * 32, oy + levelTop(sign * x * 32) + y * 32);
        ctx.drawImage(tmp, 0, 0, w * 32, d * 32);
        ctx.restore();
      } else {
        ctx.save();
        if (flipped) {
          ctx.translate(sx + ox + sprite.width, sy + oy);
          ctx.scale(-1, 1);
          ctx.drawImage(tmp, 0, 0);
        } else ctx.drawImage(tmp, sx + ox, sy + oy);
        ctx.restore();
      }
      ctx.globalAlpha = 1;
    }
  }, [cursor, flipped, held, fp, reason, activeWall, art, drawn, ox, oy, others, ghostSprite]);
  const place = () => {
    if (!held) return;
    const [x, y] = cursorRef.current;
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
  return (
    <section
      className="house-editor"
      aria-label={t("House editor")}
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).matches("input, select, textarea")) return;
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
          e.preventDefault();
          if (e.shiftKey) reapply();
          else undo();
        } else if (e.key.toLowerCase() === "r") {
          e.preventDefault();
          turn();
        } else if (e.key === "Delete" || e.key === "Backspace") {
          e.preventDefault();
          pickup();
        } else if (e.key === "Escape") {
          setHand(null);
          setMoving(null);
          setSelected(null);
          setConfirm(null);
        } else if (e.key.toLowerCase() === "w")
          setWall(wall === "floor" ? "right" : wall === "right" ? "left" : "floor");
        else if (e.key.startsWith("Arrow")) {
          e.preventDefault();
          const [x, y] = cursorRef.current;
          const next: [number, number] = [
            Math.max(0, x + (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0)),
            Math.max(0, y + (e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0)),
          ];
          cursorRef.current = next;
          setCursor(next);
        }
      }}
    >
      <aside className="house-inventory">
        <h2>{t("Furniture")}</h2>
        <label>
          {t("Search inventory")}
          <input value={search} onChange={(e) => setSearch(e.target.value)} />
        </label>
        <label>
          {t("Category")}
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            <option value="">{t("All categories")}</option>
            {categories.map((c) => (
              <option key={c} value={c}>
                {categoryName(c)}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Rarity")}
          <select value={rarity} onChange={(e) => setRarity(e.target.value)}>
            <option value="">{t("All rarities")}</option>
            {rarities.map((r) => (
              <option key={r} value={r}>
                {rarityName(r)}
              </option>
            ))}
          </select>
        </label>
        {inventory.isError && <p role="alert">{t("Could not load inventory. Try again.")}</p>}
        <div className="house-stock">
          {stock.map((p) => {
            const item = art.content.item(p.item_id)!;
            return (
              <button
                type="button"
                key={p.item_id}
                disabled={p.quantity < 1}
                aria-pressed={hand === p.item_id}
                onClick={() => choose(p.item_id)}
                onPointerDown={() => choose(p.item_id)}
              >
                <ItemThumb art={art} id={p.item_id} />
                <span>
                  {art.content.itemName(item)}
                  <small>
                    ×{p.quantity}
                    {[
                      ...new Set(
                        inventory.data?.placed.filter((i) => i.item_id === p.item_id).map((i) => i.room),
                      ),
                    ]
                      .map((room) => ` · ${t("Room {n}", { n: room + 1 })}`)
                      .join("")}
                  </small>
                </span>
              </button>
            );
          })}
          {!stock.length && <p>{t("No matching furniture.")}</p>}
        </div>
      </aside>
      <div className="house-workbench">
        <div className="house-options" role="toolbar" aria-label={t("Editor tools")}>
          <PixelButton onClick={place} disabled={!held || !!reason}>
            {t("Place")}
          </PixelButton>
          <PixelButton onClick={turn} disabled={!held && (!selected || pending)}>
            {t("Turn / flip")}
          </PixelButton>
          <PixelButton onClick={pickup} disabled={!selected || pending}>
            {t("Store")}
          </PixelButton>
          <PixelButton onClick={undo} disabled={pending || !room.undo?.length}>
            {t("Undo")}
          </PixelButton>
          <PixelButton
            onClick={reapply}
            disabled={pending || !redo.length}
            title={t("Redo re-applies the operation.")}
          >
            {t("Redo")}
          </PixelButton>
          <PixelButton
            variant="secondary"
            disabled={pending || !room.placements.length}
            onClick={() => setConfirm({ label: t("Store every piece in this room?"), op: { type: "clear" } })}
          >
            {t("Clear room")}
          </PixelButton>
          <PixelButton variant="ghost" disabled={pending} onClick={onExit}>
            {t("View mode")}
          </PixelButton>
        </div>
        <RoomStage
          rendered={drawn}
          hover={hover}
          onHover={setHover}
          label={t("Editable room")}
          editor={{
            overlay: (
              <canvas
                ref={overlay}
                className="room-canvas room-ghost"
                width={drawn.image.width}
                height={drawn.image.height}
                aria-hidden="true"
              />
            ),
            point,
            down: (index) => {
              const id = index === null ? null : scene.items[index]?.info.placement;
              const item = room.placements.find((p) => p.id === id) ?? null;
              gesture.current = { item, start: cursorRef.current };
              if (!hand && item) {
                setSelected(item.id);
                setFlipped(item.flipped);
                setMoving(item);
              }
            },
            up: () => {
              const g = gesture.current;
              gesture.current = null;
              if (held) {
                if (
                  g?.item &&
                  !hand &&
                  g.start[0] === cursorRef.current[0] &&
                  g.start[1] === cursorRef.current[1]
                ) {
                  setMoving(null);
                  return;
                }
                place();
                return;
              }
              if (g?.item) {
                setSelected(g.item.id);
                if (g.start[0] !== cursorRef.current[0] || g.start[1] !== cursorRef.current[1]) {
                  send({
                    type: "move",
                    id: g.item.id,
                    x: cursorRef.current[0],
                    y: cursorRef.current[1],
                    flipped: g.item.flipped,
                  });
                  setMoving(null);
                } else setMoving(g.item);
              }
            },
            cancel: () => {
              gesture.current = null;
              setMoving(null);
            },
          }}
        />
        <div className="house-info" role="status" aria-live="polite">
          {t((reason ?? message) || "Choose furniture, then click a tile. Alt + drag pans the room.")} ·{" "}
          {cursor[0] + 1},{cursor[1] + 1}
        </div>
        <div className="house-options">
          <label>
            {t("Cursor plane")}
            <select
              value={activeWall}
              disabled={!!held}
              onChange={(e) => setWall(e.target.value as typeof wall)}
            >
              <option value="floor">{t("Floor")}</option>
              <option value="right">{t("Right wall")}</option>
              <option value="left">{t("Left wall")}</option>
            </select>
          </label>
          {(
            [
              [-1, 0, "Move cursor left"],
              [1, 0, "Move cursor right"],
              [0, -1, "Move cursor up"],
              [0, 1, "Move cursor down"],
            ] as const
          ).map(([dx, dy, label]) => (
            <PixelButton
              key={label}
              variant="secondary"
              onClick={() => {
                const p: [number, number] = [Math.max(0, cursor[0] + dx), Math.max(0, cursor[1] + dy)];
                cursorRef.current = p;
                setCursor(p);
              }}
            >
              {t(label)}
            </PixelButton>
          ))}
        </div>
        <label>
          {t("Select a placed piece")}
          <select
            value={selected ?? ""}
            onChange={(e) => {
              setSelected(e.target.value || null);
              setHand(null);
              setMoving(null);
            }}
          >
            <option value="">{t("Choose a piece")}</option>
            {room.placements.map((p) => (
              <option key={p.id} value={p.id}>
                {art.content.itemName(art.content.item(p.item_id)!)} · {p.x + 1},{p.y + 1}
              </option>
            ))}
          </select>
        </label>
        {selected && (
          <PixelButton
            disabled={pending}
            variant="secondary"
            onClick={() => {
              const p = room.placements.find((p) => p.id === selected);
              if (p) {
                setMoving(p);
                setFlipped(p.flipped);
              }
            }}
          >
            {t("Move selected piece")}
          </PixelButton>
        )}
        {target.kind === "own" && (
          <details className="house-surfaces">
            <summary>{t("Walls, floors & house")}</summary>
            {(
              [
                ["wallpapers", "wallpaper", "Wallpaper"],
                ["floors", "floor", "Floor"],
              ] as const
            ).map(([key, field, label]) => (
              <label key={key}>
                {t(label)}
                <select
                  value={room[field]}
                  disabled={pending}
                  onChange={(e) => {
                    const s = surfaces.data?.[key].find((s) => s.id === e.target.value);
                    if (!s) return;
                    const op: EditOp = { type: "surface", [field]: s.id, confirm_price: s.price };
                    if (s.owned) send(op);
                    else
                      setConfirm({
                        label: t("Buy {name} for {price} {currency}?", {
                          name: surfaceName(s),
                          price: s.price,
                          currency: t(s.currency.toLowerCase()),
                        }),
                        op,
                      });
                  }}
                >
                  {surfaces.data?.[key].map((s) => (
                    <option key={s.id} value={s.id}>
                      {surfaceName(s)} · {s.owned ? t("Owned") : `${s.price} ${t(s.currency.toLowerCase())}`}
                    </option>
                  ))}
                </select>
              </label>
            ))}
            <PixelButton
              disabled={pending}
              variant="secondary"
              onClick={() => send({ type: "lock", locked: !house.house.locked })}
            >
              {house.house.locked ? t("Unlock house") : t("Lock house")}
            </PixelButton>
            {house.next && (
              <PixelButton
                disabled={pending}
                onClick={() =>
                  setConfirm({
                    label: t(
                      "Upgrade to {name}: {width}×{depth}, {rooms} rooms for {price} shells?",
                      house.next!,
                    ),
                    op: { type: "upgrade", from_tier: house.house.tier },
                  })
                }
              >
                {t("Upgrade house")}
              </PixelButton>
            )}
            <PixelButton variant="secondary" onClick={() => setNotesOpen(!notesOpen)}>
              {t("Guestbook")}
            </PixelButton>
            {notesOpen && (
              <div>
                {notes.data?.notes.map((n) => (
                  <blockquote key={n.id}>
                    <strong>{n.author_name}</strong>
                    <p>{n.body}</p>
                  </blockquote>
                ))}
                {notes.data?.notes.length === 0 && <p>{t("Nothing yet.")}</p>}
              </div>
            )}
          </details>
        )}
        {confirm && (
          <div className="house-confirm" role="alertdialog" aria-modal="false" aria-label={confirm.label}>
            <p>{confirm.label}</p>
            <PixelButton
              autoFocus
              disabled={pending}
              onClick={() => {
                send(confirm.op);
                setConfirm(null);
              }}
            >
              {t("Confirm")}
            </PixelButton>
            <PixelButton variant="secondary" onClick={() => setConfirm(null)}>
              {t("Cancel")}
            </PixelButton>
          </div>
        )}
      </div>
    </section>
  );
}
function ItemThumb({ art, id }: { art: Art; id: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const sprite = art.item(id);
  useEffect(() => {
    if (sprite)
      ref.current
        ?.getContext("2d")
        ?.putImageData(new ImageData(sprite.data, sprite.width, sprite.height), 0, 0);
  }, [sprite]);
  return sprite ? <canvas ref={ref} width={sprite.width} height={sprite.height} aria-hidden="true" /> : null;
}
