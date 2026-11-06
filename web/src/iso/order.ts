// Painter's order for things standing on the floor, from `room.rs::draw_order`.

import type { SceneItem } from "./scene";

const rank = (layer: SceneItem["layer"]): number => (layer === "floor" ? 0 : layer === "surface" ? 1 : 2);

interface Box {
  x: number;
  y: number;
  x2: number;
  y2: number;
}

/**
 * An item is drawn before another when it lies entirely behind it along x or y. Overlaps (a lamp on a
 * table, a pet on a free tile) go floor, then surface, then figures. `idx` are the indices into `items`
 * of the standing things; the result is those indices in drawing order.
 */
export function drawOrder(items: readonly SceneItem[], idx: readonly number[]): number[] {
  const box = (i: number): Box => {
    const it = items[i]!;
    return { x: it.x, y: it.y, x2: it.x + it.w, y2: it.y + it.d };
  };
  const before = (i: number, j: number): boolean => {
    const a = box(i);
    const b = box(j);
    if (a.x < b.x2 && b.x < a.x2 && a.y < b.y2 && b.y < a.y2) {
      return rank(items[i]!.layer) < rank(items[j]!.layer);
    }
    return (a.x2 <= b.x && b.y2 > a.y) || (a.y2 <= b.y && b.x2 > a.x);
  };
  const n = idx.length;
  const next: number[][] = Array.from({ length: n }, () => []);
  const indegree = new Array<number>(n).fill(0);
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      if (i !== j && before(idx[i]!, idx[j]!)) {
        next[i]!.push(j);
        indegree[j]!++;
      }
    }
  }
  // The ready item with the smallest (x + y, layer rank, position) goes first.
  const key = (k: number): [number, number, number] => {
    const b = box(idx[k]!);
    return [b.x + b.y, rank(items[idx[k]!]!.layer), k];
  };
  const less = (a: [number, number, number], b: [number, number, number]) =>
    a[0] !== b[0] ? a[0] < b[0] : a[1] !== b[1] ? a[1] < b[1] : a[2] < b[2];
  const ready: number[] = [];
  for (let k = 0; k < n; k++) if (indegree[k] === 0) ready.push(k);
  const out: number[] = [];
  const done = new Array<boolean>(n).fill(false);
  while (ready.length > 0) {
    let best = 0;
    for (let r = 1; r < ready.length; r++) if (less(key(ready[r]!), key(ready[best]!))) best = r;
    const k = ready.splice(best, 1)[0]!;
    out.push(idx[k]!);
    done[k] = true;
    for (const j of next[k]!) {
      indegree[j]!--;
      if (indegree[j] === 0) ready.push(j);
    }
  }
  // A cycle should not happen; if it does, the rest still gets drawn back to front.
  const rest: number[] = [];
  for (let k = 0; k < n; k++) if (!done[k]) rest.push(k);
  const far = (k: number) => {
    const b = box(idx[k]!);
    return b.x2 + b.y2;
  };
  rest.sort((a, b) => far(a) - far(b));
  for (const k of rest) out.push(idx[k]!);
  return out;
}
