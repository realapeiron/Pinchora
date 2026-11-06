//! Drawing the scene of a postcard: a profile backdrop with pets, fish and characters standing on it, from
//! the sprite atlas. The picture is drawn at the backdrop's own 240×135 pixels and scaled up whole, so
//! every pixel of every sprite stays a crisp block.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use image::{Rgba, RgbaImage, imageops};
use pinchora_game::{
    Content,
    character::preset,
    party::postcard::{Art, Scene},
};
use pinchora_render::encode_png;

use crate::{App, commands::collection::trimmed, game::scene::Sprites};

/// How much bigger the finished picture is than the backdrop.
const SCALE: u32 = 4;

/// Finished scenes by id: there are only a dozen, and the same one is shown again and again.
type Scenes = Mutex<HashMap<&'static str, Arc<Vec<u8>>>>;
static CACHE: LazyLock<Scenes> = LazyLock::new(Mutex::default);

/// The scene as a 960×540 PNG.
pub(crate) fn scene_png(app: &App, scene: &'static Scene) -> anyhow::Result<Arc<Vec<u8>>> {
    if let Some(done) = CACHE.lock().expect("scene cache").get(scene.id) {
        return Ok(done.clone());
    }
    let png = Arc::new(encode_png(&compose(&app.sprites, scene)?)?);
    CACHE.lock().expect("scene cache").insert(scene.id, png.clone());
    Ok(png)
}

/// The scene drawn: backdrop, then each prop with a small shadow under its feet, scaled up.
pub(crate) fn compose(sprites: &Sprites, scene: &Scene) -> anyhow::Result<RgbaImage> {
    let backdrop = sprites
        .atlas(&format!("bg_{}", scene.backdrop))
        .ok_or_else(|| anyhow::anyhow!("no backdrop {}", scene.backdrop))?;
    let mut canvas: RgbaImage = (*backdrop).clone();
    // Many backdrops carry a row of a flat colour at the very top (profile cards crop it away); a postcard
    // shows the whole picture, so that row is painted over with the one under it.
    for x in 0..canvas.width() {
        let below = *canvas.get_pixel(x, 1);
        canvas.put_pixel(x, 0, below);
    }
    for prop in scene.props {
        let drawn: Arc<RgbaImage> = match prop.art {
            Art::Pet(id) => sprites.pet(id),
            Art::Fish(id) => sprites.fish(id),
            Art::Hero(id) => {
                let character = preset(id)
                    .and_then(|p| p.character(Content::get()))
                    .ok_or_else(|| anyhow::anyhow!("no character preset {id}"))?;
                sprites.character(&character)
            }
        };
        let mut art = trimmed(&drawn);
        if prop.scale > 1 {
            let (w, h) = (art.width() * prop.scale, art.height() * prop.scale);
            art = imageops::resize(&art, w, h, imageops::FilterType::Nearest);
        }
        if prop.flip {
            imageops::flip_horizontal_in_place(&mut art);
        }
        let (w, h) = (art.width() as i32, art.height() as i32);
        shadow(&mut canvas, prop.x, prop.y, w);
        imageops::overlay(&mut canvas, &art, i64::from(prop.x - w / 2), i64::from(prop.y - h));
    }
    let (w, h) = (canvas.width() * SCALE, canvas.height() * SCALE);
    Ok(imageops::resize(&canvas, w, h, imageops::FilterType::Nearest))
}

/// A soft dark patch under a prop's feet, three pixel rows tall.
fn shadow(canvas: &mut RgbaImage, x: i32, y: i32, width: i32) {
    for (row, share) in [0.45f32, 0.38, 0.26].into_iter().enumerate() {
        let half = (width as f32 * share) as i32;
        for dx in -half..=half {
            let (px, py) = (x + dx, y - 1 + row as i32);
            if px < 0 || py < 0 || px >= canvas.width() as i32 || py >= canvas.height() as i32 {
                continue;
            }
            let Rgba([r, g, b, a]) = *canvas.get_pixel(px as u32, py as u32);
            let dim = |c: u8| (f32::from(c) * 0.62) as u8;
            canvas.put_pixel(px as u32, py as u32, Rgba([dim(r), dim(g), dim(b), a]));
        }
    }
}

#[cfg(test)]
mod tests {
    use pinchora_game::party::postcard::SCENES;

    use super::*;

    #[test]
    fn every_scene_draws_a_full_picture_with_something_in_it() {
        let sprites = Sprites::default();
        // `PINCHORA_PREVIEW_DIR=some/dir cargo test -p pinchora-commands scene` saves the pictures to look at.
        let preview = std::env::var_os("PINCHORA_PREVIEW_DIR").map(std::path::PathBuf::from);
        for scene in SCENES {
            let picture = compose(&sprites, scene).expect(scene.id);
            assert_eq!((picture.width(), picture.height()), (960, 540), "{}", scene.id);
            let bare = sprites.atlas(&format!("bg_{}", scene.backdrop)).expect("backdrop");
            let bare = imageops::resize(&*bare, 960, 540, imageops::FilterType::Nearest);
            let changed = picture.pixels().zip(bare.pixels()).filter(|(a, b)| a != b).count();
            assert!(
                changed > 2_000,
                "{}: the props barely show ({changed} pixels)",
                scene.id
            );
            if let Some(dir) = &preview {
                std::fs::create_dir_all(dir).expect("preview dir");
                picture.save(dir.join(format!("{}.png", scene.id))).expect("saves");
            }
        }
    }
}
