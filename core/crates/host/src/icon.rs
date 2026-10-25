//! Part pictures for the Course Maker, from the engine's own art: the game renders its palette
//! icons from the parts' models at run time (research/specs/maker.md C2, private), so the
//! hosts draw the part alone in a small course with the sprite resolver and copy its pixels out
//! of the textures on the CPU. Parts the play world doesn't draw get their models' art here
//! (the Spike Trap: [`stand_ins`]; the Hidden Block: the ? Block's, see-through); the Clear Pipe,
//! which has only 3D models, is drawn from its meshes by a small rasteriser
//! ([`clear_pipe_pixels`]).

use mmk_assets::Cache;

use crate::{DrawSprite, SpriteResolver};

/// An RGBA picture, rows top first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A picture of part `part` placed with `flags` (0 or the default flags: as placed; else e.g.
/// wings 0x2, a parachute 0x8000, the other form 0x4, big 0x4000) as the engine draws it in
/// `style` (the Course Maker's palette icons and attribute balloon): the part alone in a small
/// course, its sprites copied out of the textures (RGBA, 3 px per world px, trimmed). None for
/// parts without 2D art or that can't be placed.
pub fn part_icon(cache: &Cache, style: &str, part: i16, flags: u32) -> Option<Icon> {
    if part == HIDDEN_BLOCK {
        // play doesn't show it: the ? Block's picture, see-through
        let (mut rgba, w, h) = part_pixels(cache, style, 5, flags, 4.0)?;
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = (px[3] as f32 * HIDDEN_BLOCK_ALPHA).round() as u8;
        }
        return trim(&rgba, w, h);
    }
    let (rgba, w, h) = part_pixels(cache, style, part, flags, 4.0)?;
    trim(&rgba, w, h)
}

/// The wings (`flags` 0x2) or the parachute (0x8000) alone, as the engine draws them on a part
/// (a Hard Block's wings, a Goomba's parachute): the pixels the attachment adds, trimmed. The
/// contents balloon's dummies (`P_DummyWingL/R_00`, `P_DummyParachute_00`) show them
/// [departure: the game renders its own icons for them at run time].
pub fn attachment_icon(cache: &Cache, style: &str, flags: u32) -> Option<Icon> {
    let part = if flags & 0x8000 != 0 { 0 } else { 6 };
    let (with, w, h) = part_pixels(cache, style, part, mmk_editor::DEFAULT_FLAGS | flags, 20.0)?;
    let (without, w0, h0) = part_pixels(cache, style, part, mmk_editor::DEFAULT_FLAGS, 20.0)?;
    if (w, h) != (w0, h0) {
        return None;
    }
    let mut out = with;
    for (px, base) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(without.as_chunks::<4>().0)
    {
        if px == base {
            px.copy_from_slice(&[0, 0, 0, 0]);
        }
    }
    trim(&out, w, h)
}

