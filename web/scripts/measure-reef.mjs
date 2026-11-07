// Conservative response-body envelope, not a claim of measured HTTP transfer or phone FPS.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import { Buffer } from "node:buffer";
import console from "node:console";
import process from "node:process";
import { join } from "node:path";
const web = fileURLToPath(new URL("../", import.meta.url));
const root = fileURLToPath(new URL("../../", import.meta.url));
const files = readdirSync(join(web, "dist/assets"));
const code =
  files
    .filter((n) => /^(index-|ReefStage-|prepare\.worker-).*\.(js|css)$/.test(n))
    .reduce((n, f) => n + statSync(join(web, "dist/assets", f)).size, 0) +
  statSync(join(web, "dist/index.html")).size;
const manifest = JSON.parse(readFileSync(join(web, "public/assets/reef/manifest.json")));
const manifestBytes = statSync(join(web, "public/assets/reef/manifest.json")).size;
const maxSurface = (prefix) =>
  Math.max(
    ...Object.entries(manifest.sprites)
      .filter(([name]) => name.startsWith(prefix))
      .map(([, s]) => manifest.pages[s.page].bytes),
  );
const maxLook = Math.max(
  ...Object.values(manifest.looks).map((l) =>
    Object.values(l.directions).reduce((n, id) => n + manifest.pages[id].bytes, 0),
  ),
);
const fonts = ["Silkscreen-Regular.ttf", "Silkscreen-Bold.ttf"].reduce(
  (n, f) => n + statSync(join(root, "assets/fonts", f)).size,
  0,
);
const full = JSON.parse(readFileSync(join(root, "content/furniture.json")));
const fields = [
  "id",
  "name",
  "category",
  "layer",
  "footprint",
  "rarity",
  "iso_width",
  "surface_height",
  "tags",
  "variants",
];
const catalog =
  Buffer.byteLength(
    JSON.stringify(full.map((f) => Object.fromEntries(fields.map((k) => [k, f[k] ?? null])))),
  ) + 40_000;
// Covers API room/overview/me, Discord thumbnail, 6 character PNGs, HTTP headers and other data.
const reserve = 300_000;
const bytes =
  code + manifestBytes + maxSurface("wp_") + maxSurface("fl_") + maxLook + fonts + catalog + reserve;
console.log(
  JSON.stringify(
    {
      codeAndStyles: code,
      manifest: manifestBytes,
      worstInitialArt: maxSurface("wp_") + maxSurface("fl_") + maxLook,
      fonts,
      catalogAllowance: catalog,
      reserve,
      conservativeColdBytes: bytes,
      budget: 2_000_000,
    },
    null,
    2,
  ),
);
if (bytes > 2_000_000) process.exitCode = 1;
