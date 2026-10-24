//! The Water Level mode (`L_LeftParts_00/L_WaterLevelBtn_00`, research/specs/maker.md C13):
//! the height of the area's water, lava or poison and how it moves (area header +4 end height,
//! +5 mode, +6 speed, +7 start height).
//!
//! The game's edit actor (constructor 0x7100eff460: models `Edit_WaterLevelLine`
//! WaterLevelBtn / WaterLevelLine, `Edit_dragBtn` DragArrow) [obs]:
//! - draw 0x7100effd00, in themes 2 and 9 only: the handles stand at 31/48 of the view across
//!   (0.6458, `DAT_71023191a4`); the start line's pill there, the end's round button and the
//!   path 16 px to its left. The button's icon: ↕ when the mode is 2, else ↑ / ↓ by where the
//!   end is; the path a speed mark (1 / 2 / 3) on every tile between the lines;
//! - hit boxes 0x7100f013d0: the button ±8 px round (anchor − 16, end), the pill ±24 × ±8 round
//!   (anchor, start);
//! - press 0x7100f00710 ("touch", an undo step); drag 0x7100f008a0: the button sets the end
//!   height, the pill the start height, and the end goes with the start when they were together
//!   or the start passes the end; heights 0 (1 in 3D World) … 24 (167 vertical);
//! - 0x7100f01770: lines coming together save the mode and speed and make the liquid still
//!   (mode 0, speed 0); lines parting give them back (1 and 1 at first);
//! - release 0x7100f01540: a tap on the button (no change, the lines apart, no balloon) turns
//!   mode 1 ↔ 2 ("change_direction"), else "set"; held still more than 15 frames the button
//!   opens the speed balloon (slow / medium / fast, `Lyt_E_WaterSpeed_00…02`).
//!
//! The lines are drawn as wavy lines (yellow while the liquid stays, white when it moves), the
//! handles and the path as flat shapes ([`super::shapes`]) [obs: the reference frames].

use mmk_editor::settings::Liquid;
use mmk_ui::Quad;

use super::shapes::{self, Rgba};
use super::{MakerFlow, Mode, VIEW_W, view};
use crate::layout::Layout;
use crate::menu::{Buttons, hit};
use crate::sound::Event;

/// The handles' place across the view [obs 0x7100effd00].
pub const ANCHOR: f32 = 31.0 / 48.0;
/// A press held this long without a change opens the speed balloon (more than 15 frames)
/// [obs 0x7100f008a0: the press timer +0x364 > 0xf].
pub const HOLD: i32 = 16;
/// The edit sounds' part (the water-level actor's own sound user is not resolved: the
/// ground's stands in [inf]).
const SOUND_PART: i16 = 7;
/// The speed balloon (the attribute balloon's layout with the speed pictures) [inf: its frame
/// and buttons look as that balloon's in the reference frames].
const BALLOON: &str = super::attr::LAYOUT;

/// The liquid's surface at height `h` (world px) [themes.md §3: h × 16 + 12].
pub fn line_y(h: u8) -> f32 {
    h as f32 * 16.0 + 12.0
}

/// What a press holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// The round button on the end line (the end height).
    Target,
    /// The pill on the start line (the start height).
    Level,
}

/// The mode's state.
#[derive(Default)]
pub struct LiquidEdit {
    pub drag: Option<Handle>,
    /// Ticks the press has lasted.
    pub held: i32,
    /// The press changed a height.
    pub moved: bool,
    /// The end goes with the start (they were together, or the start passed it).
    follow: bool,
    /// Mode and speed while the lines are together (given back when they part).
    pub saved: Option<(u8, u8)>,
    pub balloon: Option<SpeedBalloon>,
}

/// The speed balloon over the round button.
pub struct SpeedBalloon {
    pub layout: Option<Layout>,
    /// Button with the cursor: 0 slow, 1 medium, 2 fast.
    pub sel: usize,
    /// The press that opened it is still down.
    pub press_held: bool,
}

impl MakerFlow {
    /// The area has a liquid whose level can be set (castle, forest).
    pub fn water_available(&self) -> bool {
        self.editor.liquid_kind().is_some()
    }

    /// The Water Level mode is on.
    pub fn water_mode(&self) -> bool {
        self.mode == Mode::Water
    }

