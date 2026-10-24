//! What the maker draws over the course view itself, as layout quads (1280 × 720 units, origin
//! centre, y up): the grid, the frame round the part (or tile) under the cursor, and the glove.
//! Textures from the import's `EditModel` group (the game's editor models, research/specs/
//! maker.md C5); without them, plain lines and a frame stand in. The grid has no texture in the
//! game's files (`Edit_grid`'s textures are placeholders): drawn as lines on every tile [inf].

use mmk_ui::{
    BLEND_NORMAL, COMBINE_NONE, Quad, SHIFT_COMBINE_ALPHA, SHIFT_COMBINE_COLOR, TEXTURE_NONE,
};

use super::{MakerFlow, SCALE, frame_rect};
use crate::DrawItem;
use crate::layout::UiKind;

pub const GROUP: &str = "EditModel";
/// The textures the maker draws with.
pub const TEXTURES: [&str; 4] = [
    "selectCursor1unit.0",
    "NormalCursor",
    "dragCursor",
    "EraserCursor.00",
];

/// One quad from (x0, y0) to (x1, y1) (layout units, y up) with texture `tex` (its whole
/// image unless `uv` = u0, v0, u1, v1 with v down).
pub fn quad(r: [f32; 4], tex: u32, uv: [f32; 4], color: [u8; 4]) -> Quad {
    let [x0, y0, x1, y1] = r;
    let [u0, v0, u1, v1] = uv;
    Quad {
        pos: [[x0, y1], [x1, y1], [x0, y0], [x1, y0]],
        uv0: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]],
        uv1: [[0.0; 2]; 4],
        color: [color; 4],
        black: [0, 0, 0, 0],
        white: [255; 4],
        tex0: tex,
        tex1: TEXTURE_NONE,
        flags: (COMBINE_NONE << SHIFT_COMBINE_COLOR)
            | (COMBINE_NONE << SHIFT_COMBINE_ALPHA)
            | BLEND_NORMAL
            | mmk_ui::FLAG_LINEAR,
    }
}

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn texture(f: &MakerFlow, name: &str) -> Option<u32> {
    f.ui_texture(name)
}

/// The grid and the cursor frame.
pub fn overlay(f: &MakerFlow) -> DrawItem {
    let mut q = Vec::new();
    // Mario's Trail under the grid: each picture where it was taken, white with its alpha
    // (research/specs/maker.md C12) [obs 0x7101333890]; nearest texels, as the course's art
    for (r, tex, alpha) in f.trail_quads() {
        let mut t = quad(r, tex, FULL, [255, 255, 255, (alpha * 255.0).round() as u8]);
        t.flags &= !mmk_ui::FLAG_LINEAR;
        q.push(t);
    }
    if f.water_mode() || f.scroll_mode() {
        // a mode over the course: the course dims under the mode's lines and handles
        // (research/specs/maker.md C13)
        if f.water_mode() {
            q.push(quad(
                [-640.0, -360.0, 640.0, 360.0],
                TEXTURE_NONE,
                FULL,
                [0, 0, 0, 80],
            ));
        }
        f.mode_quads(&mut q);
    } else if f.glove_shown() || f.carrying() {
        f.faint_mode_quads(&mut q);
        grid(f, &mut q);
        frame(f, &mut q);
        selection(f, &mut q);
    }
    if f.ring_open() {
        q.push(quad(
            [-640.0, -360.0, 640.0, 360.0],
            TEXTURE_NONE,
            FULL,
            [0, 0, 0, 120],
        ));
    }
    DrawItem::Layout {
        ui: UiKind::Smm2,
        size: [1280.0, 720.0],
        quads: q,
    }
}

fn grid(f: &MakerFlow, q: &mut Vec<Quad>) {
    let (w, h) = f.editor.size();
    let tile = 16.0 * SCALE;
    let [ax0, ay0] = f.to_layout(0.0, 0.0);
    let [ax1, ay1] = f.to_layout(w as f32 * 16.0, h as f32 * 16.0);
    let (vx0, vy0, vx1, vy1) = (
        ax0.max(-640.0),
        ay0.max(-360.0),
        ax1.min(640.0),
        ay1.min(360.0),
    );
    if vx0 >= vx1 || vy0 >= vy1 {
        return;
    }
    let line = 1.5;
    let color = [255, 255, 255, 46];
    let first = ((vx0 - ax0) / tile).ceil() as i32;
    let mut k = first;
    loop {
        let x = ax0 + k as f32 * tile;
        if x > vx1 {
            break;
        }
        q.push(quad(
            [x - line / 2.0, vy0, x + line / 2.0, vy1],
            TEXTURE_NONE,
            FULL,
            color,
        ));
        k += 1;
    }
    let mut k = ((vy0 - ay0) / tile).ceil() as i32;
    loop {
        let y = ay0 + k as f32 * tile;
        if y > vy1 {
            break;
        }
        q.push(quad(
            [vx0, y - line / 2.0, vx1, y + line / 2.0],
            TEXTURE_NONE,
            FULL,
            color,
        ));
        k += 1;
    }
}

