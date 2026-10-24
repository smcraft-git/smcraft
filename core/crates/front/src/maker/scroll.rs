//! The Custom Scroll mode (`L_LeftParts_00/L_ScrollEditBtn_00`, research/specs/maker.md C13):
//! the path the camera follows with autoscroll set to custom (main area only).
//!
//! The path runs from the start unit (a grey bird marked "S") through the numbered nodes
//! (part 89, EditAutoScrollCamera; up to 10) to the end unit ("G"); each point has a speed
//! square to its right for the leg after it, and the legs carry arrows in that speed's look
//! (slow ▶ yellow, normal ▶▶ red, fast ▶▶▶ blue). The band the camera's centre can reach is
//! lit, the rest of the course dims [obs: the reference frames; 89-autoscroll-camera.md].
//!
//! From the game's edit actor [obs]:
//! - the models: "camera_model" (`Edit_camera`) with the animation LocalizationS on the start
//!   unit (node number 0), LocalizationG on the end (11), "count" on the others (0x7100eb2070);
//! - a press within ±8 px of the point + (16, 0) is on its speed square (0x7100eb30e0); its tap
//!   (release without a drag) steps the speed field (flags >> 18) & 3: 0 → 1 → 2 → 0, an undo
//!   step and "change_speed" (0x7100eb3300); other presses go to the part's common edit
//!   handling (0x7100e903d0: carried like a part).
//!
//! Ours [inf: how nodes are added was not read or seen]: a press on a leg of the path and a
//! drag puts a new node there and carries it; a node is carried by its bird; the eraser (or
//! the erase button / right mouse button) on a node takes it away. Nodes are numbered left to
//! right ([`mmk_editor::Editor::renumber_scroll_nodes`]). The start's speed square sets the
//! course header's autoscroll speed (its byte 0x0E is the start unit's speed field, Pass 3).

use mmk_editor::settings::{ScrollKind, ScrollPoint, VIEW_H, VIEW_W};
use mmk_ui::Quad;

use super::shapes::{self, Rgba};
use super::{MakerFlow, Mode, view};

/// The edit sounds' part: the nodes' own user (`<style>_EditAutoScrollCamera`).
pub const SOUND_PART: i16 = 89;
/// The speed square sits this far right of its point (world px) [obs 0x7100eb30e0].
pub const SQUARE_DX: f32 = 16.0;
/// The grey bird's texture (made from the custom-autoscroll icon `Lyt_E_ScrollIcon_04`).
const BIRD: &str = "ScrollBirdGrey";

/// What a press holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDrag {
    /// A point's speed square: a tap changes the speed.
    Speed(ScrollKind),
    /// Carrying node `index` (object index).
    Node(usize),
    /// A press on the leg after point `leg`: a drag puts a node there.
    Leg(usize),
}

#[derive(Default)]
pub struct ScrollEdit {
    pub drag: Option<ScrollDrag>,
    /// The tile the press began on.
    pub from: (i32, i32),
    pub changed: bool,
}

impl MakerFlow {
    /// The Custom Scroll mode can be used: custom autoscroll in the main area.
    pub fn scroll_available(&self) -> bool {
        self.editor.area == 0 && self.editor.area().header.autoscroll_type == 4
    }

    pub fn scroll_mode(&self) -> bool {
        self.mode == Mode::Scroll
    }

    pub(super) fn enter_scroll(&mut self) {
        if !self.scroll_available() {
            return;
        }
        self.end_drag_if_any();
        self.leave_mode();
        self.mode = Mode::Scroll;
        self.scroll = ScrollEdit::default();
        self.bird_texture();
        self.show_mode_guide("Mode_Disp_ManualScrl", 1);
        self.update_mode_buttons();
    }

    /// The path's points (world px).
    pub fn scroll_points(&self) -> Vec<ScrollPoint> {
        self.editor.scroll_path()
    }

