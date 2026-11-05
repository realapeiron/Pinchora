//! Turns GPT-drawn sprite sheets into native-resolution, palette-exact pixel sprites and
//! packs them into the atlas the renderer embeds.
//!
//! ```text
//! pinchora-assets cut  <sheet.png> <manifest.json> <out-dir>   # sheet → one PNG per sprite
//! pinchora-assets pack <src-dir> <atlas.png> <atlas.json>       # sprites → atlas
//! ```

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Cut the sprites listed in a manifest out of a sheet.
    Cut {
        sheet: PathBuf,
        manifest: PathBuf,
        out: PathBuf,
        /// Also write a contact sheet of the results (4× scale) next to the sprites.
        #[arg(long)]
        preview: bool,
    },
    /// Pack every PNG in a folder into one atlas.
    Pack {
        src: PathBuf,
        atlas: PathBuf,
        index: PathBuf,
    },
}

/// Which sprites a sheet holds, in reading order (rows top to bottom, left to right).
#[derive(Deserialize)]
struct Manifest {
    /// Only look inside this rectangle `[x0, y0, x1, y1]` (sheet pixels).
    region: Option<[u32; 4]>,
    /// `avatar` quantizes to the character key palette instead of the shared one.
    palette: Option<String>,
    /// Scale every sprite by one factor that brings the sheet's median height to this,
    /// so figures drawn the same size stay the same size (tall hair stays taller).
    uniform_height: Option<u32>,
    items: Vec<ManifestItem>,
}

#[derive(Deserialize)]
struct ManifestItem {
    name: String,
    /// Target width in native pixels (e.g. an isometric item's iso width); 0 with a
    /// `height` keeps the drawn aspect ratio at that height.
    width: u32,
    /// Exact target height (tiles, flat wall art, rugs); otherwise keeps the drawn aspect ratio.
    height: Option<u32>,
    /// Fill every pixel (seamless tiles) instead of keying the background out.
    #[serde(default)]
    opaque: bool,
}

#[derive(Serialize, Deserialize)]
struct AtlasEntry {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Cut {
            sheet,
            manifest,
            out,
            preview,
        } => cut(&sheet, &manifest, &out, preview),
        Command::Pack { src, atlas, index } => pack(&src, &atlas, &index),
    }
}

/// The shared palette, or the character palette whose key ramps the renderer recolours.
fn palette(name: Option<&str>) -> Vec<[u8; 3]> {
    let hex = match name {
        Some("avatar") => include_str!("../../../assets/sprites/avatar_palette.hex"),
        _ => include_str!("../../../assets/sprites/palette.hex"),
    };
    hex.lines()
        .filter_map(|l| u32::from_str_radix(l.trim(), 16).ok())
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .collect()
}

/// Chroma key: only near-pure #00FF00 is background, so leafy palette greens
/// (#5bbf6a, #a6e39a, #2f7d4f) survive.
fn is_background(p: &Rgba<u8>) -> bool {
    let [r, g, b, a] = p.0;
    if a < 128 {
        return true;
    }
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    g >= 200 && r <= 110 && b <= 110 && g - r >= 110 && g - b >= 110
}

