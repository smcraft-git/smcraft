//! The 2D styles' backdrops (`<style>_DV_<theme>` models) from the cache: the theme's model is the
//! SceneDB row's `DVModel` (`M0_DV_black`: none) [obs: tables/<style>_SceneDB.json], drawn as its
//! body tiles (the 512 × 512 pictures, side by side in the model: SMB3's desert has three) repeated
//! by the model's width, each showing its texture-pattern animation's frame (twinkling night
//! skies). The sim gives the origin and the scrolling (`mmk_sim::world::BACKDROP_MODEL`); the
//! day skies and the airships also scroll their pictures (`scroll_frames`).

use std::collections::HashMap;

use mmk_assets::Cache;
use mmk_sim::World;

/// One picture of a backdrop: its frames (cache texture indices with names), where it sits in
/// the model, and the animation choosing the frame.
#[derive(Clone, Debug)]
struct Tile {
    frames: Vec<(u32, String)>,
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, Debug, Default)]
pub struct BackdropModel {
    pub name: String,
    tiles: Vec<Tile>,
    /// The repeat width (the model's extent across).
    period: f32,
    /// Frames of one whole picture's scroll to the left (None: still).
    scroll: Option<u32>,
}

/// The backdrop's texture animation (`<model>_fts`, `tex_srt0` of each body material): its
/// translate u runs linearly from 0.5/127 to −1 + 0.5/127 over the animation's frames, looping,
/// so the picture slides one width to the left per loop [obs data: `mmk bfres --curves` of
/// every 2D `_DV_` model; meas: the SMB1 / SMB3 / SMW day skies drift 37 px between player
/// updates 600 and 900 (0.125 px a frame), the SMB3 / SMW day airships 0.5, SMB1's 1]. The
/// day skies (`_DV_athletic`) take 4096 frames; the airships 512, but SMB3's and SMW's day
/// airship (no `_D`) 1024; the other backdrops have none. (The importer keeps texture-pattern
/// tracks only, so the loop lengths are read here from the model's name.)
fn scroll_frames(name: &str) -> Option<u32> {
    let base = name.strip_suffix("_V").unwrap_or(name);
    if base.ends_with("_DV_athletic") {
        Some(4096)
    } else if base.ends_with("_DV_airship") && !base.starts_with("M1_") {
        Some(1024)
    } else if base.contains("_DV_airship") {
        Some(512)
    } else {
        None
    }
}

/// The scroll's translate u at the scene's frame `n` (see `scroll_frames`).
fn scroll_u(frames: u32, n: u32) -> f32 {
    0.5 / 127.0 - (n % frames) as f32 / frames as f32
}

/// Pictures smaller than this are the backdrop's actors (UFOs, Yoshis …), not drawn here.
const MIN_TILE: f32 = 256.0;

impl BackdropModel {
    fn load(cache: &Cache, name: &str) -> Option<Self> {
        let mut tiles: Vec<Tile> = Vec::new();
        for (i, t) in cache.textures.iter().enumerate() {
            if t.model != name {
                continue;
            }
            let Some(q) = cache.sprite_quad(name, &t.name) else {
                continue;
            };
            if q.height() < MIN_TILE {
                continue;
            }
            // frames of one picture share its rectangle
            match tiles
                .iter_mut()
                .find(|g| g.x0 == q.x0 && g.y0 == q.y0 && g.w == q.width())
            {
                Some(g) => g.frames.push((i as u32, t.name.clone())),
                None => tiles.push(Tile {
                    frames: vec![(i as u32, t.name.clone())],
                    x0: q.x0,
                    y0: q.y0,
                    w: q.width(),
                    h: q.height(),
                }),
            }
        }
        if tiles.is_empty() {
            return None;
        }
        for t in &mut tiles {
            t.frames.sort_by(|a, b| a.1.cmp(&b.1));
        }
        let lo = tiles.iter().map(|t| t.x0).fold(f32::MAX, f32::min);
        let hi = tiles.iter().map(|t| t.x0 + t.w).fold(f32::MIN, f32::max);
        Some(Self {
            name: name.to_string(),
            tiles,
            period: hi - lo,
            scroll: scroll_frames(name),
        })
    }

