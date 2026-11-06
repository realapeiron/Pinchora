// The pure placement geometry of `pinchora-game/src/house.rs` the renderer needs.

import type { Content, Layer } from "./content";

export interface Rect {
  x: number;
  y: number;
  w: number;
  d: number;
}

/** An item already in the room. */
export interface Placed {
  item_id: string;
  x: number;
  y: number;
  flipped: boolean;
}

export function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.d && b.y < a.y + a.d;
}

export function contains(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.w <= outer.x + outer.w &&
    inner.y + inner.d <= outer.y + outer.d
  );
}

/**
 * Footprint of an item at (x, y) as placed: floor layers swap `[w, d]` when flipped; wall items keep
 * `[w, h]` (flipping moves them to the other wall).
 */
export function footprint(
  content: Content,
  itemId: string,
  x: number,
  y: number,
  flipped: boolean,
): { layer: Layer; rect: Rect } | null {
  const item = content.item(itemId);
  if (!item) return null;
  const [w, d] = item.base.footprint;
  const swap = flipped && item.base.layer !== "wall";
  return { layer: item.base.layer, rect: { x, y, w: swap ? d : w, d: swap ? w : d } };
}

/** How high a surface item at (x, y) stands: the top of the table or shelf under it. */
export function surfaceLift(content: Content, placed: Placed[], x: number, y: number): number {
  const spot: Rect = { x, y, w: 1, d: 1 };
  let lift = 0;
  for (const p of placed) {
    const item = content.item(p.item_id);
    const fp = footprint(content, p.item_id, p.x, p.y, p.flipped);
    if (!item || !fp) continue;
    if (fp.layer === "floor" && content.hasTag(item.base, "surface") && contains(fp.rect, spot)) {
      lift = Math.max(lift, item.base.surface_height ?? 24);
    }
  }
  return lift;
}