/// Part `part` with `flags` drawn alone, untrimmed: RGBA, its width and height (`margin` world
/// px round the part's rectangle).
fn part_pixels(
    cache: &Cache,
    style: &str,
    part: i16,
    flags: u32,
    margin: f32,
) -> Option<(Vec<u8>, u32, u32)> {
    if part == CLEAR_PIPE {
        return clear_pipe_pixels(cache);
    }
    let mut e = mmk_editor::Editor::new(style);
    let (tx, ty) = (20, 6);
    let placed = match part {
        7 => e.paint_ground(tx, ty).is_ok(),
        9 => e
            .place_pipe(tx, ty, 2, mmk_editor::parts::Dir::Up, 0)
            .is_ok(),
        55 => e
            .place_doors((tx, ty), (tx + 6, ty), mmk_editor::links::DoorKind::Normal)
            .is_ok(),
        97 => e.place_warp_boxes((tx, ty), (tx + 6, ty), false).is_ok(),
        // a track: one horizontal piece
        59 => e.place_track(tx, ty, 0).is_ok(),
        p => e.place(p, tx, ty).is_ok(),
    };
    if !placed {
        return None;
    }
    if flags != 0
        && flags != mmk_editor::DEFAULT_FLAGS
        && let Some(i) = e.object_at(tx, ty)
    {
        if flags & 0x6000 == 0x4000 && !e.object(i).is_some_and(|o| o.is_big()) {
            e.toggle_big(i).ok()?;
        }
        let i = e.object_at(tx, ty)?;
        let size = e.object(i)?.flags & 0x6000;
        e.set_flags(i, (flags & !0x6000) | size);
    }
    let r = match e.object_at(tx, ty).and_then(|i| e.object(i)) {
        Some(o) => mmk_editor::parts::footprint(o),
        None => mmk_editor::Rect::new(tx, ty, 1, 1),
    };
    // the sprites of the course with the part, less those of the same course without it
    // (background, scenery)
    let cam = (r.x as f32 * 16.0 - 160.0, r.y as f32 * 16.0 - 80.0);
    let draw = |course: &mmk_formats::smm2::Course| {
        let mut world = mmk_sim::World::from_course(course);
        world.player.hidden = true;
        world.look_at(cam.0, cam.1);
        let mut planner = crate::mesh::MeshPlanner::default();
        let mut resolver = SpriteResolver::default();
        let (mut sprites, mut meshes) = (Vec::new(), Vec::new());
        planner.frame(
            &world,
            cache,
            None,
            &mut resolver,
            &mut sprites,
            &mut meshes,
        );
        sprites
    };
    let key = |s: &DrawSprite| {
        (
            s.texture,
            s.x.to_bits(),
            s.y.to_bits(),
            s.w.to_bits(),
            s.h.to_bits(),
            s.u0.to_bits(),
            s.v0.to_bits(),
        )
    };
    let scenery: std::collections::HashSet<_> = draw(&mmk_editor::Editor::new(style).course)
        .iter()
        .map(key)
        .collect();
    let mut sprites: Vec<DrawSprite> = draw(&e.course)
        .into_iter()
        .filter(|s| !scenery.contains(&key(s)))
        .collect();
    // a part the world draws nothing for: its model's art
    if sprites.is_empty() {
        sprites = stand_ins(cache, style, part, r);
    }
    // the part's rectangle (world px), a little margin for what sticks out (the Koopa's head;
    // wings at its sides and a parachute above take more)
    let m = if flags & 0x8002 != 0 { 20.0 } else { margin };
    let (x0, y0) = (r.x as f32 * 16.0 - m, r.y as f32 * 16.0 - m);
    let (x1, y1) = ((r.x + r.w) as f32 * 16.0 + m, (r.y + r.h) as f32 * 16.0 + m);
    let scale = 3.0;
    let (w, h) = (((x1 - x0) * scale) as u32, ((y1 - y0) * scale) as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut decoded = Decoded::default();
    let any = blit(
        cache,
        &sprites,
        [x0, y0, x1, y1],
        scale,
        (w, h),
        &mut rgba,
        &mut decoded,
    );
    if !any {
        return None;
    }
    Some((rgba, w, h))
}

/// The Clear Pipe's object id (3D World; drawn only from its meshes).
const CLEAR_PIPE: i16 = 93;

/// The Hidden Block's object id.
const HIDDEN_BLOCK: i16 = 29;

/// The Hidden Block's picture is the ? Block's at this opacity, as the Course Maker's course
/// view draws it (`mmk_sim::render::SEE_THROUGH_ALPHA`: 3D World's `3W_Object_block_tomei`
/// material [obs data]; the 2D styles' [inf]).
pub const HIDDEN_BLOCK_ALPHA: f32 = mmk_sim::render::SEE_THROUGH_ALPHA;

/// Art standing in for part `part` (in tile rectangle `r`) where the world draws none: the
/// Spike Trap (43) is its field animation's first frame (`<S>_Field_anime_toge_<set>`, the
/// `%s_Field_anime_%s_%c` models the game's 2D styles draw their field parts with [obs data])
/// on each tile, in the plain theme's set (3D World has no Spike Trap).
fn stand_ins(cache: &Cache, style: &str, part: i16, r: mmk_editor::Rect) -> Vec<DrawSprite> {
    if part != 43 {
        return Vec::new();
    }
    let Some(set) = crate::frame::field_anime_set(cache, style, 0, false) else {
        return Vec::new();
    };
    let Some(texture) = cache.texture_index(&format!("{style}_Field_anime_toge_{set}"), "wait.0")
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dx in 0..r.w {
        for dy in 0..r.h {
            out.push(DrawSprite {
                texture: Some(texture as u32),
                x: (r.x + dx) as f32 * 16.0,
                y: (r.y + dy) as f32 * 16.0,
                w: 16.0,
                h: 16.0,
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
                layer: 12,
                flip_x: false,
                see_through: false,
            });
        }
    }
    out
}

/// The Clear Pipe's picture (it has no 2D art): `3W_Object_dokan_tomei`'s meshes drawn on the CPU
/// as the 3D World view shows them — a two-tile pipe standing up, a `dokan_tomei_unit_model`
/// under its mouth `dokan_tomei_top_model` [inf: the game's icon camera and piece count weren't
/// read], seen straight on (the maker's flat view), the glass as the hosts draw it
/// (`mesh::CLEAR_PIPE_TINT`, the vertex alpha raised by its facing): RGBA, 3 px per world px
/// with the parts' margin. None without the imported meshes.
fn clear_pipe_pixels(cache: &Cache) -> Option<(Vec<u8>, u32, u32)> {
    let mut store = crate::mesh::MeshStore::new(cache.root.clone());
    let id = store.find(mmk_sim::level::CLEAR_PIPE_MESH);
    let file = &store.mesh(id)?.file;
    let model = |n: &str| file.models.iter().find(|m| m.name == n);
    let pieces = [
        (model("dokan_tomei_unit_model")?, 0.0),
        (model("dokan_tomei_top_model")?, 16.0),
    ];
    let (margin, scale) = (4.0, 3.0);
    let (x0, y0, x1, y1) = (-16.0 - margin, -margin, 16.0 + margin, 32.0 + margin);
    let (w, h) = (((x1 - x0) * scale) as u32, ((y1 - y0) * scale) as u32);
    // the triangles in canvas px, with each corner's depth, alpha and facing
    type Corner = ([f32; 2], f32, f32, f32);
    let mut tris: Vec<[Corner; 3]> = Vec::new();
    for (m, base) in pieces {
        let b = m.bounds;
        // centred across, its bottom on `base`
        let (ox, oy) = (-(b[0] + b[3]) / 2.0, base - b[1]);
        for sh in &m.shapes {
            let range = sh.first_index as usize..(sh.first_index + sh.index_count) as usize;
            let idx = m.indices.get(range).unwrap_or(&[]);
            for t in idx.as_chunks::<3>().0 {
                let corner = |i: u32| -> Option<Corner> {
                    let v = m.vertices.get(i as usize)?;
                    let a = m.colors.get(i as usize).map_or(255, |c| c[3]);
                    let (wx, wy) = (v.position[0] + ox, v.position[1] + oy);
                    Some((
                        [(wx - x0) * scale, (y1 - wy) * scale],
                        v.position[2],
                        a as f32 / 255.0,
                        v.normal[2].abs().min(1.0),
                    ))
                };
                tris.push([corner(t[0])?, corner(t[1])?, corner(t[2])?]);
            }
        }
    }
    // back to front, each blended over what is behind
    tris.sort_by(|a, b| {
        let z = |t: &[Corner; 3]| t.iter().map(|c| c.1).sum::<f32>();
        z(a).total_cmp(&z(b))
    });
    let tint = crate::mesh::CLEAR_PIPE_TINT;
    let mut rgba = vec![0f32; (w * h * 4) as usize];
    for t in &tris {
        let [p0, p1, p2] = [t[0].0, t[1].0, t[2].0];
        let area = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p2[0] - p0[0]) * (p1[1] - p0[1]);
        if area.abs() < 1e-6 {
            continue;
        }
        let xs = [p0[0], p1[0], p2[0]];
        let ys = [p0[1], p1[1], p2[1]];
        let lo = |v: [f32; 3], n: u32| {
            (v.iter().copied().fold(f32::MAX, f32::min).floor().max(0.0) as u32).min(n)
        };
        let hi = |v: [f32; 3], n: u32| {
            (v.iter().copied().fold(f32::MIN, f32::max).ceil().max(0.0) as u32).min(n)
        };
        for py in lo(ys, h)..hi(ys, h) {
            for px in lo(xs, w)..hi(xs, w) {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let w0 = ((p1[0] - x) * (p2[1] - y) - (p2[0] - x) * (p1[1] - y)) / area;
                let w1 = ((p2[0] - x) * (p0[1] - y) - (p0[0] - x) * (p2[1] - y)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let a = (w0 * t[0].2 + w1 * t[1].2 + w2 * t[2].2).clamp(0.0, 1.0);
                // lit from the front: brighter where the glass faces the view
                let facing = w0 * t[0].3 + w1 * t[1].3 + w2 * t[2].3;
                let light = 0.7 + 0.3 * facing;
                let di = ((py * w + px) * 4) as usize;
                let da = rgba[di + 3];
                let out_a = a + da * (1.0 - a);
                if out_a <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let src = tint[c] * light;
                    rgba[di + c] = (src * a + rgba[di + c] * da * (1.0 - a)) / out_a;
                }
                rgba[di + 3] = out_a;
            }
        }
    }
    if rgba.as_chunks::<4>().0.iter().all(|p| p[3] <= 0.0) {
        return None;
    }
    Some((
        rgba.iter()
            .map(|&v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
            .collect(),
        w,
        h,
    ))
}

/// `rgba` (`w` × `h`) cut to what was drawn (None: nothing).
fn trim(rgba: &[u8], w: u32, h: u32) -> Option<Icon> {
    let opaque = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize] > 0;
    let cols: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
    let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
    let (cx0, cx1) = (*cols.first()?, *cols.last()?);
    let (cy0, cy1) = (*rows.first()?, *rows.last()?);
    let (tw, th) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for y in cy0..=cy1 {
        let a = ((y * w + cx0) * 4) as usize;
        out.extend_from_slice(&rgba[a..a + (tw * 4) as usize]);
    }
    Some(Icon {
        width: tw,
        height: th,
        rgba: out,
    })
}

