// A room as the browser draws it: the same scene `room_image` in `pinchora-commands` builds from a
// house room. Floor pieces, rugs and wall hangings come from the placements; pets and characters take
// the free tiles, picked with the same arithmetic, so both pictures put them in the same places.

import type { Art } from "./art";
import type { Content } from "./content";
import type { Img } from "./image";
import { footprint, surfaceLift } from "./rules";
import { hash } from "./sprites";
import type { WallSide } from "./projection";

export type SceneLayer = "floor" | "rug" | "wall" | "surface" | "figure";

/** What a pixel of a scene item stands for when the pointer is over it. */
export interface ItemInfo {
  kind: "furniture" | "pet" | "character";
  /** `Sofa (Mint)`, a pet's species name, ... */
  label: string;
  /** Category and rarity for furniture, `asleep` for a sleeping pet. */
  detail: string | null;
  /** The placement behind a furniture piece. */
  placement: string | null;
}

export interface SceneItem {
  sprite: Img;
  /** Floor layers: footprint origin tile. Wall items: offset along the wall in tiles, and the level (0 = top). */
  x: number;
  y: number;
  /** Footprint as placed, already rotated when flipped. Wall items: width along the wall and height in levels. */
  w: number;
  d: number;
  layer: SceneLayer;
  /** Mirrored: floor items face the other way, wall items hang on the left wall. */
  flipped: boolean;
  /** Draws a highlight outline. */
  selected: boolean;
  /** Surface items: height of the top they stand on, in native pixels. */
  lift: number;
  info: ItemInfo;
}

export interface Cursor {
  x: number;
  y: number;
  w: number;
  d: number;
  /** Set when the cursor sits on a wall. */
  wall: WallSide | null;
  /** Green when the item fits here, red when it does not. */
  ok: boolean;
}

export interface Scene {
  width: number;
  depth: number;
  wallpaper: Img;
  floor: Img;
  items: SceneItem[];
  cursor: Cursor | null;
  /** Tile grid on the floor and walls, with the coordinates along the front edges. */
  grid: boolean;
}

/** One room as `GET /api/house/rooms/{idx}` answers. */
export interface RoomJson {
  idx: number;
  room_type: string;
  width: number;
  depth: number;
  wallpaper: string;
  floor: string;
  placements: { id: string; item_id: string; x: number; y: number; flipped: boolean }[];
  pets: { species: string; asleep: boolean }[];
  /** Codes of the characters standing in the room. */
  characters: string[];
  /** Varies where pets and characters stand. */
  seed: number;
}

export interface ViewOptions {
  edit?: boolean;
  cursor?: Cursor | null;
  /** Id of the placement drawn highlighted. */
  selected?: string | null;
  /** What a character is called when pointed at; the owner's name, say. */
  characterLabel?: string;
}

/** `Vec::swap_remove`. */
function swapRemove<T>(list: T[], at: number): T {
  const taken = list[at]!;
  const last = list.pop()!;
  if (at < list.length) list[at] = last;
  return taken;
}

const K = 0x9e3779b97f4a7c15n;

export function buildScene(content: Content, art: Art, room: RoomJson, options: ViewOptions = {}): Scene {
  const placed = room.placements.map((p) => ({ item_id: p.item_id, x: p.x, y: p.y, flipped: p.flipped }));
  const items: SceneItem[] = [];
  for (const p of room.placements) {
    const item = content.item(p.item_id);
    const fp = footprint(content, p.item_id, p.x, p.y, p.flipped);
    const sprite = art.item(p.item_id);
    if (!item || !fp || !sprite) continue;
    items.push({
      sprite,
      x: fp.rect.x,
      y: fp.rect.y,
      w: fp.rect.w,
      d: fp.rect.d,
      layer: item.base.layer,
      flipped: p.flipped,
      selected: options.selected === p.id,
      lift: fp.layer === "surface" ? surfaceLift(content, placed, fp.rect.x, fp.rect.y) : 0,
      info: {
        kind: "furniture",
        label: content.itemName(item),
        detail: `${item.base.category} · ${item.base.rarity}`,
        placement: p.id,
      },
    });
  }

  // Free floor tiles, so pets and characters never stand inside furniture.
  const taken = new Array<boolean>(room.width * room.depth).fill(false);
  for (const p of room.placements) {
    const fp = footprint(content, p.item_id, p.x, p.y, p.flipped);
    if (fp?.layer !== "floor") continue;
    for (let y = fp.rect.y; y < Math.min(fp.rect.y + fp.rect.d, room.depth); y++) {
      for (let x = fp.rect.x; x < Math.min(fp.rect.x + fp.rect.w, room.width); x++)
        taken[y * room.width + x] = true;
    }
  }
  const free: [number, number][] = [];
  for (let y = 0; y < room.depth; y++) {
    for (let x = 0; x < room.width; x++) if (!taken[y * room.width + x]) free.push([x, y]);
  }

  // Sleepers take their tiles first, each on a spot that stays the same.
  for (const pet of room.pets.filter((p) => p.asleep)) {
    if (free.length === 0) break;
    const key = hash(pet.species);
    const [x, y] = swapRemove(free, Number((key >> 16n) % BigInt(free.length)));
    items.push({
      sprite: art.sleepingPet(pet.species, (key & 1n) === 0n),
      x,
      y,
      w: 1,
      d: 1,
      layer: "figure",
      // Drawn already turned the way it lies: a mirrored "z" is not a "z".
      flipped: false,
      selected: false,
      lift: 0,
      info: { kind: "pet", label: content.petName(pet.species), detail: "asleep", placement: null },
    });
  }
  const figures: { sprite: Img; info: ItemInfo }[] = [
    ...room.pets
      .filter((p) => !p.asleep)
      .map((p) => ({
        sprite: art.pet(p.species),
        info: { kind: "pet", label: content.petName(p.species), detail: null, placement: null } as ItemInfo,
      })),
    ...room.characters.map((code) => ({
      sprite: art.character(code),
      info: {
        kind: "character",
        label: options.characterLabel ?? "Character",
        detail: null,
        placement: null,
      } as ItemInfo,
    })),
  ];
  const seed = BigInt(room.seed);
  for (const [i, figure] of figures.entries()) {
    if (free.length === 0) break;
    const mixed = BigInt.asUintN(64, seed * K + BigInt(i) * 7919n);
    // Each figure takes its own tile.
    const [x, y] = swapRemove(free, Number((mixed >> 16n) % BigInt(free.length)));
    items.push({
      sprite: figure.sprite,
      x,
      y,
      w: 1,
      d: 1,
      layer: "figure",
      flipped: (room.seed + i) % 2 === 0,
      selected: false,
      lift: 0,
      info: figure.info,
    });
  }
  return {
    width: room.width,
    depth: room.depth,
    wallpaper: art.wallpaper(room.wallpaper),
    floor: art.floor(room.floor),
    items,
    cursor: options.cursor ?? null,
    grid: options.edit ?? false,
  };
}
