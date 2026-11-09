import type { RoomJson } from "../../../iso/scene";
import { api, ApiError } from "../../../lib/api";
import { housePath, type HouseTarget } from "./house";

export type Piece = RoomJson["placements"][number];
export type EditOp =
  | { type: "place"; item_id: string; x: number; y: number; flipped: boolean }
  | { type: "move"; id: string; x: number; y: number; flipped: boolean }
  | { type: "flip" | "pickup"; id: string }
  | { type: "clear" | "undo" }
  | { type: "surface"; wallpaper?: string; floor?: string; confirm_price?: number }
  | { type: "upgrade"; from_tier: number }
  | { type: "lock"; locked: boolean };
export type Snapshot = Pick<RoomJson, "placements" | "undo" | "revision"> & { fingerprint: string };

export function optimistic(room: RoomJson, op: EditOp, temporary: string): RoomJson {
  const next = { ...room, placements: room.placements.map((p) => ({ ...p })) };
  switch (op.type) {
    case "place":
      next.placements.push({ ...op, id: temporary });
      break;
    case "move":
      next.placements = next.placements.map((p) => (p.id === op.id ? { ...p, ...op } : p));
      break;
    case "flip":
      next.placements = next.placements.map((p) => (p.id === op.id ? { ...p, flipped: !p.flipped } : p));
      break;
    case "pickup":
      next.placements = next.placements.filter((p) => p.id !== op.id);
      break;
    case "clear":
      next.placements = [];
      break;
    case "surface":
      next.wallpaper = op.wallpaper ?? next.wallpaper;
      next.floor = op.floor ?? next.floor;
      break;
  }
  return next;
}

/** One serialized queue per mounted room. A rejected operation discards its dependent edits. */
export class RoomQueue {
  private queue: { op: EditOp; temporary: string; request: string }[] = [];
  private serial = 0;
  private running = false;
  private disposed = false;
  room: RoomJson;
  private send: (expected: string, op: EditOp, revision?: string, request?: string) => Promise<Snapshot>;
  private reload: () => Promise<RoomJson>;
  private changed: (room: RoomJson, pending: boolean, error?: string, ids?: Record<string, string>) => void;
  constructor(
    room: RoomJson,
    send: (expected: string, op: EditOp, revision?: string, request?: string) => Promise<Snapshot>,
    reload: () => Promise<RoomJson>,
    changed: (room: RoomJson, pending: boolean, error?: string, ids?: Record<string, string>) => void,
  ) {
    this.room = room;
    this.send = send;
    this.reload = reload;
    this.changed = changed;
  }
  get pending() {
    return this.running;
  }
  dispose() {
    this.disposed = true;
    this.queue = [];
  }
  receive(room: RoomJson) {
    if (!this.running) {
      this.room = room;
      this.changed(room, false);
    }
  }
  enqueue(op: EditOp) {
    if (this.disposed) return;
    this.queue.push({ op, temporary: `pending-${++this.serial}`, request: crypto.randomUUID() });
    this.paint();
    if (!this.running) void this.drain();
  }
  private paint(error?: string, ids?: Record<string, string>) {
    if (this.disposed) return;
    const room = this.queue.reduce((r, q) => optimistic(r, q.op, q.temporary), this.room);
    this.changed(room, this.queue.length > 0, error, ids);
  }
  private async drain() {
    this.running = true;
    while (this.queue.length) {
      const first = this.queue[0]!;
      try {
        const result = await (this.room.revision
          ? this.send(this.room.fingerprint ?? "", first.op, this.room.revision, first.request)
          : this.send(this.room.fingerprint ?? "", first.op));
        if (this.disposed) break;
        const created = result.placements.find((p) => !this.room.placements.some((old) => old.id === p.id));
        this.room = { ...optimistic(this.room, first.op, first.temporary), ...result };
        this.queue.shift();
        const ids: Record<string, string> = {};
        if (created && (first.op.type === "place" || first.op.type === "move")) {
          const old = first.op.type === "move" ? first.op.id : first.temporary;
          ids[old] = created.id;
          for (const entry of this.queue)
            if ("id" in entry.op && entry.op.id === old) entry.op = { ...entry.op, id: created.id };
        }
        this.paint(undefined, ids);
      } catch (error) {
        if (this.disposed) break;
        this.queue = [];
        try {
          this.room = await this.reload();
        } catch {
          /* Keep the last confirmed room if offline. */
        }
        this.paint(
          error instanceof ApiError && error.status === 409 && error.code === "room_changed"
            ? "This room changed somewhere else (Discord?), reloaded."
            : error instanceof Error
              ? error.message
              : "Could not save this change. Try again.",
        );
      }
    }
    this.running = false;
  }
}
export function sendOperation(
  target: HouseTarget,
  room: number,
  expected: string,
  op: EditOp,
  revision?: string,
  request: string = crypto.randomUUID(),
) {
  const body = revision
    ? { expected_fingerprint: expected, expected_revision: revision, request_id: request, op }
    : { expected, op };
  const send = () => api<Snapshot>(housePath(target, `/rooms/${room}/ops`), "POST", body);
  return send().catch((error) => {
    if (revision && !(error instanceof ApiError)) return send();
    throw error;
  });
}

/** Only preview subscribers redraw when the cursor crosses a tile; inventory and room pixels stay cached. */
export class RoomCursor {
  private position: [number, number] = [0, 0];
  private snapshot = this.position;
  private frame: number | null = null;
  private listeners = new Set<() => void>();
  get = () => this.position;
  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish = () => {
    this.frame = null;
    this.snapshot = this.position;
    for (const listener of this.listeners) listener();
  };
  move(x: number, y: number) {
    if (this.position[0] === x && this.position[1] === y) return;
    this.position = [x, y];
    if (typeof requestAnimationFrame === "undefined") this.publish();
    else if (this.frame === null) this.frame = requestAnimationFrame(this.publish);
  }
  dispose() {
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    this.frame = null;
    this.listeners.clear();
  }
}
