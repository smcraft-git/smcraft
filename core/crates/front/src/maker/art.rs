//! The pictures the maker draws over the course in its Water Level and Custom Scroll modes: the
//! game's editor models' textures (`EditModel/…` of the UI import, research/specs/maker.md C5,
//! C13), placed in world px as the game's edit actors place their models and icons, drawn as
//! layout quads (1280 × 720 units, origin centre, y up). Without the import a flat quad of the
//! same box stands in.
//!
//! - `Edit_EditPanel`: one sheet of 13 × 6 icons of 128 px [obs 0x7100da39c0, icon kind 5:
//!   cell id % 13, id / 13], each one tile (16 px) across at scale 1 [obs 0x7100da3c50: the
//!   icon's half size 16 × scale is its bounds; the frames: a path mark and a speed square
//!   measure one tile];
//! - `Edit_WaterLevelLine`: the liquid's lines (`Edit_WaterLevelLine.00` thick white,
//!   `.01` yellow, `.02` double white; one tile, 16 × 16, repeated across the view) and the
//!   start's pill (`WaterLevelBtn.00` on a 64 × 32 quad, x ±32, y −8.05 … 23.95);
//! - `Edit_dragBtn`: the drag arrows (`dragArrow.0…2`, 16 × 32 from 8 to 40 over the handle,
//!   `Arrow_wait`: .0 until frame 50, .2, .1 from 54, .2 from 62, 66 frames looping);
//! - `Edit_autoscroll_00…11`: the scroll path's marks, 16 × 16, pointing up (turned along the
//!   leg); `Edit_camera.00…20`: the birds on a 39 × 48 quad [obs: the models' shapes and
//!   texture patterns].

use mmk_ui::{
    BLEND_NORMAL, COMBINE_NONE, Quad, SHIFT_COMBINE_ALPHA, SHIFT_COMBINE_COLOR, SHIFT_WRAP_S0,
    TEXTURE_NONE,
};

use super::{MakerFlow, view};

pub type Rgba = [u8; 4];

/// The icon sheet.
pub const PANEL: &str = "Edit_EditPanel";
const PANEL_COLS: u32 = 13;
const PANEL_ROWS: u32 = 6;
/// An icon's size at scale 1 (world px).
pub const ICON: f32 = 16.0;

/// The icons of [`PANEL`] the modes draw [obs 0x7100effd00, 0x7100eb27e0].
pub mod icon {
    /// The custom scroll's speed squares: ▶ slow, ▶▶ normal, ▶▶▶ fast.
    pub const SQUARE_SLOW: u32 = 0x34;
    pub const SQUARE_NORMAL: u32 = 0x35;
    pub const SQUARE_FAST: u32 = 0x36;
    /// The water level's round button: falls, rises, rises and falls, still.
    pub const DOWN: u32 = 0x37;
    pub const UP: u32 = 0x38;
    pub const BOTH: u32 = 0x39;
    pub const STILL: u32 = 0x3a;
    /// The water level's path marks: slow, medium, fast.
    pub const MARK_SLOW: u32 = 0x3b;
    pub const MARK_MEDIUM: u32 = 0x3c;
    pub const MARK_FAST: u32 = 0x3d;
    /// Where the water level's path leaves the pill.
    pub const PATH_START: u32 = 0x3e;
}

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

/// A flat rectangle (layout units).
pub fn rect(q: &mut Vec<Quad>, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba) {
    q.push(quad4([[x0, y1], [x1, y1], [x0, y0], [x1, y0]], color));
}

/// A box in world px: its centre, half size, turn (radians, counter-clockwise) and texture
/// coordinates (u0, v0, u1, v1; v down).
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub at: [f32; 2],
    pub half: [f32; 2],
    pub angle: f32,
    pub uv: [f32; 4],
}

impl Place {
    pub fn new(at: [f32; 2], half: [f32; 2]) -> Self {
        Self {
            at,
            half,
            angle: 0.0,
            uv: [0.0, 0.0, 1.0, 1.0],
        }
    }

