import { blank, crop, flipHorizontal, overlay, type Img } from "../iso/image";
import { decodePng } from "../iso/png";
import { bitmap } from "./compositor";
import type { AssetLoader } from "./loader";
import { POSES, type Direction, type Frame, type Look, type Point, type Pose } from "./manifest";
import { recolorCharacter } from "./palette";
import accessories from "../../../content/accessories.json";

export type Slot = "back" | "neck" | "face" | "head" | "hand";
export interface AccessoryLayer {
  id: string;
  slot: Slot;
  sprite: Img;
  offset: Point;
  anchor: Point;
  clear: Point[];
}
const slotOrder: Slot[] = ["back", "neck", "face", "head", "hand"];
function attachment(frame: Frame, slot: Slot): Point | null {
  if (slot === "head") return frame.accessory_anchors.hat;
  if (slot === "face") return frame.accessory_anchors.head;
  if (slot === "hand") return frame.hands.screen_right;
  return frame.accessory_anchors.torso;
}
/** Reposition original bot-drawn accessory pixels by frame landmarks, in slot order. */
export function composeAccessories(body: Img, frame: Frame, layers: AccessoryLayer[], padding = 0): Img {
  const out = blank(body.width + padding * 2, body.height + padding * 2);
  const back = frame.direction === "NW" || frame.direction === "NE";
  const sorted = [...layers].sort((a, b) => slotOrder.indexOf(a.slot) - slotOrder.indexOf(b.slot));
  const draw = (layer: AccessoryLayer) => {
    const point = attachment(frame, layer.slot) ?? (layer.slot === "hand" ? layer.anchor : null);
    if (!point) return;
    // Face accessories are genuinely on the far side when facing away. Busy hands put held items away.
    if ((back && layer.slot === "face") || (layer.slot === "hand" && frame.pose.startsWith("use"))) return;
    const mirror = frame.direction === "SE" || frame.direction === "NE";
    const sprite = mirror ? flipHorizontal(layer.sprite) : layer.sprite;
    const dx = mirror
      ? point[0] - (layer.offset[0] + layer.sprite.width - layer.anchor[0])
      : point[0] + layer.offset[0] - layer.anchor[0];
    const dy = point[1] + layer.offset[1] - layer.anchor[1];
    for (const [cx, cy] of layer.clear) {
      const x = padding + dx + (mirror ? layer.sprite.width - 1 - cx : cx),
        y = padding + dy + cy;
      if (x >= 0 && y >= 0 && x < out.width && y < out.height)
        out.data.fill(0, (y * out.width + x) * 4, (y * out.width + x) * 4 + 4);
    }
    overlay(out, sprite, padding + Math.round(dx), padding + Math.round(dy));
  };
  for (const layer of sorted) if (layer.slot === "back" && !back) draw(layer);
  overlay(out, body, padding, padding);
  for (const layer of sorted) if (layer.slot !== "back" || back) draw(layer);
  return out;
}
/** Extract exact procedural accessory pixels from the existing character endpoint, without new art. */
export async function accessoryLayers(
  loader: AssetLoader,
  code: string,
  look: Look,
): Promise<{ layers: AccessoryLayer[]; warnings: string[] }> {
  const [base, ...worn] = code.split("+");
  if (!worn.length) return { layers: [], warnings: [] };
  const load = async (c: string) =>
    decodePng(
      await loader.ledger.fetch(`/api/house/character.png?code=${encodeURIComponent(c)}`, loader.signal),
    );
  const original = await load(base!);
  const idle = look.frames.find((f) => f.direction === "SW" && f.pose === "idle")!;
  const layers: AccessoryLayer[] = [];
  const warnings: string[] = [];
  for (const entry of worn) {
    const id = entry.split("*")[0]!;
    const meta = accessories.find((a) => a.id === id);
    if (!meta) {
      warnings.push(`Unsupported accessory: ${id}`);
      continue;
    }
    const slot = meta.slot as Slot;
    const anchor =
      attachment(idle, slot) ?? look.frames.map((f) => attachment(f, slot)).find((p) => p !== null);
    if (!anchor) {
      warnings.push(`${id}: hand anchor unavailable`);
      continue;
    }
    if (slot === "hand" && look.frames.some((f) => !attachment(f, slot)))
      warnings.push(`${id}: missing hand landmarks use the saved anchor in those poses.`);
    const dressed = await load(`${base}+${entry}`);
    const w = Math.max(original.width, dressed.width),
      h = Math.max(original.height, dressed.height);
    const before = blank(w, h),
      after = blank(w, h);
    overlay(before, original, Math.floor(w / 2) - Math.floor(original.width / 2), h - original.height);
    overlay(after, dressed, Math.floor(w / 2) - Math.floor(dressed.width / 2), h - dressed.height);
    const diff = blank(w, h);
    const clear: Point[] = [];
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const i = (y * w + x) * 4;
        if (before.data.subarray(i, i + 4).every((v, c) => v === after.data[i + c])) continue;
        diff.data.set(after.data.subarray(i, i + 4), i);
        if (!after.data[i + 3]) clear.push([x, y]);
      }
    layers.push({ id, slot, sprite: diff, offset: [48 - Math.floor(w / 2), 120 - h], anchor, clear });
  }
  return { layers, warnings };
}
export class Animation {
  readonly frames = new Map<string, { image: HTMLCanvasElement; meta: Frame; padding: number }>();
  readonly look: Look;
  constructor(look: Look, rows: Record<Direction, Img>, code: string, layers: AccessoryLayer[]) {
    this.look = look;
    for (const meta of look.frames) {
      const col = POSES.indexOf(meta.pose);
      const body = recolorCharacter(crop(rows[meta.direction], col * 96, 0, 96, 128), code);
      const padding = layers.length ? 96 : 0;
      this.frames.set(`${meta.direction}/${meta.pose}`, {
        image: bitmap(composeAccessories(body, meta, layers, padding)),
        meta,
        padding,
      });
    }
  }
  get(direction: Direction, pose: Pose) {
    return this.frames.get(`${direction}/${pose}`)!;
  }
  dispose() {
    for (const f of this.frames.values()) {
      f.image.width = 0;
      f.image.height = 0;
    }
    this.frames.clear();
  }
}
export function poseAt(state: "idle" | "walk" | "sit" | "use", elapsed: number, reduced: boolean): Pose {
  if (state === "walk")
    return reduced ? "idle" : (["walk1", "walk2", "walk3", "walk4"] as const)[Math.floor(elapsed / 125) % 4]!;
  if (state === "use") return reduced ? "use1" : Math.floor(elapsed / 250) % 2 ? "use2" : "use1";
  return state;
}