/// The multigrab rectangle: a tinted area with an edge [inf: the game's rectangle cursor model].
fn selection(f: &MakerFlow, q: &mut Vec<Quad>) {
    let Some(r) = f.selection_shown() else {
        return;
    };
    let [x0, y0] = f.to_layout(r.x as f32 * 16.0, r.y as f32 * 16.0);
    let [x1, y1] = f.to_layout((r.x + r.w) as f32 * 16.0, (r.y + r.h) as f32 * 16.0);
    q.push(quad(
        [x0, y0, x1, y1],
        TEXTURE_NONE,
        FULL,
        [80, 170, 255, 70],
    ));
    let l = 3.0;
    for e in [
        [x0, y0, x1, y0 + l],
        [x0, y1 - l, x1, y1],
        [x0, y0, x0 + l, y1],
        [x1 - l, y0, x1, y1],
    ] {
        q.push(quad(e, TEXTURE_NONE, FULL, [80, 170, 255, 230]));
    }
}

/// The frame round what is under the cursor (`selectCursor1unit` stretched over it, else
/// four lines).
fn frame(f: &MakerFlow, q: &mut Vec<Quad>) {
    let r = frame_rect(f);
    let [x0, y0] = f.to_layout(r.x as f32 * 16.0, r.y as f32 * 16.0);
    let [x1, y1] = f.to_layout((r.x + r.w) as f32 * 16.0, (r.y + r.h) as f32 * 16.0);
    let pad = 4.0;
    let (x0, y0, x1, y1) = (x0 - pad, y0 - pad, x1 + pad, y1 + pad);
    match texture(f, "selectCursor1unit.0") {
        Some(t) => q.push(quad([x0, y0, x1, y1], t, FULL, [255; 4])),
        None => {
            let c = if f.eraser {
                [255, 90, 90, 230]
            } else {
                [255, 255, 255, 230]
            };
            let l = 4.0;
            for r in [
                [x0, y0, x1, y0 + l],
                [x0, y1 - l, x1, y1],
                [x0, y0, x0 + l, y1],
                [x1 - l, y0, x1, y1],
            ] {
                q.push(quad(r, TEXTURE_NONE, FULL, c));
            }
        }
    }
}

/// The glove at the cursor (the eraser's own cursor with the eraser on; the holding glove
/// while carrying a part).
pub fn glove(f: &MakerFlow) -> Option<DrawItem> {
    if !f.glove_shown() {
        return None;
    }
    let name = if f.eraser {
        "EraserCursor.00"
    } else if f.carrying() {
        "dragCursor"
    } else {
        "NormalCursor"
    };
    let p = f.glove_at();
    let mut quads = Vec::new();
    // the carried part rides under the glove (its palette picture)
    if let Some(t) = f.carried_part().and_then(|part| f.part_texture(part)) {
        let s = 16.0 * SCALE;
        quads.push(quad(
            [
                p[0] - s * 0.5,
                p[1] - s * 0.62,
                p[0] + s * 0.5,
                p[1] + s * 0.62,
            ],
            t,
            FULL,
            [255, 255, 255, 220],
        ));
    }
    quads.extend(match texture(f, name) {
        // the fingertip at the cursor: the glove's top-left quarter points [inf]
        Some(t) => {
            let s = 112.0;
            vec![quad(
                [
                    p[0] - s * 0.3,
                    p[1] - s * 0.7,
                    p[0] + s * 0.7,
                    p[1] + s * 0.3,
                ],
                t,
                FULL,
                [255; 4],
            )]
        }
        None => {
            let s = 14.0;
            vec![
                quad(
                    [p[0] - s, p[1] - 2.0, p[0] + s, p[1] + 2.0],
                    TEXTURE_NONE,
                    FULL,
                    [255; 4],
                ),
                quad(
                    [p[0] - 2.0, p[1] - s, p[0] + 2.0, p[1] + s],
                    TEXTURE_NONE,
                    FULL,
                    [255; 4],
                ),
            ]
        }
    });
    Some(DrawItem::Layout {
        ui: UiKind::Smm2,
        size: [1280.0, 720.0],
        quads,
    })
}
