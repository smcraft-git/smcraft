//! Flat shapes for the maker's in-course guides that the import has no pictures for (the
//! water-level lines and handles, the custom scroll's path): untextured quads in layout units
//! (1280 × 720, origin centre, y up). The game draws these as models (`Edit_WaterLevelLine`,
//! `Edit_autoscroll`, `Edit_camera`, research/specs/maker.md C5) whose textures the UI import
//! doesn't copy; the shapes follow the reference frames [obs: their look; inf: sizes].

use mmk_ui::{
    BLEND_NORMAL, COMBINE_NONE, Quad, SHIFT_COMBINE_ALPHA, SHIFT_COMBINE_COLOR, TEXTURE_NONE,
};

pub type Rgba = [u8; 4];

/// A quad from four corners: top-left, top-right, bottom-left, bottom-right (a strip).
pub fn quad4(pos: [[f32; 2]; 4], color: Rgba) -> Quad {
    Quad {
        pos,
        uv0: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        uv1: [[0.0; 2]; 4],
        color: [color; 4],
        black: [0, 0, 0, 0],
        white: [255; 4],
        tex0: TEXTURE_NONE,
        tex1: TEXTURE_NONE,
        flags: (COMBINE_NONE << SHIFT_COMBINE_COLOR)
            | (COMBINE_NONE << SHIFT_COMBINE_ALPHA)
            | BLEND_NORMAL
            | mmk_ui::FLAG_LINEAR,
    }
}

pub fn rect(q: &mut Vec<Quad>, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba) {
    q.push(quad4([[x0, y1], [x1, y1], [x0, y0], [x1, y0]], color));
}

/// A triangle (a quad with its last two corners together).
pub fn tri(q: &mut Vec<Quad>, a: [f32; 2], b: [f32; 2], c: [f32; 2], color: Rgba) {
    q.push(quad4([a, b, c, c], color));
}

/// A line from `a` to `b`, `w` wide.
pub fn seg(q: &mut Vec<Quad>, a: [f32; 2], b: [f32; 2], w: f32, color: Rgba) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let n = [-dy / l * w / 2.0, dx / l * w / 2.0];
    q.push(quad4(
        [
            [a[0] + n[0], a[1] + n[1]],
            [b[0] + n[0], b[1] + n[1]],
            [a[0] - n[0], a[1] - n[1]],
            [b[0] - n[0], b[1] - n[1]],
        ],
        color,
    ));
}

/// A filled circle.
pub fn disc(q: &mut Vec<Quad>, c: [f32; 2], r: f32, color: Rgba) {
    let n = ((r * 0.8) as usize).clamp(12, 40);
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        tri(
            q,
            c,
            [c[0] + r * a0.cos(), c[1] + r * a0.sin()],
            [c[0] + r * a1.cos(), c[1] + r * a1.sin()],
            color,
        );
    }
}

/// A ring (circle outline) `w` wide.
pub fn ring(q: &mut Vec<Quad>, c: [f32; 2], r: f32, w: f32, color: Rgba) {
    let n = ((r * 0.8) as usize).clamp(12, 40);
    let (ri, ro) = (r - w / 2.0, r + w / 2.0);
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p = |r: f32, a: f32| [c[0] + r * a.cos(), c[1] + r * a.sin()];
        q.push(quad4([p(ro, a0), p(ro, a1), p(ri, a0), p(ri, a1)], color));
    }
}

/// A rectangle with round corners of radius `r`.
pub fn rounded(q: &mut Vec<Quad>, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, color: Rgba) {
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    rect(q, x0 + r, y0, x1 - r, y1, color);
    rect(q, x0, y0 + r, x0 + r, y1 - r, color);
    rect(q, x1 - r, y0 + r, x1, y1 - r, color);
    for (cx, cy, a) in [
        (x1 - r, y1 - r, 0.0f32),
        (x0 + r, y1 - r, 90.0),
        (x0 + r, y0 + r, 180.0),
        (x1 - r, y0 + r, 270.0),
    ] {
        let n = 8;
        for k in 0..n {
            let a0 = (a + 90.0 * k as f32 / n as f32).to_radians();
            let a1 = (a + 90.0 * (k + 1) as f32 / n as f32).to_radians();
            tri(
                q,
                [cx, cy],
                [cx + r * a0.cos(), cy + r * a0.sin()],
                [cx + r * a1.cos(), cy + r * a1.sin()],
                color,
            );
        }
    }
}

/// A wavy line across [x0, x1] at height `y`: a sine of `amp` and `period` (anchored to
/// `phase_x`, so it stays put on the course while the view scrolls), `w` wide.
#[allow(clippy::too_many_arguments)]
pub fn wave(
    q: &mut Vec<Quad>,
    x0: f32,
    x1: f32,
    y: f32,
    amp: f32,
    period: f32,
    phase_x: f32,
    w: f32,
    color: Rgba,
) {
    let step = period / 12.0;
    let at = |x: f32| {
        [
            x,
            y + amp * ((x - phase_x) / period * std::f32::consts::TAU).sin(),
        ]
    };
    let mut x = x0;
    while x < x1 {
        let nx = (x + step).min(x1);
        // a little overlap so the joints show no gaps
        seg(q, at(x - 0.4), at(nx + 0.4), w, color);
        x = nx;
    }
}

/// Glyphs of the small pixel font for the path's markers ("S", "G", the node numbers): 3 × 5,
/// rows top first.
fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        _ => return None,
    })
}

/// `text` in the pixel font centred on `c`, `px` per font pixel.
pub fn text(q: &mut Vec<Quad>, c: [f32; 2], text: &str, px: f32, color: Rgba) {
    let n = text.chars().count() as f32;
    let w = n * 4.0 - 1.0;
    let (ox, oy) = (c[0] - w * px / 2.0, c[1] + 2.5 * px);
    for (k, ch) in text.chars().enumerate() {
        let Some(g) = glyph(ch) else { continue };
        for (row, bits) in g.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    let x = ox + (k as f32 * 4.0 + col as f32) * px;
                    let y = oy - (row as f32 + 1.0) * px;
                    rect(q, x, y, x + px, y + px, color);
                }
            }
        }
    }
}
