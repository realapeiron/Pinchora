import { Art } from "../../src/iso/art";
import { Content, type Catalog } from "../../src/iso/content";
import type { Img } from "../../src/iso/image";
import { buildScene, type RoomJson } from "../../src/iso/scene";
import catalog from "../../src/iso/fixtures/content.json";
import { Animation, poseAt } from "../../src/reef/animation";
import { actorItem, Compositor, prepareRoom } from "../../src/reef/compositor";
import { AssetLoader, ByteLedger } from "../../src/reef/loader";
import type { Direction } from "../../src/reef/manifest";

async function run() {
  const content = new Content(catalog as unknown as Catalog);
  const loader = await AssetLoader.open(new ByteLedger());
  const images = new Map<string, Img>();
  const bases = content.catalog.furniture
    .filter((f) => f.layer === "floor" && f.footprint[0] === 1 && f.footprint[1] === 1)
    .slice(0, 60);
  const room: RoomJson = {
    idx: 0,
    room_type: "living",
    width: 14,
    depth: 12,
    wallpaper: "creamcalm_plaster",
    floor: "sandsettle_boards",
    characters: [],
    pets: [],
    seed: 0,
    placements: Array.from({ length: 60 }, (_, i) => {
      const base = bases[i % bases.length]!;
      return {
        id: String(i),
        item_id: base.variants.length ? `${base.id}__${base.variants[0]}` : base.id,
        x: i % 10,
        y: Math.floor(i / 10),
        flipped: false,
      };
    }),
  };
  for (const name of [...bases.map((b) => `f_${b.id}`), `wp_${room.wallpaper}`, `fl_${room.floor}`]) {
    const img = await loader.sprite(name);
    if (img) images.set(name, img);
  }
  const scene = buildScene(
    content,
    new Art({ get: (name) => images.get(name) ?? null, has: (name) => images.has(name) }, content),
    room,
  );
  const compositor = new Compositor(prepareRoom(scene));
  const look = loader.manifest.looks.bobbin!;
  const directions: Direction[] = ["SW", "SE", "NW", "NE"];
  const rows = Object.fromEntries(
    await Promise.all(directions.map(async (d) => [d, await loader.page(look.directions[d])])),
  ) as Record<Direction, Img>;
  const animation = new Animation(look, rows, "bobbin.1.11.1.2", []);
  const canvas = document.querySelector("canvas")!;
  const ctx = canvas.getContext("2d")!;
  const results = [];
  for (const count of [16, 32]) {
    const actors = Array.from({ length: count }, (_, i) => ({
      item: actorItem(i % 14, 7 + Math.floor(i / 14)),
      image: animation.get("SW", "idle").image,
      offset: [0, 8] as [number, number],
    }));
    const frames: number[] = [];
    const intervals: number[] = [];
    let previous = 0;
    for (let n = 0; n < 240; n++)
      await new Promise<void>((resolve) =>
        requestAnimationFrame((now) => {
          if (previous && n > 30) intervals.push(now - previous);
          previous = now;
          const start = performance.now();
          for (let i = 0; i < count; i++) {
            actors[i]!.item.x = (i % 14) + Math.sin(n / 60) * 0.4;
            actors[i]!.image = animation.get(directions[i % 4]!, poseAt("walk", n * 16.7, false)).image;
          }
          ctx.setTransform(1, 0, 0, 1, 0, 0);
          ctx.clearRect(0, 0, canvas.width, canvas.height);
          compositor.draw(ctx, actors);
          if (n > 30) frames.push(performance.now() - start);
          resolve();
        }),
      );
    frames.sort((a, b) => a - b);
    intervals.sort((a, b) => a - b);
    results.push({
      furniture: 60,
      avatars: count,
      cpuP95: frames[Math.floor(frames.length * 0.95)],
      intervalP95: intervals[Math.floor(intervals.length * 0.95)],
      fps: 1000 / (intervals.reduce((s, v) => s + v, 0) / intervals.length),
      cacheBytes: loader.cache.bytes,
    });
  }
  document.getElementById("result")!.textContent = JSON.stringify(results);
  document.body.dataset.done = "true";
  compositor.dispose();
  animation.dispose();
  loader.clear();
}
void run().catch((e) => {
  document.getElementById("result")!.textContent = String(e);
  document.body.dataset.error = "true";
});