    /// A press at layout point `p` (`erase`: the eraser or the erase button): on a speed
    /// square, a node or a leg it is taken (true).
    pub(super) fn scroll_press(&mut self, p: [f32; 2], erase: bool) -> bool {
        let [x, y] = self.to_world(p);
        let pts = self.scroll_points();
        let near = |a: [f32; 2], r: f32| (x - a[0]).abs() <= r && (y - a[1]).abs() <= r;
        let tile = self.tile_at(p);
        // speed squares first (start and nodes), then the nodes' birds, then the legs
        let mut take = None;
        for pt in &pts {
            if pt.kind != ScrollKind::End && near([pt.x + SQUARE_DX, pt.y], 8.0) {
                take = Some(ScrollDrag::Speed(pt.kind));
                break;
            }
        }
        if take.is_none() {
            take = pts.iter().find_map(|pt| match pt.kind {
                ScrollKind::Node(i) if near([pt.x, pt.y], 10.0) => Some(ScrollDrag::Node(i)),
                _ => None,
            });
        }
        if take.is_none() && !erase {
            take = pts.windows(2).enumerate().find_map(|(k, w)| {
                let a = [w[0].x + SQUARE_DX, w[0].y];
                let b = [w[1].x, w[1].y];
                (seg_dist([x, y], a, b) <= 6.0).then_some(ScrollDrag::Leg(k))
            });
        }
        let Some(d) = take else {
            return false;
        };
        if erase {
            // the eraser takes a node away (its square or its bird)
            let node = match d {
                ScrollDrag::Speed(ScrollKind::Node(i)) | ScrollDrag::Node(i) => Some(i),
                _ => None,
            };
            if let Some(i) = node {
                self.editor.checkpoint();
                self.editor.remove_scroll_node(i);
                self.changed();
                self.sound(SOUND_PART, "eraser");
            }
            return true;
        }
        self.editor.checkpoint();
        self.scroll = ScrollEdit {
            drag: Some(d),
            from: tile,
            changed: false,
        };
        self.sound(SOUND_PART, "touch");
        true
    }

    /// The press goes on at `p`: a carried node follows; a leg dragged off its tile gets a new
    /// node.
    pub(super) fn scroll_hold(&mut self, p: [f32; 2], cx: &mut crate::Cx) {
        let t = self.tile_at(p);
        match self.scroll.drag {
            Some(ScrollDrag::Node(i)) => {
                let at = self
                    .editor
                    .scroll_nodes()
                    .into_iter()
                    .find(|n| n.0 == i)
                    .map(|n| n.1);
                if at.is_some_and(|a| a != t) && self.editor.move_scroll_node(i, t.0, t.1).is_ok() {
                    self.scroll.changed = true;
                    self.changed();
                    self.sound(SOUND_PART, "move");
                }
            }
            Some(ScrollDrag::Leg(k)) if t != self.scroll.from => {
                let speed = self.scroll_points().get(k).map_or(0, |pt| pt.speed.min(2));
                match self.editor.add_scroll_node(t.0, t.1, speed) {
                    Ok(i) => {
                        self.scroll.drag = Some(ScrollDrag::Node(i));
                        self.scroll.changed = true;
                        self.changed();
                        self.sound(SOUND_PART, "set");
                    }
                    Err(e) => {
                        self.scroll.drag = None;
                        self.editor.discard_checkpoint();
                        self.refuse(cx, SOUND_PART, &e);
                    }
                }
            }
            Some(ScrollDrag::Speed(_)) if t != self.scroll.from => {
                // dragged off the square: no tap
                self.scroll.from = (i32::MIN, i32::MIN);
            }
            _ => {}
        }
    }

    /// The press ends: a tap on a speed square steps its speed (slow → normal → fast → slow).
    pub(super) fn scroll_release(&mut self) {
        let Some(d) = self.scroll.drag.take() else {
            return;
        };
        match d {
            ScrollDrag::Speed(kind) if self.scroll.from != (i32::MIN, i32::MIN) => {
                match kind {
                    ScrollKind::Start => {
                        let s = self.editor.course.header.autoscroll_speed & 3;
                        self.editor.set_custom_scroll_speed((s + 1) % 3);
                    }
                    ScrollKind::Node(i) => {
                        let s = self
                            .editor
                            .object(i)
                            .map_or(0, |o| ((o.flags >> 18) & 3) as u8);
                        self.editor.set_scroll_node_speed(i, (s + 1) % 3);
                    }
                    ScrollKind::End => {}
                }
                self.changed();
                self.sound(SOUND_PART, "change_speed");
                return;
            }
            ScrollDrag::Node(_) if self.scroll.changed => self.sound(SOUND_PART, "set"),
            _ => {}
        }
        if !self.scroll.changed {
            self.editor.discard_checkpoint();
        }
    }

