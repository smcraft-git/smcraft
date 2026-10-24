//! The 2D styles' backdrops (`<style>_DV_<theme>` models) from the cache: the theme's model is the
//! SceneDB row's `DVModel` (`M0_DV_black`: none) [obs: tables/<style>_SceneDB.json], drawn as its
//! body tiles (the 512 × 512 pictures, side by side in the model: SMB3's desert has three) repeated
//! by the model's width, each showing its texture-pattern animation's frame (twinkling night
//! skies). The sim gives the origin and the scrolling (`mmk_sim::world::BACKDROP_MODEL`).

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
    /// (ox, oy): (texture, x, y, w, h) bottom-left world px, the frame for `tick`.
    pub fn tiles(
        &mut self,
        cache: &Cache,
        world: &World,
        (ox, oy): (f32, f32),
        out: &mut Vec<(u32, f32, f32, f32, f32)>,
    ) {
        let (cam, view_w, tick) = (world.camera_x, mmk_sim::world::VIEW_W, world.tick as u32);
        let Some(m) = self.get(cache, world) else {
            return;
        };
        if m.period <= 0.0 {
            return;
        }
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