/// Mario's Trail: player 1 as the engine draws it now, the way the game's `SnapshotTextureMgr`
/// renders the player into one of its snapshot textures (research/specs/maker.md C12, private):
/// the rectangle from 80 px left to 80 px right of the player's feet and from 12 px below them to
/// 52 above (3D World 24 / 4 / 44) [obs 0x710158de40], 2 px per world px, nothing but the player.
/// Returns the picture (rows top first) and that rectangle (x0, y0, x1, y1, world px, y up); None
/// when the player isn't drawn or has no 2D art.
pub fn player_snapshot(
    cache: &Cache,
    world: &mmk_sim::World,
    decoded: &mut Decoded,
) -> Option<(Icon, [f32; 4])> {
    let sp = world.player_sprite()?;
    let style = world.level.style.clone();
    let d = SpriteResolver::default().resolve_one(&style, &sp, cache);
    let (px, py) = (world.player.x, world.player.y);
    let (side, below, above) = if style == "3W" {
        (24.0, 4.0, 44.0)
    } else {
        (80.0, 12.0, 52.0)
    };
    let area = [px - side, py - below, px + side, py + above];
    let scale = 2.0;
    let (w, h) = (
        ((area[2] - area[0]) * scale) as u32,
        ((area[3] - area[1]) * scale) as u32,
    );
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    if !blit(cache, &[d], area, scale, (w, h), &mut rgba, decoded) {
        return None;
    }
    Some((
        Icon {
            width: w,
            height: h,
            rgba,
        },
        area,
    ))
}

