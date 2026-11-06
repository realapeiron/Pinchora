# Pinchora brand kit

Pinchora is a cozy pixel ocean for Discord communities: little homes, friendly crabs,
shells, pearls, fishing and pets. The mascot is a cheerful coral crab with raised
claws, cream eyes and a navy outline. Scenes use dusk lighting, warm windows,
teal roofs and isometric game furniture.

## Palette

The source of truth is `Theme::reef()` in `crates/pinchora-render/src/theme.rs`.

| Role | Hex |
| --- | --- |
| Night sea / background top | `#1d2b53` |
| Deep ocean / background bottom | `#0f1631` |
| Panel | `#141b3d` |
| Panel edge | `#2e3d75` |
| Cream ink | `#fff1e8` |
| Muted periwinkle | `#8fa0d8` |
| Coral accent | `#ff6b4a` |
| Gold / windows | `#ffd166` |
| Mint | `#06d6a0` |
| Heart | `#ff4d6d` |

The logo uses six colors: deep ocean, night sea, coral, coral shadow `#c2413f`,
coral highlight `#ff9e7a`, and cream. Scene shading uses the existing 32-color
`assets/sprites/palette.hex`, plus panel and heart colors above. This includes sea
blue `#1f5f8b`, sky blue `#8fd3ff`, wood `#8a5a34` / `#e0ac6e`, and muted purple.
AI originals are retained for provenance; exported artwork is mapped to this
palette without dithering. Existing emoji sprites retain their original colors.

## Type and language

Use **Silkscreen Regular / Bold**, from `assets/fonts/Silkscreen-{Regular,Bold}.ttf`,
for brand headings and short captions. Its license is `assets/fonts/OFL-Silkscreen.txt`.
The supplied wordmark has custom AI-generated pixel lettering; it is artwork,
not a replacement font. The website's accessible text wordmark continues to use
Silkscreen. Use the existing Noto Sans family for longer body text.

Brand name: **PINCHORA** in the logo, **Pinchora** in prose.
Tagline, exact source string: **Build your reef. Play with your server.**
Silkscreen renders lowercase characters as capital-shaped glyphs. The build draws
the exact sentence with this font, using a binary mask and integer pixel scaling.
No tagline is baked into an AI source image.

## Logo rules

- Keep clear space of at least one eye-height around the standalone crab, and one
  quarter of the cap-height around the full wordmark. Canvas margins are not a
  substitute for this surrounding layout space.
- Prefer the full wordmark at **256 px wide or larger**. Below that, pair the crab
  with accessible Silkscreen text, as the site does. App icon: **32 px minimum**;
  16 px is a favicon-only exception with necessarily reduced detail.
- Use the cream monochrome silhouette on dark backgrounds. It is a one-ink
  silhouette with transparent surroundings, not a grayscale conversion.
- Use supplied rounded/circular tiles for avatars; opaque square web icons include
  extra mascot padding for platform masks. Never put important detail in corners.
- Preserve proportions. Scale with nearest-neighbour interpolation, ideally at
  integer multiples. Use `image-rendering: pixelated` for website rendering.
- Do not smooth edges, stretch, rotate the logo, add gradients or glow, recolor it
  arbitrarily, replace the eyes, crop claws, change the spelling, or add text to scenes.
- Use dark, quiet backgrounds behind the wordmark. Do not lay it over busy houses.

## Build and source art

Run `python3 tools/brand/build.py` with **Python 3.10+ and Pillow 10+** installed.
On the authoring machine the tested interpreter is `/opt/homebrew/bin/python3`
(Pillow 12.2). This is an offline build: it reads no environment file, requires no
API credentials and makes no network requests. `tools/brand/requirements.txt`
records the dependency. Run from any working directory.

Nine images were created using the built-in image-generation tool. Exact prompts
are saved in `tools/brand/prompts.json`. Original files are in `tools/brand/sources/`:

| Source | Native size | Purpose |
| --- | --- | --- |
| `icon.png` | 1254×1254 | Crab identity and tile concept |
| `wordmark.png` | 2172×724 | Mascot + correctly spelled PINCHORA; genuine alpha |
| `hero.png` | 2172×724 | Wide reef town; quiet left third |
| `rules.png`, `announcements.png`, `support.png` | 2172×724 each | Notice board, lighthouse beam, lifebuoy crab |
| `showcase.png`, `games.png`, `fishing.png` | 2172×724 each | Decorated room, arcade shack, fishing rod |

The builder extracts the mascot, removes isolated background artifacts, snaps
logo and scene colors to the palette, and exports using nearest-neighbour scaling.
It uses proportional crops for the Discord app and X banners. Channel artwork is
contained at full height, aligned right, and its empty left ocean edge is extended
to reach 4:1; scene objects are never stretched. Server, OG and YouTube layouts
retain the complete panorama at a proportional size.

