import type { Content, Furniture } from "../iso/content";
import { footprint } from "../iso/rules";
import type { RoomJson } from "../iso/scene";
import type { Direction, Point } from "./manifest";

export interface Interaction {
  kind: "seat" | "usable";
  approaches: Point[];
  anchor: Point;
  height: number;
  direction: Direction;
  /** Optional original-art foreground mask; no guessed arm masks. */
  foreground?: string;
}
export interface NavigationClass {
  kind: "walkable" | "blocking" | "seat" | "usable";
  /** null blocks the entire footprint; [] is walk-through. */
  blocked: Point[] | null;
  interaction?: Interaction;
}
export interface NavigationManifest {
  version: 1;
  classes: Record<string, NavigationClass>;
}
// Small pilot allowlist: unlisted floor furniture remains solid, never inferred as a usable seat.
export const NAVIGATION: NavigationManifest = {
  version: 1,
  classes: {
    pebblerest_stool: {
      kind: "seat",
      blocked: null,
      interaction: {
        kind: "seat",
        approaches: [
          [0, 1],
          [1, 0],
          [-1, 0],
          [0, -1],
        ],
        anchor: [0, 0],
        height: 20,
        direction: "SW",
      },
    },
    duneplait_stool: {
      kind: "seat",
      blocked: null,
      interaction: {
        kind: "seat",
        approaches: [
          [0, 1],
          [1, 0],
          [-1, 0],
          [0, -1],
        ],
        anchor: [0, 0],
        height: 18,
        direction: "SW",
      },
    },
    mosshem_pouf: {
      kind: "seat",
      blocked: null,
      interaction: {
        kind: "seat",
        approaches: [
          [0, 1],
          [1, 0],
          [-1, 0],
          [0, -1],
        ],
        anchor: [0, 0],
        height: 12,
        direction: "SW",
      },
    },
  },
};
export function classify(base: Furniture): NavigationClass {
  const known = NAVIGATION.classes[base.id];
  if (known) return known;
  if (base.layer === "rug" || base.layer === "wall") return { kind: "walkable", blocked: [] };
  // A surface item has an explicit conservative policy: its footprint blocks even without a support.
  if (base.tags.includes("game"))
    return {
      kind: "usable",
      blocked: null,
      interaction: {
        kind: "usable",
        approaches: [
          [0, base.footprint[1]],
          [base.footprint[0], 0],
          [-1, 0],
          [0, -1],
        ],
        anchor: [0, 0],
        height: 0,
        direction: "NW",
      },
    };
  return { kind: "blocking", blocked: null };
}
export interface Grid {
  width: number;
  depth: number;
  blocked: Set<number>;
}
export interface WalkTarget {
  id: string;
  label: string;
  approaches: Point[];
}
export interface Target extends Interaction {
  id: string;
  label: string;
}
export const key = (g: Grid, p: Point) => p[1] * g.width + p[0];
export const free = (g: Grid, p: Point) =>
  p.every(Number.isInteger) &&
  p[0] >= 0 &&
  p[1] >= 0 &&
  p[0] < g.width &&
  p[1] < g.depth &&
  !g.blocked.has(key(g, p));
export const equal = (a: Point, b: Point) => a[0] === b[0] && a[1] === b[1];
export const NEIGHBORS: Point[] = [
  [1, 0],
  [0, 1],
  [-1, 0],
  [0, -1],
];
export function navigation(
  content: Content,
  room: RoomJson,
): { grid: Grid; targets: Target[]; objects: WalkTarget[] } {
  const grid: Grid = { width: room.width, depth: room.depth, blocked: new Set() };
  const targets: Target[] = [];
  const objects: WalkTarget[] = [];
  for (const p of room.placements) {
    const item = content.item(p.item_id);
    const fp = footprint(content, p.item_id, p.x, p.y, p.flipped);
    if (!item || !fp) continue;
    const c = classify(item.base);
    const { x, y, w, d } = fp.rect;
    const approaches: Point[] = [];
    if (fp.layer === "wall") approaches.push(p.flipped ? [0, p.x] : [p.x, 0]);
    else if (c.kind === "walkable") approaches.push([x, y]);
    else {
      for (let dx = 0; dx < w; dx++) approaches.push([x + dx, y + d], [x + dx, y - 1]);
      for (let dy = 0; dy < d; dy++) approaches.push([x - 1, y + dy], [x + w, y + dy]);
    }
    objects.push({ id: p.id, label: content.itemName(item), approaches });
    const transform = ([x, y]: Point): Point => (p.flipped ? [p.x + y, p.y + x] : [p.x + x, p.y + y]);
    if (c.blocked === null) {
      for (let y = fp.rect.y; y < fp.rect.y + fp.rect.d; y++)
        for (let x = fp.rect.x; x < fp.rect.x + fp.rect.w; x++)
          if (x >= 0 && y >= 0 && x < grid.width && y < grid.depth) grid.blocked.add(key(grid, [x, y]));
    } else for (const cell of c.blocked) grid.blocked.add(key(grid, transform(cell)));
    if (c.interaction) {
      const i = c.interaction;
      const facing: Record<Direction, Direction> = { SW: "SE", SE: "SW", NW: "NE", NE: "NW" };
      targets.push({
        ...i,
        id: p.id,
        label: content.itemName(item),
        anchor: transform(i.anchor),
        approaches: i.approaches.map(transform),
        direction: p.flipped ? facing[i.direction] : i.direction,
      });
    }
  }
  return { grid, targets, objects };
}
/** Four-neighbour A*. Ties: f, h, insertion order; fixtures are language-neutral JSON. */
export function findPath(g: Grid, start: Point, goal: Point): Point[] | null {
  if (!free(g, start) || !free(g, goal)) return null;
  const distance = (p: Point) => Math.abs(p[0] - goal[0]) + Math.abs(p[1] - goal[1]);
  const open = [{ p: start, g: 0, h: distance(start), serial: 0 }];
  let serial = 1;
  const cost = new Map([[key(g, start), 0]]);
  const parents = new Map<number, Point>();
  while (open.length) {
    open.sort((a, b) => a.g + a.h - (b.g + b.h) || a.h - b.h || a.serial - b.serial);
    const current = open.shift()!;
    if (current.g !== cost.get(key(g, current.p))) continue;
    if (equal(current.p, goal)) {
      const path: Point[] = [goal];
      let p = goal;
      while (!equal(p, start)) {
        p = parents.get(key(g, p))!;
        path.push(p);
      }
      return path.reverse();
    }
    for (const [dx, dy] of NEIGHBORS) {
      const p: Point = [current.p[0] + dx, current.p[1] + dy];
      const next = current.g + 1;
      if (!free(g, p) || next >= (cost.get(key(g, p)) ?? Infinity)) continue;
      cost.set(key(g, p), next);
      parents.set(key(g, p), current.p);
      open.push({ p, g: next, h: distance(p), serial: serial++ });
    }
  }
  return null;
}
export function spawn(g: Grid): Point | null {
  for (let y = g.depth - 1; y >= 0; y--) for (let x = 0; x < g.width; x++) if (free(g, [x, y])) return [x, y];
  return null;
}
export function approach(g: Grid, start: Point, target: Target): Point[] | null {
  const routes = target.approaches.map((p) => findPath(g, start, p)).filter((p): p is Point[] => p !== null);
  routes.sort((a, b) => a.length - b.length);
  return routes[0] ?? null;
}