/// Textures decoded for [`blit`] (by asset-cache index; None: not decodable), kept by a caller
/// that draws often.
#[derive(Default)]
pub struct Decoded(std::collections::HashMap<u32, Option<(u32, u32, Vec<u8>)>>);

/// Draw `sprites` (world px, y up) into an RGBA canvas of `size` showing world rectangle
/// `area` (x0, y0, x1, y1) at `scale` px per world px, nearest texels, alpha over what is
/// there. True if any pixel was drawn.
fn blit(
    cache: &Cache,
    sprites: &[DrawSprite],
    area: [f32; 4],
    scale: f32,
    (w, h): (u32, u32),
    rgba: &mut [u8],
    decoded: &mut Decoded,
) -> bool {
    let [x0, y0, x1, y1] = area;
    let mut any = false;
    for s in sprites {
        let Some(t) = s.texture else { continue };
        if s.x >= x1 || s.x + s.w <= x0 || s.y >= y1 || s.y + s.h <= y0 {
            continue;
        }
        let tex = decoded
            .0
            .entry(t)
            .or_insert_with(|| {
                let m = cache.texture(t as usize)?;
                crate::texture::decode_mtex(m).map(|p| (m.width, m.height, p))
            })
            .as_ref();
        let Some((tw, th, px)) = tex else { continue };
        // only the canvas rows and columns the sprite covers
        let cy_from = (((y1 - (s.y + s.h)) * scale).floor().max(0.0)) as u32;
        let cy_to = (((y1 - s.y) * scale).ceil().max(0.0) as u32).min(h);
        let cx_from = (((s.x - x0) * scale).floor().max(0.0)) as u32;
        let cx_to = (((s.x + s.w - x0) * scale).ceil().max(0.0) as u32).min(w);
        for cy in cy_from..cy_to {
            let wy = y1 - (cy as f32 + 0.5) / scale;
            if wy < s.y || wy >= s.y + s.h {
                continue;
            }
            let fv = (s.y + s.h - wy) / s.h;
            for cx in cx_from..cx_to {
                let wx = x0 + (cx as f32 + 0.5) / scale;
                if wx < s.x || wx >= s.x + s.w {
                    continue;
                }
                let mut fu = (wx - s.x) / s.w;
                if s.flip_x {
                    fu = 1.0 - fu;
                }
                let u = s.u0 + fu * (s.u1 - s.u0);
                let v = s.v0 + fv * (s.v1 - s.v0);
                let sx = ((u * *tw as f32) as u32).min(tw - 1);
                let sy = ((v * *th as f32) as u32).min(th - 1);
                let si = ((sy * tw + sx) * 4) as usize;
                let a = px[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let di = ((cy * w + cx) * 4) as usize;
                for c in 0..3 {
                    let d = rgba[di + c] as u32;
                    rgba[di + c] = ((px[si + c] as u32 * a + d * (255 - a)) / 255) as u8;
                }
                rgba[di + 3] = (a + rgba[di + 3] as u32 * (255 - a) / 255).min(255) as u8;
                any = true;
            }
        }
    }
    any
}

/// A course's picture for the Coursebot: its start view (384 × 216 world px, 1 px each) as the
/// engine draws it, on the sky colour [inf: the game keeps a screenshot taken when saving].
/// 3D-model styles show only what has 2D art.
pub fn course_view(cache: &Cache, course: &mmk_formats::smm2::Course) -> Icon {
    use mmk_sim::world::{VIEW_H, VIEW_W};
    let mut world = mmk_sim::World::from_course(course);
    world.look_at(0.0, 0.0);
    let (cx, cy) = (world.camera_x, world.camera_y);
    let mut planner = crate::mesh::MeshPlanner::default();
    let mut resolver = SpriteResolver::default();
    let (mut sprites, mut meshes) = (Vec::new(), Vec::new());
    planner.frame(
        &world,
        cache,
        None,
        &mut resolver,
        &mut sprites,
        &mut meshes,
    );
    let (w, h) = (VIEW_W as u32, VIEW_H as u32);
    let sky = crate::clear_color(&world, cache);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&[(sky >> 24) as u8, (sky >> 16) as u8, (sky >> 8) as u8, 255]);
    }
    blit(
        cache,
        &sprites,
        [cx, cy, cx + VIEW_W, cy + VIEW_H],
        1.0,
        (w, h),
        &mut rgba,
        &mut Decoded::default(),
    );
    Icon {
        width: w,
        height: h,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The imported cache (`MMK_ASSETS`, else ~/smm-work/cache/smm2-3.0.3); None without it, as
    /// in CI.
    fn cache() -> Option<Cache> {
        let root = std::env::var_os("MMK_ASSETS")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| std::path::PathBuf::from(h).join("smm-work/cache/smm2-3.0.3"))
            })?;
        root.join("sprites.json")
            .exists()
            .then(|| Cache::open(&root).ok())
            .flatten()
    }

    /// The Spike Trap and the Hidden Block have pictures in every style that has them (the
    /// hidden one see-through at 0.75), the Clear Pipe in 3D World from its meshes.
    #[test]
    fn parts_without_play_art_have_pictures() {
        let Some(cache) = cache() else {
            eprintln!("no imported cache: skipped");
            return;
        };
        for style in ["M1", "M3", "MW", "WU", "3W"] {
            if style != "3W" {
                assert!(
                    part_icon(&cache, style, 43, 0).is_some(),
                    "{style} spike trap"
                );
            }
            let hidden = part_icon(&cache, style, 29, 0).expect("hidden block");
            let max_a = hidden
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3])
                .max()
                .unwrap();
            assert_eq!(max_a, (255.0 * HIDDEN_BLOCK_ALPHA).round() as u8, "{style}");
        }
        let Some(pipe) = part_icon(&cache, "3W", CLEAR_PIPE, 0) else {
            eprintln!("no imported meshes: clear pipe skipped");
            return;
        };
        // two tiles wide, two tall (3 px a world px), translucent
        assert!((90..=110).contains(&pipe.width), "{}", pipe.width);
        assert!((90..=100).contains(&pipe.height), "{}", pipe.height);
        assert!(
            pipe.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] > 0 && p[3] < 255)
        );
    }
}
