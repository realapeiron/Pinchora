import { screenToTile } from "../iso/projection";
import type { Point } from "./manifest";
/** Translation and scale are integer physical pixels, including on fractional DPR phones. */
export class Camera {
  x = 0;
  y = 0;
  scale = 1;
  dpr: number;
  constructor(dpr = 1) {
    this.dpr = dpr;
    this.scale = Math.max(1, Math.round(dpr));
  }
  center(width: number, height: number, point: Point): void {
    this.x = Math.round((width * this.dpr) / 2 - point[0] * this.scale);
    this.y = Math.round((height * this.dpr) / 2 - point[1] * this.scale);
  }
  scene(cssX: number, cssY: number): Point {
    return [
      Math.floor((cssX * this.dpr - this.x) / this.scale),
      Math.floor((cssY * this.dpr - this.y) / this.scale),
    ];
  }
  pan(dx: number, dy: number): void {
    this.x += Math.round(dx * this.dpr);
    this.y += Math.round(dy * this.dpr);
  }
  zoom(next: number, at: Point): void {
    const scale = Math.max(1, Math.min(8, Math.round(next)));
    const x = (at[0] * this.dpr - this.x) / this.scale;
    const y = (at[1] * this.dpr - this.y) / this.scale;
    this.x = Math.round(at[0] * this.dpr - x * scale);
    this.y = Math.round(at[1] * this.dpr - y * scale);
    this.scale = scale;
  }
  tile(cssX: number, cssY: number, origin: Point, width: number, depth: number): Point | null {
    const [x, y] = this.scene(cssX, cssY);
    return screenToTile(x - origin[0], y - origin[1], width, depth);
  }
}
/** A tap must never survive a drag, a second pointer, or pointercancel. */
export class Gesture {
  private points = new Map<number, Point>();
  private start: Point = [0, 0];
  private moved = false;
  down(id: number, p: Point): void {
    if (!this.points.size) {
      this.start = p;
      this.moved = false;
    } else this.moved = true;
    this.points.set(id, p);
  }
  move(id: number, p: Point): { pan: Point; pinch: number } | null {
    const before = this.points.get(id);
    if (!before) return null;
    const old = [...this.points.values()];
    this.points.set(id, p);
    if (Math.hypot(p[0] - this.start[0], p[1] - this.start[1]) > 7) this.moved = true;
    if (this.points.size === 2) {
      const now = [...this.points.values()];
      return {
        pan: [0, 0],
        pinch:
          Math.hypot(now[0]![0] - now[1]![0], now[0]![1] - now[1]![1]) -
          Math.hypot(old[0]![0] - old[1]![0], old[0]![1] - old[1]![1]),
      };
    }
    return { pan: this.moved ? [p[0] - before[0], p[1] - before[1]] : [0, 0], pinch: 0 };
  }
  up(id: number): boolean {
    const existed = this.points.delete(id);
    return existed && !this.moved && !this.points.size;
  }
  cancel(): void {
    this.points.clear();
    this.moved = true;
  }
}