## Deliverables and use

All paths below are relative to the repository root. PNG dimensions are pixels.

| File(s) | Size | Use |
| --- | --- | --- |
| `assets/brand/icon-1024.png` | 1024×1024 | App icon master |
| `assets/brand/icon-512.png`, `icon-256.png` | 512×512; 256×256 | App directory and small avatar |
| `assets/brand/icon-round.png` | 1024×1024 | Circular avatar with transparent corners |
| `assets/brand/icon-monochrome.png` | 1024×1024 | Cream one-ink silhouette |
| `assets/brand/mascot.png`, `web/public/mascot.png` | 432×304 each | Transparent crab; shared `Crab.tsx` component |
| `assets/brand/wordmark.png` | 2048×576 | Transparent horizontal logo |
| `assets/brand/hero.png` | 2172×724 | Clean, text-free panorama for future compositions |
| `assets/brand/discord-app-banner.png` | 680×240 | Discord application banner |
| `assets/brand/server-banner.png` | 960×540 | Discord server banner |
| `assets/brand/x-banner.png` | 1500×500 | X profile header |
| `assets/brand/youtube-banner.png` | 2560×1440 | YouTube channel art |
| `assets/brand/og.png`, `web/public/og.png` | 1200×630 each | Social/link preview; logo + single-line tagline |
| `assets/brand/channels/rules.png` | 1920×480 | Rules channel: notice board |
| `assets/brand/channels/announcements.png` | 1920×480 | Announcements: lighthouse beam |
| `assets/brand/channels/support.png` | 1920×480 | Support: crab and lifebuoy |
| `assets/brand/channels/showcase.png` | 1920×480 | Showcase: furnished room |
| `assets/brand/channels/games.png` | 1920×480 | Games: arcade shack |
| `assets/brand/channels/fishing.png` | 1920×480 | Fishing: rod, reel and bobber |
| `web/public/favicon-16x16.png`, `favicon-32x32.png`, `favicon-48x48.png` | 16²; 32²; 48² | Browser tabs |
| `web/public/apple-touch-icon.png` | 180×180 | Apple home-screen icon |
| `web/public/icon-192.png`, `icon-512.png` | 192²; 512² | Manifest icons, including maskable use |
| `web/public/favicon.ico` | 16² + 32² + 48² frames | Legacy browser favicon |
| `web/public/favicon.svg` | 512×512 viewBox | Existing preferred favicon path; embeds pixel PNG, not vector art |
| `web/public/site.webmanifest` | JSON | Existing installation metadata and icon paths |
| `assets/brand/emoji/{shell,crab,pearl,fish,heart,star,house,rod,coin,chest,key,bunny,seal,squid,wave,lantern}.png` | 32×32 each | Sixteen transparent Discord emoji |
| `assets/brand/emoji/sources.json` | JSON | Exact sprite provenance and crop/remix method |
| `assets/brand/manifest.json` | JSON | Every exported PNG's dimensions, origin and SHA-256 |

YouTube's centered **1546×423** safe rectangle is `[507,508,2053,931)` in the
2560×1440 canvas. The logo and tagline are inside it. The panorama is decorative
outside this mobile-safe region; desktop/TV reveal more of the reef. The 423 px
height requires a half-pixel centering choice; the build rounds the top to 508.

The sixteen emoji come from existing `assets/sprites/src/` artwork. Pearl and heart
are crops from the community-pearl and heartfelt-home badges. Rod is explicitly a
remix: the shell-script key's shaft rotated with nearest-neighbour interpolation,
plus a scripted line, reel and bobber. It is not an existing standalone rod sprite.

The landing retains all original favicon and OG URLs. `web/index.html` adds only
the missing 48 px link; `Crab.tsx` now uses the approved transparent mascot with its
existing dimensions and decorative accessibility behavior. `routes.ts` updates
the OG alt text to describe the new reef scene.

## Verification and review limits

The builder verifies dimensions, PNG decoding, transparent wordmark, sixteen
emoji, ICO frame sizes and resolution of every favicon/manifest link in the HTML.
TypeScript checking and Prettier checking passed for the site integration.
The kit was visually reviewed after rendering, including tiny icons, all six
channel scenes, the exact wordmark spelling, the OG tagline and YouTube safe crop.
Native AI sources are retained unmodified; use the cleaned exports for production.
Platform compression and real account crops still warrant a preview when the lead
uploads the assets. No files have been published or pushed.

## Generator ownership

`tools/brand/build.py` owns the public mascot, favicons and Open Graph image.
The Rust `write_landing_assets` generator writes only `web/src/site/showcase` cards.
`brand_assets_match` checks that the public images match their brand masters and
that all three ICO frames match the PNG favicons, preserving the kit when cards are refreshed.