    /// The handles' x (world px).
    pub fn liquid_anchor(&self) -> f32 {
        self.cam[0] + VIEW_W * ANCHOR
    }

    pub(super) fn enter_water(&mut self) {
        if !self.water_available() {
            return;
        }
        self.end_drag_if_any();
        self.leave_mode();
        self.mode = Mode::Water;
        self.water = LiquidEdit::default();
        // which text: water, poison or lava (Mode_Disp_WaterLevel_00 / 01 / 02) [obs data]
        let text = match self.editor.liquid_kind() {
            Some(3) => "Mode_Disp_WaterLevel_01",
            Some(2) => "Mode_Disp_WaterLevel_02",
            _ => "Mode_Disp_WaterLevel_00",
        };
        let layer = if self.editor.liquid_kind() == Some(1) {
            2
        } else {
            3
        };
        self.show_mode_guide(text, layer);
        self.update_mode_buttons();
    }

    /// A press at layout point `p` in the mode: on a handle it takes it (true).
    pub(super) fn water_press(&mut self, p: [f32; 2]) -> bool {
        let [x, y] = self.to_world(p);
        let l = self.editor.liquid();
        let ax = self.liquid_anchor();
        let handle = if (x - (ax - 16.0)).abs() <= 8.0 && (y - line_y(l.end)).abs() <= 8.0 {
            Handle::Target
        } else if (x - ax).abs() <= 24.0 && (y - line_y(l.start)).abs() <= 8.0 {
            Handle::Level
        } else {
            return false;
        };
        self.editor.checkpoint();
        self.water.drag = Some(handle);
        self.water.held = 0;
        self.water.moved = false;
        self.water.follow = l.start == l.end;
        self.sound(SOUND_PART, "touch");
        true
    }

    /// The press goes on at `p`: the handle follows it; held still on the button, the speed
    /// balloon opens.
    pub(super) fn water_hold(&mut self, p: [f32; 2]) {
        let Some(handle) = self.water.drag else {
            return;
        };
        self.water.held += 1;
        let [_, y] = self.to_world(p);
        let (lo, hi) = self.editor.liquid_range();
        let h = ((y - 12.0) / 16.0).round().clamp(lo as f32, hi as f32) as u8;
        let mut l = self.editor.liquid();
        let apart = l.start != l.end;
        match handle {
            Handle::Target if h != l.end => l.end = h,
            Handle::Level if h != l.start => {
                // the end goes with the start once the start reaches or passes it
                if (l.start..=h).contains(&l.end) || (h..=l.start).contains(&l.end) {
                    self.water.follow = true;
                }
                l.start = h;
                if self.water.follow {
                    l.end = h;
                }
            }
            _ => {
                if handle == Handle::Target
                    && !self.water.moved
                    && self.water.held >= HOLD
                    && apart
                    && self.water.balloon.is_none()
                {
                    self.open_speed_balloon();
                }
                return;
            }
        }
        self.set_liquid_keeping(l, apart);
        self.water.moved = true;
        self.sound(SOUND_PART, "move");
    }

    /// Heights changed (`was_apart`: the lines were apart before): coming together the mode
    /// and speed are kept aside and the liquid stays; parting they come back.
    fn set_liquid_keeping(&mut self, mut l: Liquid, was_apart: bool) {
        let apart = l.start != l.end;
        if was_apart && !apart {
            self.water.saved = Some((l.mode, l.speed));
            l.mode = 0;
            l.speed = 0;
        } else if !was_apart && apart {
            let (mode, speed) = self.water.saved.take().unwrap_or((1, 1));
            l.mode = mode.max(1);
            l.speed = speed.max(1);
        }
        self.editor.set_liquid(l);
        self.changed();
    }

    /// The press ends: a tap on the button turns rises ↔ rises and falls.
    pub(super) fn water_release(&mut self) {
        let Some(handle) = self.water.drag.take() else {
            return;
        };
        let l = self.editor.liquid();
        if handle == Handle::Target
            && !self.water.moved
            && l.start != l.end
            && self.water.balloon.is_none()
        {
            let mode = if l.mode == 1 { 2 } else { 1 };
            self.editor.set_liquid(Liquid {
                mode,
                speed: l.speed.max(1),
                ..l
            });
            self.changed();
            self.sound(SOUND_PART, "change_direction");
            return;
        }
        if self.water.moved {
            self.sound(SOUND_PART, "set");
        } else if self.water.balloon.is_none() {
            self.editor.discard_checkpoint();
        }
    }

