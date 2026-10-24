//! The Custom Scroll mode (`L_LeftParts_00/L_ScrollEditBtn_00`, research/specs/maker.md C13):
//! the path the camera follows with autoscroll set to custom (main area only).
//!
//! The path runs from the start unit (a grey bird marked "S") through the numbered nodes
//! (part 89, EditAutoScrollCamera; up to 10, yellow birds) to the end unit ("G"); each point
//! has a speed square to its right for the leg after it, and the legs carry dotted marks in
//! that speed's look (slow ▶ yellow, normal ▶▶ red, fast ▶▶▶ blue). The band the camera can
//! take is lit, the rest of the course dims [obs: the reference frames;
//! 89-autoscroll-camera.md]. The markers stand where the points are before the camera's clamp
//! (the start at x 184 on a new course) [obs: the frames]; the pictures are the game's
//! (`Edit_camera`, `Edit_autoscroll`, the icon sheet's squares, [`super::art`]).
//!
//! From the game's edit actor [obs]:
//! - the models: "camera_model" (`Edit_camera`) with the animation LocalizationS on the start
//!   unit (node number 0), LocalizationG on the end (11), "count" on the others (0x7100eb2070);
//!   drawn 8 px left of the point, the speed square (icon 0x34 / 0x35 / 0x36) 16 px right of
//!   it, only in the mode (0x7100eb27e0); the marks a tile apart along each leg (0x7100fd5c10);
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

use super::art::{self, Rgba};
use super::{MakerFlow, Mode};

