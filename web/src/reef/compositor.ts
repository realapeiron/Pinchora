import { blank, clone, flipHorizontal, overlay, type Img } from "../iso/image";
import { drawOrder } from "../iso/order";
import { MARGIN } from "../iso/projection";
import { anchor, bounds, renderRoom } from "../iso/render";
import type { Scene, SceneItem } from "../iso/scene";
import type { Point } from "./manifest";

export const isStanding = (item: SceneItem) =>
  item.layer === "floor" || item.layer === "surface" || item.layer === "figure";
export interface PreparedRoom {
  scene: Scene;
  background: Img;
  foreground: Img;
  hit: Int32Array<ArrayBuffer>;
  origin: Point;
  sprites: Img[];
}
/** One expensive pixel-accurate pass per layout/art revision, never per animation frame. */
export function prepareRoom(scene: Scene, transparentBackdrop = false): PreparedRoom {
  const sprites = scene.items.map((i) => (i.flipped && isStanding(i) ? flipHorizontal(i.sprite) : i.sprite));
  const b = bounds(scene, sprites);
  const background = renderRoom(scene, "background", b, undefined, transparentBackdrop);
  const origin: Point = [MARGIN - b[0], MARGIN - b[1]];
  const foreground = blank(background.image.width, background.image.height);
  if (scene.grid || scene.cursor || scene.items.some((i) => i.selected)) {
    // Cursor blends use the already composed scene, preserving Rust's integer blend rounding.
    const base = clone(background.image);
    for (const i of standingOrder(scene.items)) {
      const [x, y] = anchor(scene.items[i]!, sprites[i]!);
      overlay(base, sprites[i]!, x + origin[0], y + origin[1]);
    }
    const finished = renderRoom(scene, "foreground", b, clone(base)).image;
    for (let i = 0; i < base.data.length; i += 4)
      if (!base.data.subarray(i, i + 4).every((v, c) => v === finished.data[i + c]))
        foreground.data.set(finished.data.subarray(i, i + 4), i);
  }
  return { scene, background: background.image, foreground, hit: background.hit, origin, sprites };
}
/** Same graph as room.rs; fractional positions require no new sorting rule. Roster order is array order. */
export function standingOrder(items: readonly SceneItem[]): number[] {
  return drawOrder(
    items,
    items.flatMap((item, i) => (isStanding(item) ? [i] : [])),
  );
}
/** Software oracle for the cached compositor, including alpha semantics of Rust image::overlay. */
export function composeReference(room: PreparedRoom): Img {
  const out = clone(room.background);
  for (const i of standingOrder(room.scene.items)) {
    const p = anchor(room.scene.items[i]!, room.sprites[i]!);
    overlay(out, room.sprites[i]!, Math.round(p[0]) + room.origin[0], Math.round(p[1]) + room.origin[1]);
  }
  overlay(out, room.foreground, 0, 0);
  return out;
}
export function bitmap(img: Img): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = img.width;
  canvas.height = img.height;
  canvas.getContext("2d")!.putImageData(new ImageData(img.data, img.width, img.height), 0, 0);
  return canvas;
}
export interface ActorDraw {
  item: SceneItem;
  image: HTMLCanvasElement;
  offset: Point;
  seat?: { id: string; rows: number };
}
/** Draw retained native bitmaps; the caller sets only integer scale/translation. */
export class Compositor {
  readonly background: HTMLCanvasElement;
  readonly foreground: HTMLCanvasElement;
  readonly sprites: HTMLCanvasElement[];
  readonly shadow: HTMLCanvasElement;
  private lastItems: SceneItem[] = [];
  private lastOrder: number[] = [];
  private positionKey = "";
  readonly room: PreparedRoom;
  constructor(room: PreparedRoom) {
    this.room = room;
    this.background = bitmap(room.background);
    this.foreground = bitmap(room.foreground);
    this.sprites = room.sprites.map(bitmap);
    const shadow = blank(29, 9);
    for (let dy = -4; dy <= 4; dy++)
      for (let dx = -14; dx <= 14; dx++) {
        if (dx * dx * 16 + dy * dy * 196 <= 14 * 14 * 16)
          shadow.data.set([0, 0, 0, 60], ((dy + 4) * 29 + dx + 14) * 4);
      }
    this.shadow = bitmap(shadow);
  }
  draw(ctx: CanvasRenderingContext2D, actors: ActorDraw[] = [], ground?: () => void): void {
    const [ox, oy] = this.room.origin;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.background, 0, 0);
    ground?.();
    const key = actors.map((a) => `${a.item.x},${a.item.y}`).join(";");
    if (key !== this.positionKey || !this.lastItems.length) {
      this.lastItems = [...this.room.scene.items, ...actors.map((a) => a.item)];
      this.lastOrder = standingOrder(this.lastItems);
      this.positionKey = key;
    }
    // Actor shadows precede every standing object, just like room.rs.
    for (const a of actors) {
      ctx.drawImage(
        this.shadow,
        Math.round(32 * (a.item.x - a.item.y)) + ox - 14,
        Math.round(16 * (a.item.x + a.item.y) + 20) + oy - 4,
      );
    }
    for (const i of this.lastOrder) {
      const a = actors[i - this.room.scene.items.length];
      if (a) {
        const [x, y] = anchor(a.item, a.item.sprite);
        ctx.drawImage(a.image, Math.round(x + a.offset[0]) + ox, Math.round(y + a.offset[1]) + oy);
        if (a.seat) {
          const at = this.room.scene.items.findIndex((i) => i.info.placement === a.seat!.id);
          if (at >= 0) {
            const sprite = this.sprites[at]!;
            const item = this.room.scene.items[at]!;
            const [sx, sy] = anchor(item, this.room.sprites[at]!);
            const rows = Math.min(sprite.height, a.seat.rows);
            const top = sprite.height - rows;
            ctx.drawImage(
              sprite,
              0,
              top,
              sprite.width,
              rows,
              Math.round(sx) + ox,
              Math.round(sy) + oy + top,
              sprite.width,
              rows,
            );
          }
        }
      } else {
        const [x, y] = anchor(this.room.scene.items[i]!, this.room.sprites[i]!);
        ctx.drawImage(this.sprites[i]!, Math.round(x) + ox, Math.round(y) + oy);
      }
    }
    ctx.drawImage(this.foreground, 0, 0);
  }
  /** Reverse painter order with native alpha masks; shadows are not interactive objects. */
  pick(x: number, y: number): number | null {
    const [ox, oy] = this.room.origin;
    for (const i of standingOrder(this.room.scene.items).reverse()) {
      const sprite = this.room.sprites[i]!;
      const [left, top] = anchor(this.room.scene.items[i]!, sprite);
      const sx = Math.floor(x - ox - left),
        sy = Math.floor(y - oy - top);
      if (
        sx >= 0 &&
        sy >= 0 &&
        sx < sprite.width &&
        sy < sprite.height &&
        sprite.data[(sy * sprite.width + sx) * 4 + 3]
      )
        return i;
    }
    const at = Math.floor(y) * this.room.background.width + Math.floor(x);
    if (x < 0 || y < 0 || x >= this.room.background.width || y >= this.room.background.height) return null;
    const tag = this.room.hit[at] ?? 0;
    const item = this.room.scene.items[tag - 1];
    return item && (item.layer === "rug" || item.layer === "wall") ? tag - 1 : null;
  }
  dispose(): void {
    for (const c of [this.background, this.foreground, this.shadow, ...this.sprites]) {
      c.width = 0;
      c.height = 0;
    }
  }
}
export function actorItem(x = 0, y = 0): SceneItem {
  return {
    sprite: blank(96, 128),
    x,
    y,
    w: 1,
    d: 1,
    layer: "figure",
    flipped: false,
    selected: false,
    lift: 0,
    info: { kind: "character", label: "You", detail: null, placement: null },
  };
}