    // ------------------------------------------------------------ the speed balloon

    fn open_speed_balloon(&mut self) {
        let l = self.editor.liquid();
        self.water.balloon = Some(SpeedBalloon {
            layout: self.ui.layout(BALLOON),
            sel: (l.speed.clamp(1, 3) - 1) as usize,
            press_held: true,
        });
        self.ui.sound_key(BALLOON, "In", &[]);
        self.layout_speed_balloon(true);
    }

    /// Lay the balloon out: three buttons with the speed pictures, the current speed checked,
    /// from the button down and to the left [obs: the reference frames], turned away from the
    /// screen's left and bottom edges.
    pub(super) fn layout_speed_balloon(&mut self, opening: bool) {
        let tick = self.tick;
        let l = self.editor.liquid();
        let at = self.to_layout(self.liquid_anchor() - 16.0, line_y(l.end));
        let Some(b) = &mut self.water.balloon else {
            return;
        };
        let sel = b.sel;
        let Some(lay) = &mut b.layout else { return };
        lay.set_translate("N_InOut_00", at);
        lay.hold("Type", 2.0);
        lay.hold("Direction", 2.0);
        for k in 0..super::attr::BUTTONS {
            lay.clear_pane(&format!("N_Btn_{k:02}"));
        }
        lay.apply(tick);
        let (right, above) = match lay.rect("W_Frame_00") {
            Some(r) => (
                r.center[0] - r.size[0] / 2.0 < -600.0,
                r.center[1] - r.size[1] / 2.0 < -330.0,
            ),
            None => (false, false),
        };
        let direction = match (right, above) {
            (false, false) => 2.0,
            (true, false) => 3.0,
            (false, true) => 1.0,
            (true, true) => 0.0,
        };
        lay.hold("Direction", direction);
        // the directions to the left mirror the buttons too: turned back
        let scale = [if right { 1.0 } else { -1.0 }, 1.0];
        if scale != [1.0, 1.0] {
            for k in 0..3 {
                lay.set_pane(
                    &format!("N_Btn_{k:02}"),
                    mmk_ui::PaneOverride {
                        scale: Some(scale),
                        ..Default::default()
                    },
                );
            }
        }
        for k in 0..super::attr::BUTTONS {
            let btn = super::attr::button(k);
            if k >= 3 {
                lay.show(&btn, false);
                continue;
            }
            lay.visible(&btn, None);
            let icon = format!("{btn}/P_Icon_00");
            lay.show(&icon, true);
            lay.image(&icon, view::GROUP, &format!("Lyt_E_WaterSpeed_{k:02}"));
            lay.show(&format!("{btn}/L_NewIcon_00"), false);
            lay.hold(&format!("{btn}/Type"), 0.0);
            let checked = k + 1 == l.speed as usize;
            lay.hold(&format!("{btn}/Check"), if checked { 1.0 } else { 0.0 });
            lay.hold(&format!("{btn}/Invalid"), 0.0);
            if k == sel {
                if opening {
                    lay.hold_end(&format!("{btn}/Select"));
                } else {
                    lay.play(&format!("{btn}/Select"), tick);
                }
            } else {
                lay.hold(&format!("{btn}/Select"), 0.0);
            }
        }
        if opening {
            lay.play("In", tick);
        }
    }

    /// A tick with the balloon open: a tap on a button sets that speed, a tap elsewhere or back
    /// closes it; left / right and confirm with the d-pad.
    pub(super) fn speed_balloon_tick(
        &mut self,
        pressed: Buttons,
        place_held: bool,
        click: Option<[f32; 2]>,
    ) {
        let tick = self.tick;
        let Some(b) = &mut self.water.balloon else {
            return;
        };
        if b.press_held {
            // the opening press: its release chooses nothing
            if !place_held {
                b.press_held = false;
                self.water_release();
            }
            return;
        }
        if let Some(p) = click {
            let panes: Vec<String> = (0..3)
                .map(|k| format!("{}/B_Btn_00", super::attr::button(k)))
                .collect();
            match b.layout.as_mut().and_then(|l| hit(l, tick, &panes, p)) {
                Some(k) => {
                    b.sel = k;
                    self.choose_speed();
                }
                None => self.close_speed_balloon(),
            }
            return;
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(BALLOON, "normal");
            self.close_speed_balloon();
            return;
        }
        if let Some(d) = pressed.direction()
            && d[0] != 0.0
        {
            let k = (b.sel as i32 + d[0].signum() as i32).rem_euclid(3) as usize;
            b.sel = k;
            self.ui
                .sound_button(BALLOON, &super::attr::button(k), Event::Select);
            self.layout_speed_balloon(false);
            return;
        }
        if pressed.has(Buttons::CONFIRM) {
            self.choose_speed();
        }
    }

