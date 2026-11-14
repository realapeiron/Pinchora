/** Where each atlas sprite sits in the sheet (`/assets/atlas.json`). */
export type AtlasIndex = Record<string, { x: number; y: number; w: number; h: number }>;
export const ATLAS_URL = "/assets/atlas.png";
export const ATLAS_SIZE = { w: 1024, h: 9080 };

/** The little pixel drawings, as the bot has them: `#` the colour, `o` the light. */
export const GLYPHS: Record<string, string[]> = {
  crab: ["#.......#", "##.....##", ".#.o.o.#.", ".#######.", "#########", ".#.#.#.#."],
  heart: [".##.##.", "#o#####", "#######", ".#####.", "..###..", "...#..."],
  star: ["...#...", "..###..", "#######", ".#####.", ".##.##.", "#.....#"],
  shell: ["..###..", ".#o#o#.", "#o#o#o#", "#######", ".#.#.#.", "..###.."],
};
