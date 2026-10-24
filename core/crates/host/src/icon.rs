//! Part pictures for the Course Maker, from the engine's own art: the game renders its palette
//! icons from the parts' models at run time (research/specs/maker.md C2, private), so the
//! hosts draw the part alone in a small course with the sprite resolver and copy its pixels out
//! of the textures on the CPU.

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
    let sprites: Vec<DrawSprite> = draw(&e.course)
        .into_iter()
        .filter(|s| !scenery.contains(&key(s)))
        .collect();
    // the part's rectangle (world px), a little margin for what sticks out (the Koopa's head;
    // wings at its sides and a parachute above take more)
    let m = if flags & 0x8002 != 0 { 20.0 } else { 4.0 };
    let (x0, y0) = (r.x as f32 * 16.0 - m, r.y as f32 * 16.0 - m);
    let (x1, y1) = ((r.x + r.w) as f32 * 16.0 + m, (r.y + r.h) as f32 * 16.0 + m);
    let scale = 3.0;
    let (w, h) = (((x1 - x0) * scale) as u32, ((y1 - y0) * scale) as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let any = blit(cache, &sprites, [x0, y0, x1, y1], scale, (w, h), &mut rgba);
    if !any {
        return None;
    }
    // trimmed to what was drawn
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
) -> bool {
    let [x0, y0, x1, y1] = area;
    let mut decoded: std::collections::HashMap<u32, Option<(u32, u32, Vec<u8>)>> =
        Default::default();
    let mut any = false;
    for s in sprites {
        let Some(t) = s.texture else { continue };
        if s.x >= x1 || s.x + s.w <= x0 || s.y >= y1 || s.y + s.h <= y0 {
            continue;
        }
        let tex = decoded
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
    );
    Icon {
        width: w,
        height: h,
        rgba,
    }
}