    /// The first picture's first frame (the clear colour's source).
    pub fn first_texture(&self) -> Option<u32> {
        self.tiles.first()?.frames.first().map(|f| f.0)
    }
}

/// The theme's backdrop model name (None: black, or a 3D style's background scene).
pub fn model_name(cache: &Cache, world: &World) -> Option<String> {
    let style = world.level.style.as_str();
    let table = cache.table(&format!("{style}_SceneDB")).ok()?;
    let rows = table.as_array()?;
    let night = world.level.night && rows.len() >= 20;
    let r = rows.get(world.level.theme as usize + if night { 10 } else { 0 })?;
    let name = r.get("DVModel")?.as_str()?;
    (name != "M0_DV_black").then(|| name.to_string())
}

/// Backdrop models by (style, theme, night), loaded once each.
#[derive(Debug, Default)]
pub struct Backdrops {
    models: HashMap<(String, u8, bool), Option<BackdropModel>>,
}

impl Backdrops {
    pub fn get(&mut self, cache: &Cache, world: &World) -> Option<&BackdropModel> {
        let key = (
            world.level.style.clone(),
            world.level.theme,
            world.level.night,
        );
        self.models
            .entry(key)
            .or_insert_with(|| {
                let name = model_name(cache, world)?;
                BackdropModel::load(cache, &name)
            })
            .as_ref()
    }

    /// The backdrop's pictures for a view whose left edge is `camera_x`, the model's origin at
    /// (ox, oy): (texture, x, y, w, h) bottom-left world px, the frame and the scroll for the
    /// scene's frame (`World::scene_frame`; a whole-course render keeps the pictures still).
    pub fn tiles(
        &mut self,
        cache: &Cache,
        world: &World,
        (ox, oy): (f32, f32),
        out: &mut Vec<(u32, f32, f32, f32, f32)>,
    ) {
        let (cam, view_w, tick) = (world.camera_x, mmk_sim::world::VIEW_W, world.scene_frame());
        let Some(m) = self.get(cache, world) else {
            return;
        };
        if m.period <= 0.0 {
            return;
        }
        // the pictures' scroll: a picture that is its model's whole width slides as the texture
        // does (every scrolling backdrop is one picture across)
        let ox = match m.scroll {
            Some(n) if !world.map_view => ox + scroll_u(n, tick) * m.period,
            _ => ox,
        };
        let anim = cache.anim(&m.name, &m.name);
        let lo = m.tiles.iter().map(|t| t.x0).fold(f32::MAX, f32::min);
        let mut k = ((cam - (ox + lo)) / m.period).floor();
        while ox + lo + k * m.period < cam + view_w {
            for t in &m.tiles {
                let x = ox + t.x0 + k * m.period;
                if x > cam + view_w || x + t.w < cam {
                    continue;
                }
                let frame = anim
                    .and_then(|a| a.texture_at(tick))
                    .and_then(|name| t.frames.iter().find(|f| f.1 == name))
                    .or(t.frames.first());
                if let Some(&(tex, _)) = frame {
                    out.push((tex, x, oy + t.y0, t.w, t.h));
                }
            }
            k += 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The day skies scroll a picture in 4096 frames, SMB3 / SMW day airships in 1024, the
    /// other airships in 512; the rest stand still.
    #[test]
    fn scroll_loops() {
        assert_eq!(scroll_frames("MW_DV_athletic"), Some(4096));
        assert_eq!(scroll_frames("M1_DV_athletic_V"), Some(4096));
        assert_eq!(scroll_frames("M3_DV_airship"), Some(1024));
        assert_eq!(scroll_frames("MW_DV_airship_V"), Some(1024));
        assert_eq!(scroll_frames("M3_DV_airship_D"), Some(512));
        assert_eq!(scroll_frames("MW_DV_airship_D_V"), Some(512));
        assert_eq!(scroll_frames("M1_DV_airship"), Some(512));
        assert_eq!(scroll_frames("M1_DV_plain"), None);
        // SMB1's airship: a pixel a frame, 2 px right of the picture's place at the start
        assert!((scroll_u(512, 0) * 512.0 - 2.016).abs() < 1e-3);
        assert!((scroll_u(512, 963) * 512.0 - (2.016 - 451.0)).abs() < 1e-3);
    }
}
