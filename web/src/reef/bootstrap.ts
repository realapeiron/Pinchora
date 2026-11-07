import { Art } from "../iso/art";
import { Content, type Catalog } from "../iso/content";
import type { Img } from "../iso/image";
import { buildScene, type RoomJson } from "../iso/scene";
import { Animation, accessoryLayers } from "./animation";
import { AssetLoader, ByteLedger } from "./loader";
import type { Direction } from "./manifest";
import { RoomPreparation } from "./preparation";
import { navigation } from "./navigation";

export async function bootstrap(room: RoomJson, own: boolean, signal: AbortSignal) {
  const ledger = new ByteLedger();
  const [catalog, loader, code] = await Promise.all([
    ledger.json<Catalog>("/api/house/content", signal),
    AssetLoader.open(ledger, signal),
    own
      ? Promise.resolve(room.characters[0])
      : ledger
          .json<{ character: string | null }>("/api/reef/bootstrap", signal)
          .then((profile) => profile.character)
          // Older/offline preview servers still expose the original room API.
          .catch(() =>
            ledger.json<RoomJson>("/api/house/rooms/0", signal).then((ownRoom) => ownRoom.characters[0]),
          ),
  ]);
  const content = new Content(catalog);
  const look = code ? loader.manifest.looks[code.split(".")[0]!] : undefined;
  if (!code || !look)
    throw new Error(
      "Create your character in Discord before playing. Your saved look must have a Reef sheet.",
    );
  const images = new Map<string, Img>();
  const get = async (name: string) => {
    const img = await loader.sprite(name);
    if (img) images.set(name, img);
  };
  const directions = ["SW", "SE", "NW", "NE"] as const;
  const [rows, accessory] = await Promise.all([
    Promise.all(directions.map((d) => loader.page(look.directions[d]))),
    accessoryLayers(loader, code, look),
    get(`wp_${room.wallpaper}`),
    get(`fl_${room.floor}`),
  ]);
  const animation = new Animation(
    look,
    Object.fromEntries(directions.map((d, i) => [d, rows[i]!])) as Record<Direction, Img>,
    code,
    accessory.layers,
  );
  const makeScene = () =>
    buildScene(
      content,
      new Art({ get: (name) => images.get(name) ?? null, has: (name) => images.has(name) }, content),
      { ...room, characters: [] },
    );
  const names = [
    ...new Set([
      ...room.placements
        .map((p) => content.item(p.item_id))
        .filter((i) => i !== null)
        .map((i) => `f_${i.base.id}`),
      ...room.pets.map((p) => `p_${p.species}`),
    ]),
  ];
  const nav = navigation(content, room);
  const preparation = new RoomPreparation();
  signal.addEventListener("abort", () => preparation.dispose(), { once: true });
  const remoteAnimations = new Map<string, Promise<Animation>>();
  const remoteReady = new Set<Animation>();
  return {
    loader,
    ledger,
    animation,
    async loadAnimation(character: string) {
      if (character === code) return animation;
      const cached = remoteAnimations.get(character);
      if (cached) return cached;
      const job = (async () => {
        const remoteLook = loader.manifest.looks[character.split(".")[0]!];
        if (!remoteLook) throw new Error("Saved look has no Reef sheet");
        const [remoteRows, layers] = await Promise.all([
          Promise.all(directions.map((d) => loader.page(remoteLook.directions[d]))),
          accessoryLayers(loader, character, remoteLook),
        ]);
        signal.throwIfAborted();
        const result = new Animation(
          remoteLook,
          Object.fromEntries(directions.map((d, i) => [d, remoteRows[i]!])) as Record<Direction, Img>,
          character,
          layers.layers,
        );
        remoteReady.add(result);
        return result;
      })();
      remoteAnimations.set(character, job);
      return job;
    },
    releaseAnimation(character: string) {
      if (character === code) return;
      const pending = remoteAnimations.get(character);
      remoteAnimations.delete(character);
      void pending?.then((animation) => {
        animation.dispose();
        remoteReady.delete(animation);
      });
    },
    makeScene,
    preparation,
    nav,
    warnings: accessory.warnings,
    async stream(update: () => Promise<void>) {
      // Page-grouped and sequential: at most one decode in flight; layout can already be played.
      const pages = new Map<string, string[]>();
      for (const name of names) {
        const id = loader.manifest.sprites[name]?.page;
        if (id) pages.set(id, [...(pages.get(id) ?? []), name]);
      }
      for (const group of pages.values()) {
        signal.throwIfAborted();
        for (const name of group) await get(name);
        await update();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
    },
    dispose() {
      preparation.dispose();
      loader.clear();
      images.clear();
      animation.dispose();
      for (const remote of remoteReady) remote.dispose();
      remoteReady.clear();
      remoteAnimations.clear();
    },
  };
}
