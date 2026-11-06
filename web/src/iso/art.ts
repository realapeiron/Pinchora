// What `Sprites` is to the bot: every sprite a room needs by name, with palette variants and stand-ins
// made on first use and kept.

import type { Atlas } from "./atlas";
import type { Content, Ramp } from "./content";
import { type Img, blank } from "./image";
import { petSprite, placeholderItem, placeholderTile, recolor, sleepingPet } from "./sprites";

export class Art {
  readonly atlas: Atlas;
  readonly content: Content;
  private readonly made = new Map<string, Img>();
  private readonly characters = new Map<string, Img>();

  constructor(atlas: Atlas, content: Content) {
    this.atlas = atlas;
    this.content = content;
  }

  private cached(key: string, make: () => Img): Img {
    let sprite = this.made.get(key);
    if (!sprite) {
      sprite = make();
      this.made.set(key, sprite);
    }
    return sprite;
  }

  /** The sprite for a playable item id (`sofa__mint`); `null` for an id the catalogue does not know. */
  item(itemId: string): Img | null {
    const item = this.content.item(itemId);
    if (!item) return null;
    const { base, variant } = item;
    return this.cached(itemId, () => {
      const [w, d] = base.footprint;
      const sprite =
        this.atlas.get(`f_${base.id}`) ?? placeholderItem(base.id, w, d, base.layer, base.iso_width);
      const ramp: Ramp | undefined =
        variant === null ? undefined : this.content.catalog.variant_ramps[variant];
      return ramp ? recolor(sprite, ramp) : sprite;
    });
  }

  pet(species: string): Img {
    return this.cached(`p_${species}`, () => petSprite(this.atlas.get(`p_${species}`), species));
  }

  /** A pet lying asleep with a "z z" above it, facing left or right. */
  sleepingPet(species: string, left: boolean): Img {
    return this.cached(`ps_${species}_${left}`, () => sleepingPet(this.pet(species), left));
  }

  wallpaper(id: string): Img {
    return this.cached(`wp_${id}`, () => this.atlas.get(`wp_${id}`) ?? placeholderTile(id, true));
  }

  floor(id: string): Img {
    return this.cached(`fl_${id}`, () => this.atlas.get(`fl_${id}`) ?? placeholderTile(id, false));
  }

  /** Remembers the sprite the server drew for a character code. */
  setCharacter(code: string, sprite: Img): void {
    this.characters.set(code, sprite);
  }

  hasCharacter(code: string): boolean {
    return this.characters.has(code);
  }

  /** The character's sprite, or an empty one while it has not arrived (the figure keeps its tile). */
  character(code: string): Img {
    return this.characters.get(code) ?? blank(1, 1);
  }
}
