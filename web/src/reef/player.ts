import { approach, equal, findPath, spawn, type Grid, type Target } from "./navigation";
import type { Direction, Point } from "./manifest";
/** Local preview only. No inventory, reward or server-owned state is mutated. */
export class Player {
  tile: Point | null;
  position: Point | null;
  direction: Direction = "SW";
  state: "idle" | "walk" | "sit" | "use" = "idle";
  private route: Point[] = [];
  private progress = 0;
  private pending: Target | null = null;
  target: Target | null = null;
  elapsed = 0;
  message = "Ready";
  readonly grid: Grid;
  constructor(grid: Grid) {
    this.grid = grid;
    this.tile = spawn(grid);
    this.position = this.tile ? [...this.tile] : null;
    if (!this.tile) this.message = "No free entry tile. View the room or clear a floor tile in Discord.";
  }
  synchronize(
    tile: Point,
    position: Point,
    direction: Direction,
    state: "idle" | "walk" | "sit" | "use",
    seat: Omit<Target, "label"> | null,
  ) {
    this.route = [];
    this.progress = 0;
    this.pending = null;
    this.tile = tile;
    this.position = position;
    this.direction = direction;
    this.state = state;
    this.target = seat ? { ...seat, label: "" } : null;
  }
  move(goal: Point, interaction?: Target): boolean {
    if (!this.tile) return false;
    // Replanning starts at the next exact tile, never cuts across a blocked corner.
    const inFlight = this.state === "walk" && this.route.length > 1 ? this.route[1]! : this.tile;
    const path = interaction
      ? approach(this.grid, inFlight, interaction)
      : findPath(this.grid, inFlight, goal);
    if (!path) {
      this.message = "That destination is unreachable.";
      return false;
    }
    this.route = equal(inFlight, this.tile) ? path : [this.tile, ...path];
    if (equal(inFlight, this.tile)) this.progress = 0;
    this.pending = interaction ?? null;
    this.target = null;
    this.state = this.route.length > 1 ? "walk" : "idle";
    this.elapsed = 0;
    this.message = this.state === "walk" ? "Walking" : "Ready";
    if (this.route.length <= 1) this.arrive();
    return true;
  }
  private arrive() {
    this.target = this.pending;
    this.pending = null;
    this.route = [];
    if (this.target) {
      this.state = this.target.kind === "seat" ? "sit" : "use";
      this.direction = this.target.direction;
      if (this.target.kind === "usable" && this.tile) {
        const dx = this.target.anchor[0] - this.tile[0];
        const dy = this.target.anchor[1] - this.tile[1];
        if (dx || dy)
          this.direction = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? "SE" : "NW") : dy > 0 ? "SW" : "NE";
      }
      this.message = `${this.state === "sit" ? "Sitting on" : "Using"} ${this.target.label}`;
    } else {
      this.state = "idle";
      this.message = "Ready";
    }
    this.elapsed = 0;
  }
  update(dt: number, reduced: boolean) {
    this.elapsed += dt;
    if (this.state === "use" && this.elapsed >= 1000) {
      this.target = null;
      this.state = "idle";
      this.message = "Ready";
    }
    if (this.state !== "walk" || !this.tile) return;
    this.progress += Math.min(dt, 100) * 0.003;
    while (this.progress >= 1 && this.route.length > 1) {
      this.progress--;
      this.route.shift();
      this.tile = this.route[0]!;
    }
    if (this.route.length <= 1) {
      this.position = [...this.tile];
      this.arrive();
      return;
    }
    const next = this.route[1]!;
    const dx = next[0] - this.tile[0],
      dy = next[1] - this.tile[1];
    this.direction = dx > 0 ? "SE" : dx < 0 ? "NW" : dy > 0 ? "SW" : "NE";
    this.position = reduced
      ? [...this.tile]
      : [this.tile[0] + dx * this.progress, this.tile[1] + dy * this.progress];
  }
}
