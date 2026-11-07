import { prepareRoom } from "./compositor";
import type { Scene } from "../iso/scene";
self.onmessage = (event: MessageEvent<{ id: number; scene: Scene; transparentBackdrop?: boolean }>) => {
  try {
    const room = prepareRoom(event.data.scene, event.data.transparentBackdrop);
    const buffers = new Set<ArrayBuffer>([
      room.background.data.buffer,
      room.foreground.data.buffer,
      room.hit.buffer,
    ]);
    for (const img of [
      ...room.sprites,
      ...room.scene.items.map((i) => i.sprite),
      room.scene.wallpaper,
      room.scene.floor,
    ])
      buffers.add(img.data.buffer);
    self.postMessage({ id: event.data.id, room }, { transfer: [...buffers] });
  } catch (error) {
    self.postMessage({ id: event.data.id, error: String(error) });
  }
};
