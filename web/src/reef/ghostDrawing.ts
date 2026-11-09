import type { Dispatch, SetStateAction } from "react";
import { translate as spaTranslate } from "../i18n";
import { Art } from "../iso/art";
import { flipHorizontal } from "../iso/image";
import { levelTop } from "../iso/projection";
import { anchor } from "../iso/render";
import { footprint, surfaceLift } from "../iso/rules";
import type { RoomJson, SceneItem } from "../iso/scene";
import type { Ghost } from "./DecoratePanel";
import type { bootstrap } from "./bootstrap";
import { bitmap, isStanding } from "./compositor";
import type { Point } from "./manifest";

export interface GhostArt {
  id: string;
  art: Art | null;
  bitmap: HTMLCanvasElement | null;
  orientation: string;
  position: string;
  item: SceneItem | null;
}
interface GhostPreparation {
  loaded: Awaited<ReturnType<typeof bootstrap>>;
  abort: AbortController;
  room: RoomJson;
  setWarnings: Dispatch<SetStateAction<string[]>>;
}
export function prepareGhost(
  state: GhostArt,
  ghost: Ghost,
  { loaded, abort, room, setWarnings }: GhostPreparation,
) {
  if (ghost.item_id !== state.id) {
    state.id = ghost.item_id;
    state.art = null;
    if (state.bitmap) state.bitmap.width = state.bitmap.height = 0;
    state.bitmap = null;
    state.item = null;
    state.position = "";
    state.orientation = "";
    const requested = state.id;
    const base = loaded.content.item(requested)?.base.id;
    if (base)
      void loaded.loader
        .sprite(`f_${base}`)
        .then((img) => {
          if (img && !abort.signal.aborted && state.id === requested)
            state.art = new Art({ get: () => img, has: () => true }, loaded.content);
        })
        .catch(() => {
          if (!abort.signal.aborted && state.id === requested)
            setWarnings((current) => [
              ...new Set([...current, spaTranslate("Some room art could not load.")]),
            ]);
        });
  }
  const positionKey = `${state.id}:${ghost.x}:${ghost.y}:${ghost.flipped}`;
  if (state.art && positionKey !== state.position) {
    const sprite = state.art.item(state.id);
    const fp = footprint(loaded.content, state.id, ghost.x, ghost.y, ghost.flipped);
    state.item =
      sprite && fp
        ? {
            sprite,
            ...fp.rect,
            layer: fp.layer,
            flipped: ghost.flipped,
            selected: false,
            lift:
              fp.layer === "surface" ? surfaceLift(loaded.content, room.placements, fp.rect.x, fp.rect.y) : 0,
            info: { kind: "furniture", label: "", detail: null, placement: null },
          }
        : null;
    state.position = positionKey;
  }
}
export function drawGhost(state: GhostArt, ghost: Ghost, ctx: CanvasRenderingContext2D, origin: Point) {
  drawGhostSprite(state, ghost, ctx, origin);
  drawGhostOutline(state.item, ghost, ctx, origin);
}
function drawGhostSprite(state: GhostArt, ghost: Ghost, ctx: CanvasRenderingContext2D, origin: Point) {
  const item = state.item;
  if (item) {
    const key = `${state.id}:${ghost.flipped}`;
    if (key !== state.orientation) {
      if (state.bitmap) state.bitmap.width = state.bitmap.height = 0;
      state.bitmap = bitmap(item.flipped && isStanding(item) ? flipHorizontal(item.sprite) : item.sprite);
      state.orientation = key;
    }
    const pos = anchor(item, item.sprite);
    ctx.globalAlpha = 0.6;
    const [ox, oy] = origin;
    if (item.layer === "wall") {
      const sign = ghost.flipped ? -1 : 1;
      ctx.save();
      ctx.transform(
        sign,
        0.5,
        0,
        1,
        ox + sign * item.x * 32,
        oy + levelTop(sign * item.x * 32) + item.y * 32,
      );
      ctx.drawImage(state.bitmap!, 0, 0, item.w * 32, item.d * 32);
      ctx.restore();
    } else if (item.layer === "rug") {
      const sw = item.sprite.width,
        sh = item.sprite.height;
      ctx.save();
      if (ghost.flipped)
        ctx.transform(
          (-32 * item.d) / sw,
          (16 * item.d) / sw,
          (32 * item.w) / sh,
          (16 * item.w) / sh,
          ox + 32 * (item.x - item.y),
          oy + 16 * (item.x + item.y),
        );
      else
        ctx.transform(
          (32 * item.w) / sw,
          (16 * item.w) / sw,
          (-32 * item.d) / sh,
          (16 * item.d) / sh,
          ox + 32 * (item.x - item.y),
          oy + 16 * (item.x + item.y),
        );
      ctx.drawImage(state.bitmap!, 0, 0);
      ctx.restore();
    } else ctx.drawImage(state.bitmap!, Math.round(pos[0]) + ox, Math.round(pos[1]) + oy);
    ctx.globalAlpha = 1;
  }
}
function drawGhostOutline(
  item: SceneItem | null,
  ghost: Ghost,
  ctx: CanvasRenderingContext2D,
  origin: Point,
) {
  const [ox, oy] = origin;
  const x = ghost.x,
    y = ghost.y,
    w = item?.w ?? 1,
    d = item?.d ?? 1;
  const points =
    item?.layer === "wall"
      ? (() => {
          const sign = ghost.flipped ? -1 : 1,
            a = sign * 32 * x,
            b = sign * 32 * (x + w);
          return [
            [a, levelTop(a) + y * 32],
            [b, levelTop(b) + y * 32],
            [b, levelTop(b) + (y + d) * 32],
            [a, levelTop(a) + (y + d) * 32],
          ];
        })()
      : [
          [32 * (x - y), 16 * (x + y)],
          [32 * (x + w - y), 16 * (x + w + y)],
          [32 * (x + w - y - d), 16 * (x + w + y + d)],
          [32 * (x - y - d), 16 * (x + y + d)],
        ];
  ctx.strokeStyle = ghost.valid ? "#80dbad" : "#ff8e8e";
  ctx.lineWidth = 2;
  ctx.setLineDash(ghost.valid ? [] : [3, 3]);
  ctx.beginPath();
  points.forEach(([px, py], i) => (i ? ctx.lineTo(ox + px!, oy + py!) : ctx.moveTo(ox + px!, oy + py!)));
  ctx.closePath();
  ctx.stroke();
  ctx.setLineDash([]);
}