    /// The box from `x0`/`y0` to `x1`/`y1` (world px).
    pub fn span(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(
            [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
            [(x1 - x0) / 2.0, (y1 - y0) / 2.0],
        )
    }

    pub fn turned(self, angle: f32) -> Self {
        Self { angle, ..self }
    }

    pub fn scaled(self, s: f32) -> Self {
        Self {
            half: [self.half[0] * s, self.half[1] * s],
            ..self
        }
    }
}

/// The turn that points a picture drawn pointing up along `u` (a unit direction, y up).
pub fn angle_of(u: [f32; 2]) -> f32 {
    (-u[0]).atan2(u[1])
}

impl MakerFlow {
    /// A texture of the editor's models (`EditModel/<name>`), loaded on first use.
    pub fn model_texture(&self, name: &str) -> Option<u32> {
        self.ui
            .cache_mut()
            .texture_file(view::GROUP, name)
            .map(|t| t.id)
    }

    /// Picture `tex` (else a flat `fallback`) over box `p`, at `alpha`.
    pub(super) fn picture(
        &self,
        q: &mut Vec<Quad>,
        tex: Option<u32>,
        p: Place,
        alpha: u8,
        fallback: Rgba,
    ) {
        let (s, c) = p.angle.sin_cos();
        let corner = |x: f32, y: f32| {
            let (x, y) = (x * p.half[0], y * p.half[1]);
            self.to_layout(p.at[0] + x * c - y * s, p.at[1] + x * s + y * c)
        };
        let pos = [
            corner(-1.0, 1.0),
            corner(1.0, 1.0),
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
        ];
        let Some(t) = tex else {
            let [r, g, b, a] = fallback;
            q.push(quad4(pos, [r, g, b, (a as u16 * alpha as u16 / 255) as u8]));
            return;
        };
        let [u0, v0, u1, v1] = p.uv;
        let mut quad = quad4(pos, [255, 255, 255, alpha]);
        quad.uv0 = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
        quad.tex0 = t;
        q.push(quad);
    }

    /// Model texture `name` over box `p`.
    pub(super) fn model(&self, q: &mut Vec<Quad>, name: &str, p: Place, alpha: u8, fallback: Rgba) {
        self.picture(q, self.model_texture(name), p, alpha, fallback);
    }

    /// Icon `id` of [`PANEL`] centred on `at` (world px), `scale` times its tile, turned by
    /// `angle`.
    pub(super) fn icon(&self, q: &mut Vec<Quad>, id: u32, at: [f32; 2], scale: f32, angle: f32) {
        let (col, row) = ((id % PANEL_COLS) as f32, (id / PANEL_COLS) as f32);
        let (w, h) = (1.0 / PANEL_COLS as f32, 1.0 / PANEL_ROWS as f32);
        let mut p = Place::new(at, [ICON / 2.0 * scale; 2]).turned(angle);
        p.uv = [col * w, row * h, (col + 1.0) * w, (row + 1.0) * h];
        self.model(q, PANEL, p, 255, [255, 204, 0, 255]);
    }

    /// A one-tile picture repeated across the view along the row from world y `y0` to `y0` +
    /// 16, each copy on a tile (the liquid's lines: `WaterLevelLine_model` on a whole number of
    /// tiles, its centre snapped to 32 px [obs 0x7100f01ce0]).
    pub(super) fn row_strip(
        &self,
        q: &mut Vec<Quad>,
        name: &str,
        y0: f32,
        alpha: u8,
        fallback: Rgba,
    ) {
        let x0 = (self.to_world([-640.0, 0.0])[0] / 16.0).floor() * 16.0;
        let x1 = (self.to_world([640.0, 0.0])[0] / 16.0).ceil() * 16.0;
        let mut p = Place::span(x0, y0, x1, y0 + 16.0);
        p.uv = [x0 / 16.0, 0.0, x1 / 16.0, 1.0];
        let n = q.len();
        self.model(q, name, p, alpha, fallback);
        if let Some(s) = q.get_mut(n)
            && s.tex0 != TEXTURE_NONE
        {
            s.flags |= 1 << SHIFT_WRAP_S0;
        }
    }
}
