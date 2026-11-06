// The catalogue the browser draws with, as `GET /api/house/content` hands it out, and the lookups the
// bot's `Content` does: an item id like `sofa__mint~bound` resolves to a base design and a palette variant.

export type Layer = "floor" | "rug" | "wall" | "surface";

export interface Furniture {
  id: string;
  name: string;
  category: string;
  layer: Layer;
  /** Tiles along x and along y when facing the front; wall items: tiles along the wall and levels. */
  footprint: [number, number];
  rarity: string;
  iso_width: number | null;
  surface_height: number | null;
  tags: string[];
  variants: string[];
}

export interface Named {
  id: string;
  name: string;
}

export interface Tier {
  tier: number;
  name: string;
  rooms: number;
  width: number;
  depth: number;
}

export type Ramp = [number, number, number][];

export interface Catalog {
  furniture: Furniture[];
  wallpapers: Named[];
  floors: Named[];
  room_types: Named[];
  pets: Named[];
  tiers: Tier[];
  base_ramp: Ramp;
  variant_ramps: Record<string, Ramp>;
}

/** What follows an item id when it cannot leave its owner. */
const SOULBOUND = "~bound";

export interface Item {
  base: Furniture;
  variant: string | null;
}

export class Content {
  readonly catalog: Catalog;
  private readonly index: Map<string, Furniture>;

  constructor(catalog: Catalog) {
    this.catalog = catalog;
    this.index = new Map(catalog.furniture.map((f) => [f.id, f]));
  }

  /** Resolves `sofa`, `sofa__mint` or `sofa__mint~bound`; `null` for anything the catalogue does not know. */
  item(id: string): Item | null {
    const plain = id.endsWith(SOULBOUND) ? id.slice(0, -SOULBOUND.length) : id;
    const cut = plain.indexOf("__");
    const baseId = cut < 0 ? plain : plain.slice(0, cut);
    const variant = cut < 0 ? null : plain.slice(cut + 2);
    const base = this.index.get(baseId);
    if (!base) return null;
    if (variant === null) return base.variants.length === 0 ? { base, variant: null } : null;
    return base.variants.includes(variant) ? { base, variant } : null;
  }

  /** The name of an item as the bot says it: `Sofa (Mint)`. */
  itemName(item: Item): string {
    if (item.variant === null) return item.base.name;
    const first = item.variant.charAt(0).toUpperCase();
    return `${item.base.name} (${first}${item.variant.slice(1)})`;
  }

  petName(species: string): string {
    return this.catalog.pets.find((p) => p.id === species)?.name ?? species;
  }

  roomName(roomType: string): string {
    return this.catalog.room_types.find((r) => r.id === roomType)?.name ?? roomType;
  }

  hasTag(base: Furniture, tag: string): boolean {
    return base.tags.includes(tag);
  }
}
