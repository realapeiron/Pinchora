/** Versioned, transport-independent content contract. IDs remain strings. */
export type Direction = "SW" | "SE" | "NW" | "NE";
export type Pose = "idle" | "walk1" | "walk2" | "walk3" | "walk4" | "sit" | "use1" | "use2";
export type Point = [number, number];
export interface Frame {
  direction: Direction;
  pose: Pose;
  foot_anchor: Point;
  seat_contact: Point | null;
  accessory_anchors: { hat: Point; head: Point; torso: Point };
  hands: { screen_left: Point | null; screen_right: Point | null };
}
export interface Look {
  directions: Record<Direction, string>;
  frames: Frame[];
}
export interface AssetManifest {
  version: 1;
  pages: Record<string, { url: string; bytes: number; width: number; height: number }>;
  sprites: Record<string, { page: string; rect: [number, number, number, number] }>;
  looks: Record<string, Look>;
}
export function validateManifest(m: AssetManifest): AssetManifest {
  if (m.version !== 1 || !m.pages || !m.sprites || !m.looks) throw new Error("Unsupported Reef manifest");
  for (const p of Object.values(m.pages)) {
    if (
      !/^\/assets\/reef\/[a-f0-9]+\.png$/.test(p.url) ||
      ![p.width, p.height, p.bytes].every((n) => Number.isInteger(n) && n > 0) ||
      p.width > 1024 ||
      p.height > 1024
    )
      throw new Error("Invalid Reef page");
  }
  for (const s of Object.values(m.sprites)) {
    const p = m.pages[s.page];
    const [x, y, w, h] = s.rect;
    if (
      !p ||
      !s.rect.every(Number.isInteger) ||
      x < 0 ||
      y < 0 ||
      w <= 0 ||
      h <= 0 ||
      x + w > p.width ||
      y + h > p.height
    )
      throw new Error("Invalid Reef crop");
  }
  for (const look of Object.values(m.looks)) {
    for (const d of ["SW", "SE", "NW", "NE"] as const) {
      const p = m.pages[look.directions[d]];
      if (!p || p.width !== 768 || p.height !== 128) throw new Error("Invalid animation row");
      for (const pose of POSES) {
        const frames = look.frames.filter((f) => f.direction === d && f.pose === pose);
        if (frames.length !== 1 || frames[0]!.foot_anchor.join() !== "48,120")
          throw new Error("Invalid animation frame");
      }
    }
  }
  return m;
}
export const POSES: Pose[] = ["idle", "walk1", "walk2", "walk3", "walk4", "sit", "use1", "use2"];
