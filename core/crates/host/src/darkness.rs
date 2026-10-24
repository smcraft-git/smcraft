//! The night darkness (underwater and ghost house at night): the sim's lights
//! (`mmk_sim::World::darkness`) resolved to the shapes they are drawn with, for every renderer.
//!
//! Data [obs]: `tables/LightMaskTable.json` gives an actor's light kind, shape type and y offset;
//! `tables/LightMaskWipeTypeTable.json` gives each shape type a texture of the `M0_Object_mask`
//! model and a scale per style (`M0_` = SMB1, `M3_`, `MW_`, `WU_`). A shape is drawn
//! scale × 16 world px each side of its centre [inf: fits the game's frames: the start sign's
//! ScaleType6 (3.5) light reaches 49 px with `dot_hard_96`, lit 84 of its 96 texels]; a player's
//! light is drawn at the sim's half size with the `PlayerMove` shape (67.5 px each side: 57 px of
//! light with `dot_hard_player`, lit 82 of 96 texels, as measured).

use std::collections::HashMap;

use mmk_assets::Cache;
use mmk_sim::World;
use mmk_sim::world::LightSource;

/// The model holding the light shapes.
pub const MASK_MODEL: &str = "M0_Object_mask";

/// One light to draw: the shape's texture (cache index) over the square of `half` world px each
/// side of (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DarkLight {
    pub texture: u32,
    pub x: f32,
    pub y: f32,
    pub half: f32,
}

/// This frame's darkness: `level` 0 … 1 covers everything outside the lights.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DarkFrame {
    pub level: f32,
    pub lights: Vec<DarkLight>,
}

/// The two tables, read once.
#[derive(Debug, Default)]
pub struct DarknessTables {
    loaded: bool,
    /// shape type → (texture, scale) per style prefix
    types: HashMap<(String, String), (String, f32)>,
    /// actor name → (shape type, y offset)
    actors: HashMap<String, (String, f32)>,
}

impl DarknessTables {
    fn load(&mut self, cache: &Cache) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Ok(serde_json::Value::Array(rows)) = cache.table("LightMaskWipeTypeTable") {
            for r in rows {
                let Some(name) = r.get("TypeName").and_then(|v| v.as_str()) else {
                    continue;
                };
                for p in ["M0", "M3", "MW", "WU"] {
                    let tex = r.get(format!("{p}_Texture")).and_then(|v| v.as_str());
                    let scale = r.get(format!("{p}_Scale")).and_then(|v| v.as_f64());
                    if let (Some(t), Some(s)) = (tex, scale) {
                        self.types
                            .insert((p.into(), name.to_ascii_lowercase()), (t.into(), s as f32));
                    }
                }
            }
        }
        if let Ok(serde_json::Value::Array(rows)) = cache.table("LightMaskTable") {
            for r in rows {
                let name = r.get("name").and_then(|v| v.as_str());
                let kind = r.get("MaskType_str").and_then(|v| v.as_str());
                let y = r.get("Yoffset").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if let (Some(n), Some(k)) = (name, kind) {
                    self.actors
                        .insert(n.into(), (k.to_ascii_lowercase(), y as f32));
                }
            }
        }
    }

    /// (texture, scale) of a shape type in a style.
    fn shape(&self, style: &str, kind: &str) -> Option<(&str, f32)> {
        let prefix = match style {
            "M3" | "MW" | "WU" => style,
            _ => "M0",
        };
        self.types
            .get(&(prefix.to_string(), kind.to_ascii_lowercase()))
            .map(|(t, s)| (t.as_str(), *s))
    }
}

/// The world's darkness resolved against the cache (None: no darkness in this theme).
pub fn frame(world: &World, cache: &Cache, tables: &mut DarknessTables) -> Option<DarkFrame> {
    let d = world.darkness()?;
    tables.load(cache);
    let style = world.level.style.as_str();
    let mut out = DarkFrame {
        level: d.level,
        lights: Vec::with_capacity(d.lights.len()),
    };
    for l in &d.lights {
        let (kind, half, dy) = match l.source {
            LightSource::Player => ("PlayerMove".to_string(), Some(l.half), 0.0),
            LightSource::Table(name) => match tables.actors.get(name) {
                Some((k, y)) => (k.clone(), None, *y),
                None => continue,
            },
        };
        let Some((tex, scale)) = tables.shape(style, &kind) else {
            continue;
        };
        let Some(texture) = cache.texture_index(MASK_MODEL, tex) else {
            continue;
        };
        out.lights.push(DarkLight {
            texture: texture as u32,
            x: l.x,
            y: l.y + dy,
            half: half.unwrap_or(scale * 16.0),
        });
    }
    Some(out)
}