    /// The grey bird: the custom-autoscroll icon in greys (the game's `Edit_camera` model,
    /// whose textures the import doesn't have) [inf: its look from the reference frames].
    fn bird_texture(&mut self) -> Option<u32> {
        let mut cache = self.ui.cache_mut();
        if let Some(t) = cache.texture_file(view::GROUP, BIRD) {
            return Some(t.id);
        }
        let src = cache.texture_file(view::GROUP, "Lyt_E_ScrollIcon_04")?;
        let m = cache.texture_data(src.id)?.clone();
        let rgba = match m.format {
            mmk_assets::MtexFormat::Rgba8 => m.data.clone(),
            mmk_assets::MtexFormat::Bc7 => mmk_formats::bc7::decode(&m.data, m.width, m.height),
            _ => return None,
        };
        let mut grey = rgba;
        for px in grey.as_chunks_mut::<4>().0 {
            let l = 0.3 * px[0] as f32 + 0.59 * px[1] as f32 + 0.11 * px[2] as f32;
            // light grey body, darker wing, the outline kept white
            let v = (110.0 + l * 0.42).min(250.0) as u8;
            px[0] = v;
            px[1] = v;
            px[2] = (v as u16 + 10).min(255) as u8;
        }
        let tex = mmk_assets::Mtex {
            format: mmk_assets::MtexFormat::Rgba8,
            variant: mmk_assets::MtexVariant::Srgb,
            width: m.width,
            height: m.height,
            channel_select: [2, 3, 4, 5],
            data: grey,
        };
        Some(cache.add_texture_file(view::GROUP, BIRD, tex).id)
    }

    // ------------------------------------------------------------ drawing

