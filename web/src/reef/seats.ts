/** Source-art lower foreground bands. These contain the front rim/legs, no backrest.
 * Copy original RGBA pixels after the seated actor; flipping uses the already flipped room sprite.
 * Keep this small allowlist in sync with navigation-manifest.json. */
export const SEAT_FOREGROUND_ROWS: Record<string, number> = {
  pebblerest_stool: 20,
  duneplait_stool: 16,
  mosshem_pouf: 10,
  reedwink_stool: 20,
  hemloop_chair: 24,
};