    /// The selected speed goes to the liquid; the balloon closes.
    fn choose_speed(&mut self) {
        let Some(b) = &self.water.balloon else { return };
        let speed = b.sel as u8 + 1;
        let pane = super::attr::button(b.sel);
        let l = self.editor.liquid();
        self.ui.sound_button(BALLOON, &pane, Event::Decide);
        if l.speed != speed && l.start != l.end {
            self.editor.set_liquid(Liquid { speed, ..l });
            self.changed();
            self.sound(SOUND_PART, "change_speed");
            self.water.balloon = None;
            return;
        }
        self.close_speed_balloon();
    }

    /// Close the balloon; the press's undo step goes when nothing changed.
    pub(super) fn close_speed_balloon(&mut self) {
        if self.water.balloon.take().is_some() {
            self.editor.discard_checkpoint();
        }
    }

    /// The speed balloon is open.
    pub fn speed_balloon_open(&self) -> bool {
        self.water.balloon.is_some()
    }

    // ------------------------------------------------------------ drawing

    /// The lines, the pill, the path and the button over the course.
    pub(super) fn liquid_quads(&self, q: &mut Vec<Quad>) {
        let l = self.editor.liquid();
        let ax = self.liquid_anchor();
        let apart = l.start != l.end;
        let s = super::SCALE;
        let phase = self.to_layout(0.0, 0.0)[0];
        let dark: Rgba = [96, 30, 24, 255];
        let line = |q: &mut Vec<Quad>, h: u8, color: Rgba, double: bool| {
            let y = self.to_layout(0.0, line_y(h))[1];
            let (amp, period) = (1.6 * s, 16.0 * s);
            if double {
                for dy in [2.0 * s, -0.4 * s] {
                    shapes::wave(q, -640.0, 640.0, y + dy, amp, period, phase, 0.9 * s, dark);
                    shapes::wave(
                        q,
                        -640.0,
                        640.0,
                        y + dy,
                        amp,
                        period,
                        phase,
                        0.45 * s,
                        color,
                    );
                }
            } else {
                shapes::wave(q, -640.0, 640.0, y, amp, period, phase, 1.5 * s, dark);
                shapes::wave(q, -640.0, 640.0, y, amp, period, phase, 0.8 * s, color);
            }
        };
        let white: Rgba = [255, 255, 255, 255];
        let yellow: Rgba = [255, 204, 0, 255];
        if apart {
            line(q, l.start, white, true);
            line(q, l.end, white, true);
        } else {
            line(q, l.start, yellow, false);
        }
        // the path: a dotted line from the pill to the button with the speed marks on each
        // tile between
        let [px, y0] = self.to_layout(ax - 16.0, line_y(l.start));
        let [_, y1] = self.to_layout(ax - 16.0, line_y(l.end));
        if apart {
            let up = y1 > y0;
            let (lo, hi) = (y0.min(y1), y0.max(y1));
            let mut y = lo + 2.0 * s;
            while y < hi - 2.0 * s {
                shapes::disc(q, [px, y], 0.55 * s, dark);
                shapes::disc(q, [px, y], 0.35 * s, white);
                y += 2.0 * s;
            }
            let n = l.start.abs_diff(l.end);
            for k in 1..n {
                let y = y0 + (y1 - y0) * k as f32 / n as f32;
                speed_mark(q, [px, y], l.speed.clamp(1, 3), up, s);
            }
        }
        // the pill on the start line, its little circle where the path starts
        let [cx, cy] = self.to_layout(ax, line_y(l.start));
        let (hw, hh) = (24.0 * s, 8.0 * s);
        shapes::rounded(
            q,
            cx - hw - 4.0,
            cy - hh - 4.0,
            cx + hw + 4.0,
            cy + hh + 4.0,
            hh + 4.0,
            dark,
        );
        shapes::rounded(
            q,
            cx - hw - 1.5,
            cy - hh - 1.5,
            cx + hw + 1.5,
            cy + hh + 1.5,
            hh + 1.5,
            white,
        );
        shapes::rounded(q, cx - hw, cy - hh, cx + hw, cy + hh, hh, yellow);
        shapes::disc(q, [px, cy], 2.2 * s, dark);
        shapes::disc(q, [px, cy], 1.6 * s, white);
        shapes::disc(q, [px, cy], 1.0 * s, yellow);
        if !apart {
            return;
        }
        // the round button on the end line: ↑ / ↓ (rises or falls), ↕ (rises and falls)
        let c = [px, y1];
        shapes::disc(q, c, 9.5 * s, dark);
        shapes::disc(q, c, 7.5 * s, yellow);
        let icon: Rgba = [80, 30, 20, 255];
        let a = 4.2 * s;
        let arrow = |q: &mut Vec<Quad>, dir: f32| {
            shapes::seg(q, c, [c[0], c[1] + dir * a], 1.6 * s, icon);
            shapes::tri(
                q,
                [c[0] - 2.6 * s, c[1] + dir * (a - 2.2 * s)],
                [c[0] + 2.6 * s, c[1] + dir * (a - 2.2 * s)],
                [c[0], c[1] + dir * (a + 0.8 * s)],
                icon,
            );
        };
        if l.mode == 2 {
            arrow(q, 1.0);
            arrow(q, -1.0);
        } else {
            let dir = if l.end > l.start { 1.0 } else { -1.0 };
            shapes::seg(q, [c[0], c[1] - dir * a * 0.7], c, 1.6 * s, icon);
            arrow(q, dir);
        }
        // the drag arrows above and below the button while it is held (`Edit_dragBtn`)
        if self.water.drag == Some(Handle::Target) {
            for dir in [1.0f32, -1.0] {
                let b = [c[0], c[1] + dir * 11.0 * s];
                let t = [c[0], c[1] + dir * 32.0 * s];
                shapes::seg(q, b, t, 3.0 * s, dark);
                shapes::seg(q, b, t, 1.8 * s, yellow);
                let tip = [t[0], t[1] + dir * 3.0 * s];
                shapes::tri(
                    q,
                    [t[0] - 4.2 * s, t[1] - dir * 1.0 * s],
                    [t[0] + 4.2 * s, t[1] - dir * 1.0 * s],
                    [tip[0], tip[1] + dir * 1.0 * s],
                    dark,
                );
                shapes::tri(
                    q,
                    [t[0] - 3.2 * s, t[1] - dir * 0.2 * s],
                    [t[0] + 3.2 * s, t[1] - dir * 0.2 * s],
                    tip,
                    yellow,
                );
            }
        }
    }

