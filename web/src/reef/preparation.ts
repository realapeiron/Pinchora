import type { Scene } from "../iso/scene";
import type { PreparedRoom } from "./compositor";
/** Layout/texture projection is isolated from the frame loop, including during streaming. */
export class RoomPreparation {
  private worker = new Worker(new URL("./prepare.worker.ts", import.meta.url), { type: "module" });
  private next = 0;
  private pending = new Map<
    number,
    { resolve: (room: PreparedRoom) => void; reject: (error: Error) => void }
  >();
  constructor() {
    this.worker.onmessage = (event: MessageEvent<{ id: number; room: PreparedRoom; error?: string }>) => {
      const p = this.pending.get(event.data.id);
      this.pending.delete(event.data.id);
      if (event.data.error) p?.reject(new Error(event.data.error));
      else p?.resolve(event.data.room);
    };
    this.worker.onerror = (event) => {
      for (const p of this.pending.values()) p.reject(new Error(event.message));
      this.pending.clear();
    };
  }
  prepare(scene: Scene): Promise<PreparedRoom> {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, scene });
    });
  }
  dispose() {
    this.worker.terminate();
    for (const p of this.pending.values()) p.reject(new DOMException("Aborted", "AbortError"));
    this.pending.clear();
  }
}