    /// The band, the path, the birds and the speed squares over the course.
    pub(super) fn scroll_quads(&self, q: &mut Vec<Quad>) {
        let s = super::SCALE;
        let a = &self.editor.course.main.header;
        // the lit band: where the camera's centre can go (the area less half a view all round)
        let [bx0, by0] = self.to_layout(
            a.boundary_left as f32 + VIEW_W / 2.0,
            a.boundary_bottom as f32 + VIEW_H / 2.0,
        );
        let [bx1, by1] = self.to_layout(
            a.boundary_right as f32 - VIEW_W / 2.0,
            a.boundary_top as f32 - VIEW_H / 2.0,
        );
        let (bx0, by0, bx1, by1) = (
            bx0.clamp(-640.0, 640.0),
            by0.clamp(-360.0, 360.0),
            bx1.clamp(-640.0, 640.0),
            by1.clamp(-360.0, 360.0),
        );
        let dim: Rgba = [0, 0, 0, 96];
        shapes::rect(q, -640.0, -360.0, 640.0, by0, dim);
        shapes::rect(q, -640.0, by1, 640.0, 360.0, dim);
        shapes::rect(q, -640.0, by0, bx0, by1, dim);
        shapes::rect(q, bx1, by0, 640.0, by1, dim);
        shapes::rect(q, bx0, by0, bx1, by1, [255, 255, 255, 40]);
        let pts = self.scroll_points();
        let dark: Rgba = [96, 30, 24, 255];
        let white: Rgba = [255, 255, 255, 255];
        // the legs: dots, and an arrow on every tile
        for w in pts.windows(2) {
            let a = self.to_layout(w[0].x + SQUARE_DX, w[0].y);
            let b = self.to_layout(w[1].x, w[1].y);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1.0 {
                continue;
            }
            let u = [dx / len, dy / len];
            let tile = 16.0 * s;
            let start = 9.0 * s;
            let mut d = start;
            while d < len - 6.0 * s {
                let c = [a[0] + u[0] * d, a[1] + u[1] * d];
                if ((d - start) / tile).fract() < 0.01 {
                    arrows(q, c, u, w[0].speed, s);
                } else {
                    shapes::disc(q, c, 0.75 * s, dark);
                    shapes::disc(q, c, 0.5 * s, white);
                }
                d += tile / 4.0;
            }
        }
        // the points: the bird (S, the node's number, G) and the speed square
        let bird = self
            .ui
            .cache_mut()
            .texture_file(view::GROUP, BIRD)
            .map(|t| t.id);
        let mut number = 0;
        for pt in &pts {
            let c = self.to_layout(pt.x, pt.y);
            let label = match pt.kind {
                ScrollKind::Start => "S".to_string(),
                ScrollKind::End => "G".to_string(),
                ScrollKind::Node(_) => {
                    number += 1;
                    number.to_string()
                }
            };
            let (w, h) = (17.0 * s, 17.0 * s);
            let at = [c[0] - 6.0 * s, c[1] + 1.5 * s];
            match bird {
                Some(t) => q.push(view::quad(
                    [at[0] - w, at[1] - h, at[0] + w, at[1] + h],
                    t,
                    [0.0, 0.0, 1.0, 1.0],
                    [255; 4],
                )),
                None => {
                    shapes::disc(q, at, 8.0 * s, white);
                    shapes::disc(q, at, 7.0 * s, [150, 150, 160, 255]);
                }
            }
            shapes::text(
                q,
                [at[0] - 1.0 * s, at[1] - 3.0 * s],
                &label,
                1.3 * s,
                [60, 60, 66, 255],
            );
            if pt.kind != ScrollKind::End {
                speed_square(q, self.to_layout(pt.x + SQUARE_DX, pt.y), pt.speed, s);
            }
        }
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (a[0] + dx * t - p[0], a[1] + dy * t - p[1]);
    (cx * cx + cy * cy).sqrt()
}

/// A speed's colours: (fill, glyph).
fn speed_colors(speed: u8) -> (Rgba, Rgba) {
    match speed {
        0 => ([255, 204, 0, 255], [70, 26, 20, 255]),
        1 => ([240, 86, 86, 255], [96, 20, 20, 255]),
        _ => ([70, 140, 250, 255], [16, 40, 110, 255]),
    }
}

/// `n` play triangles of `size` pointing along `u`, `step` apart, centred on `c`.
fn play_glyph(
    q: &mut Vec<Quad>,
    c: [f32; 2],
    u: [f32; 2],
    n: u8,
    size: f32,
    step: f32,
    color: Rgba,
) {
    let v = [-u[1], u[0]];
    for k in 0..n {
        let off = (k as f32 - (n as f32 - 1.0) / 2.0) * step;
        let o = [c[0] + u[0] * off, c[1] + u[1] * off];
        let p = |a: f32, b: f32| [o[0] + u[0] * a + v[0] * b, o[1] + u[1] * a + v[1] * b];
        shapes::tri(
            q,
            p(-size / 2.0, size / 2.0),
            p(-size / 2.0, -size / 2.0),
            p(size / 2.0, 0.0),
            color,
        );
    }
}

/// The leg's arrow mark of a speed: ▶ yellow, ▶▶ red, ▶▶▶ blue, outlined.
fn arrows(q: &mut Vec<Quad>, c: [f32; 2], u: [f32; 2], speed: u8, s: f32) {
    let (fill, _) = speed_colors(speed);
    let n = speed.min(2) + 1;
    let step = 3.6 * s;
    play_glyph(q, c, u, n, 6.4 * s, step, [96, 30, 24, 255]);
    play_glyph(q, c, u, n, 4.6 * s, step, fill);
}

/// A point's speed square: a dark-edged rounded square in the speed's colour with its ▶.
fn speed_square(q: &mut Vec<Quad>, c: [f32; 2], speed: u8, s: f32) {
    let (fill, glyph) = speed_colors(speed);
    let h = 7.5 * s;
    shapes::rounded(
        q,
        c[0] - h,
        c[1] - h,
        c[0] + h,
        c[1] + h,
        1.6 * s,
        [96, 30, 24, 255],
    );
    let h2 = h - 1.2 * s;
    shapes::rounded(
        q,
        c[0] - h2,
        c[1] - h2,
        c[0] + h2,
        c[1] + h2,
        1.0 * s,
        [255, 255, 255, 255],
    );
    let h3 = h2 - 0.9 * s;
    shapes::rect(q, c[0] - h3, c[1] - h3, c[0] + h3, c[1] + h3, fill);
    play_glyph(q, c, [1.0, 0.0], speed.min(2) + 1, 4.6 * s, 3.4 * s, glyph);
}