    /// The speed balloon's layout, while open.
    pub(super) fn speed_balloon_layout(&mut self) -> Option<&mut Layout> {
        self.water.balloon.as_mut().and_then(|b| b.layout.as_mut())
    }
}

/// A speed mark on the path: one orange (slow), two red (medium) or three blue (fast)
/// triangles pointing the way the liquid goes [obs: `Lyt_E_WaterSpeed_00…02`, the reference
/// frames].
fn speed_mark(q: &mut Vec<Quad>, c: [f32; 2], speed: u8, up: bool, s: f32) {
    let fill: Rgba = match speed {
        1 => [248, 178, 92, 255],
        2 => [232, 32, 24, 255],
        _ => [24, 104, 240, 255],
    };
    let dir = if up { 1.0 } else { -1.0 };
    let n = speed as f32;
    for k in 0..speed {
        let y = c[1] + (k as f32 - (n - 1.0) / 2.0) * 1.6 * s * dir;
        let (w, h) = (2.6 * s, 1.7 * s);
        shapes::tri(
            q,
            [c[0] - w - 0.6 * s, y - h / 2.0 * dir - 0.4 * s * dir],
            [c[0] + w + 0.6 * s, y - h / 2.0 * dir - 0.4 * s * dir],
            [c[0], y + h / 2.0 * dir + 0.7 * s * dir],
            [96, 30, 24, 255],
        );
        shapes::tri(
            q,
            [c[0] - w, y - h / 2.0 * dir],
            [c[0] + w, y - h / 2.0 * dir],
            [c[0], y + h / 2.0 * dir],
            fill,
        );
    }
}
