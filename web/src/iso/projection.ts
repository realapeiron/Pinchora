// The isometric projection of `pinchora-render/src/room.rs`.
//
// Floor tile (x, y) has its top corner at screen (32·(x−y), 16·(x+y)): x runs down-right, y runs
// down-left. The right wall stands on the y = 0 edge, the left wall on the x = 0 edge.

/** Floor tile size in native pixels (a 2:1 diamond). */
export const TILE_W = 64;
export const TILE_H = 32;
/** Wall items snap to this many rows on a wall, `LEVEL_H` pixels each. */
export const WALL_LEVELS = 3;
export const LEVEL_H = 32;
export const WALL_PAD = 8;
export const WALL_H = WALL_LEVELS * LEVEL_H + WALL_PAD;
export const SLAB = 8;
export const MARGIN = 12;

export type WallSide = "right" | "left";

/** Floor-plane coordinates of a screen point, 64 units per tile along each axis. */
export function plane(lx: number, ly: number): [number, number] {
  return [lx + 2 * ly, 2 * ly - lx - 1];
}

/** The first floor row of a screen column; the back walls end right above it. */
export function floorTop(lx: number): number {
  return lx >= 0 ? Math.floor((lx + 2) / 2) : Math.floor((1 - lx) / 2);
}

/** Screen column of the `m`-th pixel along a wall, counted from the corner. */
export function wallColumn(side: WallSide, m: number): number {
  return side === "right" ? m : -1 - m;
}

/** Top row of wall level 0 in a screen column. */
export function levelTop(lx: number): number {
  return floorTop(lx) - WALL_H + WALL_PAD;
}

/** Screen position of the top corner of tile (x, y), before the scene's own offset. */
export function tileTop(x: number, y: number): [number, number] {
  return [32 * (x - y), 16 * (x + y)];
}

/**
 * The floor tile under a screen point (scene coordinates, the room's own pixels), or `null` when
 * the point is off the floor.
 */
export function screenToTile(lx: number, ly: number, width: number, depth: number): [number, number] | null {
  const [a, b] = plane(lx, ly);
  if (a < 0 || b < 0 || a >= 64 * width || b >= 64 * depth) return null;
  return [Math.floor(a / 64), Math.floor(b / 64)];
}