/// The edit sounds' part: the nodes' own user (`<style>_EditAutoScrollCamera`).
pub const SOUND_PART: i16 = 89;
/// The speed square sits this far right of its point (world px) [obs 0x7100eb30e0].
pub const SQUARE_DX: f32 = 16.0;
/// The bird's model stands 8 px left of its point, on a 39 × 48 quad [obs 0x7100eb27e0,
/// `camera_model`'s shape].
const BIRD_DX: f32 = -8.0;
const BIRD_HALF: [f32; 2] = [19.5, 24.0];
/// The path marks' pictures by the leg's speed (`Edit_autoscroll_01` / `05` / `09`; the one
/// before them, without the dots ahead, for a leg's last mark) [obs 0x7100fd5c10: the table
/// 0x71023c6fa0], and the distances to the next bird that end a leg (40) and make its last
/// mark (58).
const MARKS: [usize; 3] = [1, 5, 9];
const STOP: f32 = 40.0;
const LAST: f32 = 58.0;
/// The lit band reaches past the view-centre range by this much (world px) [inf: the frames].
const BAND_X: f32 = 32.0;
const BAND_Y: f32 = 40.0;

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
        self.show_mode_guide("Mode_Disp_ManualScrl", 1);
        self.update_mode_buttons();
    }

    /// The path's points as the camera follows them (world px).
    pub fn scroll_points(&self) -> Vec<ScrollPoint> {
        self.editor.scroll_path()
    }

    /// Where the path's markers stand (world px): the points before the camera's clamp.
    pub fn scroll_marks(&self) -> Vec<ScrollPoint> {
        self.editor.scroll_marks()
    }

    /// A press at layout point `p` (`erase`: the eraser or the erase button): on a speed
    /// square, a node or a leg it is taken (true).
    pub(super) fn scroll_press(&mut self, p: [f32; 2], erase: bool) -> bool {
        let [x, y] = self.to_world(p);
        let pts = self.scroll_marks();
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

    // ------------------------------------------------------------ drawing

    /// The band, the path, the birds and the speed squares over the course (`in_mode`: the
    /// Custom Scroll mode; outside it only the birds, faint) [obs 0x7100eb27e0, 0x7100fd5c10].
    pub(super) fn scroll_quads(&self, q: &mut Vec<Quad>, in_mode: bool) {
        let pts = self.scroll_marks();
        if !in_mode {
            self.birds(q, &pts, 102);
            return;
        }
        // the lit band (where the path can take the camera: the view-centre range, reaching
        // 2 tiles further across and 2.5 further up and down [inf: its left and bottom edges
        // in the frames]), the rest of the course dims
        let a = &self.editor.course.main.header;
        let [bx0, by0] = self.to_layout(
            a.boundary_left as f32 + VIEW_W / 2.0 - BAND_X,
            a.boundary_bottom as f32 + VIEW_H / 2.0 - BAND_Y,
        );
        let [bx1, by1] = self.to_layout(
            a.boundary_right as f32 - VIEW_W / 2.0 + BAND_X,
            a.boundary_top as f32 - VIEW_H / 2.0 + BAND_Y,
        );
        let (bx0, by0, bx1, by1) = (
            bx0.clamp(-640.0, 640.0),
            by0.clamp(-360.0, 360.0),
            bx1.clamp(-640.0, 640.0),
            by1.clamp(-360.0, 360.0),
        );
        let dim: Rgba = [0, 0, 0, 96];
        art::rect(q, -640.0, -360.0, 640.0, by0, dim);
        art::rect(q, -640.0, by1, 640.0, 360.0, dim);
        art::rect(q, -640.0, by0, bx0, by1, dim);
        art::rect(q, bx1, by0, 640.0, by1, dim);
        art::rect(q, bx0, by0, bx1, by1, [255, 255, 255, 40]);
        // the legs: a mark every tile from the square on, the last one (within 58 px of the
        // next bird) without the dots ahead; the leg ends within 40 px of it [obs 0x7100fd5c10]
        for w in pts.windows(2) {
            let from = [w[0].x + SQUARE_DX, w[0].y];
            let to = [w[1].x + BIRD_DX, w[1].y];
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < STOP {
                continue;
            }
            let u = [dx / len, dy / len];
            let turn = art::angle_of(u);
            let speed = w[0].speed.min(2) as usize;
            let (mut at, mut left) = (from, len);
            loop {
                let last = left < LAST;
                at = [at[0] + u[0] * 16.0, at[1] + u[1] * 16.0];
                let mark = MARKS[speed] - last as usize;
                let p = art::Place::new(at, [8.0, 8.0]).turned(turn);
                self.model(
                    q,
                    &format!("Edit_autoscroll_{mark:02}"),
                    p,
                    255,
                    [255, 204, 0, 255],
                );
                left = ((to[0] - at[0]).powi(2) + (to[1] - at[1]).powi(2)).sqrt();
                if last || left < STOP {
                    break;
                }
            }
        }
        self.birds(q, &pts, 255);
        // the speed squares (not the end's)
        for pt in &pts {
            if pt.kind != ScrollKind::End {
                let id = match pt.speed {
                    0 => art::icon::SQUARE_SLOW,
                    1 => art::icon::SQUARE_NORMAL,
                    _ => art::icon::SQUARE_FAST,
                };
                self.icon(q, id, [pt.x + SQUARE_DX, pt.y], 1.0, 0.0);
            }
        }
    }

    /// The birds (`Edit_camera`'s `camera_model`, 8 px left of their point): S on the start
    /// (`LocalizationS`), the node's number on each node (`count`, numbered left to right), G
    /// on the end (`LocalizationG`); at `alpha` (const_color0's: 1 in the mode, 0.4 outside)
    /// [obs 0x7100eb2070, 0x7100eb27e0].
    fn birds(&self, q: &mut Vec<Quad>, pts: &[ScrollPoint], alpha: u8) {
        let mut number = 0;
        for pt in pts {
            let frame = match pt.kind {
                ScrollKind::Start => 0,
                ScrollKind::End => 11,
                ScrollKind::Node(_) => {
                    number += 1;
                    number.min(10)
                }
            };
            let p = art::Place::new([pt.x + BIRD_DX, pt.y], BIRD_HALF);
            let name = format!("Edit_camera.{frame:02}");
            self.model(q, &name, p, alpha, [150, 150, 160, 255]);
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