/// "Redmean" weighted RGB distance; cheap and close to perceptual.
fn distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    let rm = (f32::from(a[0]) + f32::from(b[0])) / 2.0;
    let dr = f32::from(a[0]) - f32::from(b[0]);
    let dg = f32::from(a[1]) - f32::from(b[1]);
    let db = f32::from(a[2]) - f32::from(b[2]);
    ((2.0 + rm / 256.0) * dr * dr + 4.0 * dg * dg + (2.0 + (255.0 - rm) / 256.0) * db * db).sqrt()
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> usize {
    (0..palette.len())
        .min_by(|&i, &j| distance(palette[i], c).total_cmp(&distance(palette[j], c)))
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug)]
struct BBox {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl BBox {
    fn w(&self) -> u32 {
        self.x1 - self.x0 + 1
    }
    fn h(&self) -> u32 {
        self.y1 - self.y0 + 1
    }
    fn near(&self, o: &BBox, gap: u32) -> bool {
        self.x0 <= o.x1 + gap && o.x0 <= self.x1 + gap && self.y0 <= o.y1 + gap && o.y0 <= self.y1 + gap
    }
    fn union(&self, o: &BBox) -> BBox {
        BBox {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
}

/// Connected non-background blobs, merged when closer than `gap`, in reading order.
fn blobs(img: &RgbaImage, region: [u32; 4], gap: u32) -> Vec<BBox> {
    let [rx0, ry0, rx1, ry1] = region;
    let (w, h) = img.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut boxes: Vec<(BBox, u32)> = Vec::new();
    for y in ry0..ry1.min(h) {
        for x in rx0..rx1.min(w) {
            let i = (y * w + x) as usize;
            if seen[i] || is_background(img.get_pixel(x, y)) {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[i] = true;
            let mut bb = BBox {
                x0: x,
                y0: y,
                x1: x,
                y1: y,
            };
            let mut area = 0;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                bb = bb.union(&BBox {
                    x0: cx,
                    y0: cy,
                    x1: cx,
                    y1: cy,
                });
                let neighbours = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbours {
                    if nx < rx0 || ny < ry0 || nx >= rx1.min(w) || ny >= ry1.min(h) {
                        continue;
                    }
                    let ni = (ny * w + nx) as usize;
                    if !seen[ni] && !is_background(img.get_pixel(nx, ny)) {
                        seen[ni] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            boxes.push((bb, area));
        }
    }
    let mut merged: Vec<BBox> = boxes.into_iter().filter(|(_, a)| *a >= 40).map(|(b, _)| b).collect();
    loop {
        let mut changed = false;
        'outer: for i in 0..merged.len() {
            for j in i + 1..merged.len() {
                if merged[i].near(&merged[j], gap) {
                    merged[i] = merged[i].union(&merged[j]);
                    merged.remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Reading order: cluster into rows by vertical centre.
    merged.sort_by_key(|b| b.y0 + b.y1);
    let mut rows: Vec<Vec<BBox>> = Vec::new();
    for b in merged {
        let cy = (b.y0 + b.y1) / 2;
        match rows.last_mut() {
            Some(row)
                if {
                    let r = row[0];
                    cy >= r.y0 && cy <= r.y1
                } =>
            {
                row.push(b)
            }
            _ => rows.push(vec![b]),
        }
    }
    rows.into_iter()
        .flat_map(|mut r| {
            r.sort_by_key(|b| b.x0);
            r
        })
        .collect()
}

/// Shrinks a crop to `tw`×`th` native pixels: each target pixel takes the most common
/// palette colour of its source block, or transparency when the block is mostly background.
/// With `opaque`, every block takes a colour (tiles fill their square edge to edge, and a
/// green wallpaper must not be keyed out).
fn downsample(img: &RgbaImage, bb: BBox, tw: u32, th: u32, palette: &[[u8; 3]], opaque: bool) -> RgbaImage {
    let mut out = RgbaImage::new(tw, th);
    let sx = bb.w() as f32 / tw as f32;
    let sy = bb.h() as f32 / th as f32;
    let mut cache: HashMap<[u8; 3], usize> = HashMap::new();
    for ty in 0..th {
        for tx in 0..tw {
            let (x0, x1) = (
                bb.x0 + (tx as f32 * sx) as u32,
                bb.x0 + (((tx + 1) as f32 * sx) as u32).max((tx as f32 * sx) as u32 + 1),
            );
            let (y0, y1) = (
                bb.y0 + (ty as f32 * sy) as u32,
                bb.y0 + (((ty + 1) as f32 * sy) as u32).max((ty as f32 * sy) as u32 + 1),
            );
            // Sample the inner part of the block; block edges carry the source's blur.
            let (ix0, ix1, iy0, iy1) = shrink(x0, x1, y0, y1);
            let mut counts = vec![0u32; palette.len()];
            let (mut bg, mut total) = (0u32, 0u32);
            let pixel = |x: u32, y: u32| img.get_pixel(x.min(img.width() - 1), y.min(img.height() - 1));
            let touches_bg = (iy0..iy1).any(|y| (ix0..ix1).any(|x| is_background(pixel(x, y))));
            for y in iy0..iy1 {
                for x in ix0..ix1 {
                    let p = pixel(x, y);
                    total += 1;
                    // Next to the backdrop, green-tinted pixels are key spill, not art.
                    let spill = touches_bg && i32::from(p[1]) - i32::from(p[0].max(p[2])) > 50;
                    if !opaque && (is_background(p) || spill) {
                        bg += 1;
                    } else {
                        let c = [p[0], p[1], p[2]];
                        let i = *cache.entry(c).or_insert_with(|| nearest(palette, c));
                        counts[i] += 1;
                    }
                }
            }
            if total == 0 || bg * 2 > total {
                continue;
            }
            let (best, _) = counts.iter().enumerate().max_by_key(|(_, n)| **n).unwrap_or((0, &0));
            let c = palette[best];
            out.put_pixel(tx, ty, Rgba([c[0], c[1], c[2], 255]));
        }
    }
    out
}

fn shrink(x0: u32, x1: u32, y0: u32, y1: u32) -> (u32, u32, u32, u32) {
    let cut = |a: u32, b: u32| {
        let len = b - a;
        if len >= 4 { (a + len / 4, b - len / 4) } else { (a, b) }
    };
    let (ax, bx) = cut(x0, x1);
    let (ay, by) = cut(y0, y1);
    (ax, bx, ay, by)
}

fn cut(sheet: &Path, manifest: &Path, out: &Path, preview: bool) -> anyhow::Result<()> {
    let img = image::open(sheet)
        .with_context(|| format!("open {}", sheet.display()))?
        .to_rgba8();
    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(manifest)?)?;
    let region = manifest.region.unwrap_or([0, 0, img.width(), img.height()]);
    let found = blobs(&img, region, 10);
    if found.len() != manifest.items.len() {
        bail!(
            "{}: found {} sprites but the manifest lists {} — redraw the sheet or adjust the region",
            sheet.display(),
            found.len(),
            manifest.items.len()
        );
    }
    std::fs::create_dir_all(out)?;
    let palette = palette(manifest.palette.as_deref());
    let mut made = Vec::new();
    let uniform = manifest.uniform_height.map(|h| {
        let mut heights: Vec<u32> = found.iter().map(BBox::h).collect();
        heights.sort_unstable();
        h as f32 / heights[heights.len() / 2].max(1) as f32
    });
    for (bb, item) in found.iter().zip(&manifest.items) {
        let (tw, th) = match (uniform, item.width, item.height) {
            (Some(f), _, _) => (
                (bb.w() as f32 * f).round().max(1.0) as u32,
                (bb.h() as f32 * f).round().max(1.0) as u32,
            ),
            (None, w, h) => match (w, h) {
                (0, Some(h)) => ((bb.w() as f32 * h as f32 / bb.h() as f32).round().max(1.0) as u32, h),
                (w, Some(h)) => (w, h),
                (w, None) => {
                    let w = w.max(1);
                    (w, (bb.h() as f32 * w as f32 / bb.w() as f32).round().max(1.0) as u32)
                }
            },
        };
        let sprite = downsample(&img, *bb, tw, th, &palette, item.opaque);
        sprite.save(out.join(format!("{}.png", item.name)))?;
        println!(
            "{:<28} {:>3}x{:<3} from {}x{} @({},{})",
            item.name,
            tw,
            th,
            bb.w(),
            bb.h(),
            bb.x0,
            bb.y0
        );
        made.push(sprite);
    }
    if preview {
        let scale = 4;
        let cell = made.iter().map(|s| s.width().max(s.height())).max().unwrap_or(16) * scale + 8;
        let cols = 6u32;
        let rows = (made.len() as u32).div_ceil(cols);
        let mut sheet = RgbaImage::from_pixel(cols * cell, rows * cell, Rgba([0x2e, 0x3d, 0x75, 255]));
        for (i, s) in made.iter().enumerate() {
            let big = image::imageops::resize(
                s,
                s.width() * scale,
                s.height() * scale,
                image::imageops::FilterType::Nearest,
            );
            let (cx, cy) = ((i as u32 % cols) * cell + 4, (i as u32 / cols) * cell + 4);
            image::imageops::overlay(&mut sheet, &big, i64::from(cx), i64::from(cy));
        }
        sheet.save(out.join("_preview.png"))?;
    }
    Ok(())
}

/// Shelf packing, tallest first; 1 px gaps so nearest-neighbour scaling never bleeds.
fn pack(src: &Path, atlas_path: &Path, index_path: &Path) -> anyhow::Result<()> {
    let mut sprites: Vec<(String, RgbaImage)> = Vec::new();
    for entry in std::fs::read_dir(src)? {
        let path = entry?.path();
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        if path.extension().is_some_and(|e| e == "png") && !name.starts_with('_') {
            sprites.push((name, image::open(&path)?.to_rgba8()));
        }
    }
    sprites.sort_by(|a, b| b.1.height().cmp(&a.1.height()).then(a.0.cmp(&b.0)));
    let width = 1024;
    let (mut x, mut y, mut row_h) = (0u32, 0u32, 0u32);
    let mut placed: BTreeMap<String, AtlasEntry> = BTreeMap::new();
    for (name, img) in &sprites {
        if x + img.width() > width {
            x = 0;
            y += row_h + 1;
            row_h = 0;
        }
        placed.insert(
            name.clone(),
            AtlasEntry {
                x,
                y,
                w: img.width(),
                h: img.height(),
            },
        );
        x += img.width() + 1;
        row_h = row_h.max(img.height());
    }
    let height = (y + row_h).max(1);
    let mut atlas = RgbaImage::new(width, height);
    for (name, img) in &sprites {
        let e = &placed[name];
        image::imageops::replace(&mut atlas, img, i64::from(e.x), i64::from(e.y));
    }
    atlas.save(atlas_path)?;
    std::fs::write(index_path, serde_json::to_string_pretty(&placed)?)?;
    println!("packed {} sprites into {}x{}", sprites.len(), width, height);
    Ok(())
}
