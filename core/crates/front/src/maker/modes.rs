//! What the Water Level and Custom Scroll modes share (research/specs/maker.md C13): the left
//! column's two mode buttons, the mode banner `L_ModeGuide_00`, the tick that gives presses to
//! the mode's handles, and the one-time tips.
//!
//! The buttons (`Edit_WaterLevelBtn_00`, `Edit_ScrollEditBtn_00`) [obs data]: `Invalid` greys
//! them (no liquid in the theme; autoscroll not custom), `Check` shows them chosen (their
//! looping picture, `ButtonLoop`), the water button's `Type` picks water / poison / lava. The
//! banner (`Edit_ModeGuide_00`): `InMode` brings the mode's frame round the screen and its name
//! (`T_Mode_00`, `TypeMode` its colour), `InLayer` the icon (`TypeLayer`: 0 sound, 1 scroll,
//! 2 water, 3 lava / poison) [obs 0x71018bf990: the animation states; inf: which the game
//! uses for which mode].
//!
//! In a mode the palette, the right column and the clapperboard stay; a press on any of them
//! (or on the mode's button again, or back) leaves the mode first [inf]. Presses over the
//! course go to the mode's handles; undo / redo work as in editing.

use super::help::{HelpTip, Tip};
use super::{MakerFlow, MakerInput, Mode, PAGE, TAP_AFTER, keys};
use crate::host::Host;
use crate::menu::Buttons;
use crate::sound::Event;

const LEFT: &str = "L_LeftParts_00";
const GUIDE: &str = "L_ModeGuide_00";
/// Where the banner's name and icon go (their panes' x in their parents): the name centred at
/// about x −250, the icon at −347 [obs: the reference frames].
const MODE_TEXT_X: f32 = 39.0;
const LAYER_ICON_X: f32 = -72.4;

impl MakerFlow {
    /// Leave the Water Level / Custom Scroll mode (presses in progress end as they are).
    pub(super) fn leave_mode(&mut self) {
        if !matches!(self.mode, Mode::Water | Mode::Scroll) {
            return;
        }
        if self.water.balloon.is_some() {
            self.close_speed_balloon();
        }
        if self.water.drag.is_some() {
            self.water_release();
        }
        if self.scroll.drag.is_some() {
            self.scroll_release();
        }
        self.mode = Mode::Edit;
        if let Some(p) = &mut self.page {
            for a in ["InMode", "InLayer"] {
                p.stop(&format!("{GUIDE}/{a}"));
            }
            p.play(&format!("{GUIDE}/OutMode"), self.tick);
            p.play(&format!("{GUIDE}/OutLayer"), self.tick);
        }
        self.update_mode_buttons();
    }

    /// The banner: the mode's name (message `label`) and icon (`TypeLayer` frame `layer`).
    pub(super) fn show_mode_guide(&mut self, label: &str, layer: u8) {
        let text = self.ui.message(PAGE, label);
        let tick = self.tick;
        let Some(p) = &mut self.page else { return };
        p.show(GUIDE, true);
        for a in ["OutMode", "OutLayer"] {
            p.stop(&format!("{GUIDE}/{a}"));
        }
        if let Some(t) = &text {
            p.text(&format!("{GUIDE}/T_Mode_00"), t);
        }
        // the frame's colour: the water level's and custom scroll's (TypeMode state 1)
        p.hold(&format!("{GUIDE}/TypeMode"), 1.0);
        p.hold(&format!("{GUIDE}/TypeLayer"), layer as f32);
        p.play(&format!("{GUIDE}/InMode"), tick);
        p.play(&format!("{GUIDE}/InLayer"), tick);
        // as the reference frames show it: the name on the mode's base with the icon on its
        // left; no frame round the screen, no second base [obs: the frames; inf: how the game
        // composes the two groups]
        p.show(&format!("{GUIDE}/W_Frame_00"), false);
        p.show(&format!("{GUIDE}/W_LayerBase_00"), false);
        p.show(&format!("{GUIDE}/T_Layer_00"), false);
        p.set_translate(&format!("{GUIDE}/T_Mode_00"), [MODE_TEXT_X, 225.0]);
        p.set_translate(&format!("{GUIDE}/N_LayerPos_00"), [LAYER_ICON_X, 2.0]);
    }

    /// The two mode buttons as the course and the mode are.
    pub(super) fn update_mode_buttons(&mut self) {
        let water = self.water_available();
        let scroll = self.scroll_available();
        let kind = self.editor.liquid_kind();
        let mode = self.mode;
        let tick = self.tick;
        let Some(p) = &mut self.page else { return };
        let b = format!("{LEFT}/L_WaterLevelBtn_00");
        // water, poison, lava [obs data: the Type frames' boat pictures]
        let ty = match kind {
            Some(3) => 1.0,
            Some(2) => 2.0,
            _ => 0.0,
        };
        p.hold(&format!("{b}/Type"), ty);
        for (b, valid, on) in [
            (b, water, mode == Mode::Water),
            (
                format!("{LEFT}/L_ScrollEditBtn_00"),
                scroll,
                mode == Mode::Scroll,
            ),
        ] {
            if valid {
                p.stop(&format!("{b}/Invalid"));
            } else {
                p.hold_end(&format!("{b}/Invalid"));
            }
            p.hold(&format!("{b}/Check"), if on { 1.0 } else { 0.0 });
            let looping = format!("{b}/ButtonLoop");
            if on {
                if !p.is_playing(&looping) {
                    p.play_loop(&looping, tick);
                }
            } else {
                p.stop(&looping);
            }
        }
    }

    /// Show tip `t` unless it was shown before.
    pub(super) fn show_tip(&mut self, host: &mut dyn Host, t: Tip) {
        if host.load_value(t.key()).as_deref() == Some("1") {
            return;
        }
        host.store_value(t.key(), "1");
        self.help = Some(HelpTip::new(&self.ui, t));
        self.mode = Mode::Help;
    }

    /// A tick in the Water Level or Custom Scroll mode.
    pub(super) fn mode_tick(
        &mut self,
        input: &MakerInput,
        pressed: Buttons,
        keys: u16,
        start_pressed: bool,
        cx: &mut crate::Cx,
    ) {
        let place_held = input.ui.held.has(Buttons::CONFIRM) || input.down;
        let pointer_pressed = input.down && !self.down_was;
        if input.pointer.is_some() {
            self.pointer_tick = self.tick;
        }
        let tap = input
            .ui
            .click
            .filter(|_| pointer_pressed || self.tick - self.pointer_tick > TAP_AFTER);
        let click = tap.or(if pointer_pressed { input.pointer } else { None });
        // the speed balloon has the presses while it is open
        if self.water.balloon.is_some() {
            self.speed_balloon_tick(pressed, place_held, click);
            return;
        }
        if keys & keys::UNDO != 0 {
            self.undo();
        }
        if keys & keys::REDO != 0 && self.editor.redo() {
            self.changed();
        }
        // what the editing does elsewhere leaves the mode first
        let busy = self.water.drag.is_some() || self.scroll.drag.is_some();
        if !busy
            && (start_pressed
                || pressed.has(Buttons::BACK)
                || keys & (keys::PARTS | keys::AREA | keys::PLAY_HERE | keys::NEXT | keys::PREV)
                    != 0)
        {
            self.ui.sound_back(PAGE, "normal");
            self.leave_mode();
            if start_pressed {
                self.open_menu();
            }
            return;
        }
        // the mode left when the course changed under it (undo, a theme change)
        if (self.mode == Mode::Water && !self.water_available())
            || (self.mode == Mode::Scroll && !self.scroll_available())
        {
            self.leave_mode();
            return;
        }
        // a press on a water-level handle first (they stand over the scroll bar along the
        // bottom), then the page's buttons
        let erase = self.eraser || input.alt || (input.ui.held.has(Buttons::BACK) && !input.start);
        if !busy
            && self.mode == Mode::Water
            && let Some(p) = click
            && input.pointer.is_some()
            && self.mode_press(p, erase)
        {
            self.glove = p;
            return;
        }
        if !busy
            && let Some(p) = click
            && self.mode_page_click(p, cx)
        {
            self.down_was = true;
            return;
        }
        // the cursor: the pointer, else the d-pad (a tile a step)
        if let Some(p) = input.pointer {
            self.glove = p;
            self.cur = self.tile_at(p);
        } else if let Some(d) = pressed.direction() {
            let (w, h) = self.editor.size();
            self.cur.0 = (self.cur.0 + d[0] as i32).clamp(0, w - 1);
            self.cur.1 = (self.cur.1 + d[1] as i32).clamp(0, h - 1);
            self.follow_cursor();
            self.glove = self.tile_center(self.cur);
        }
        let press =
            pressed.has(Buttons::CONFIRM) || pointer_pressed || (input.alt && !self.alt_was);
        // a touch tap without a pointer: a press and its release at once
        if input.pointer.is_none()
            && let Some(p) = tap
            && !busy
        {
            self.glove = p;
            if self.mode_press(p, erase) {
                self.mode_hold(p, cx);
                self.mode_release();
            }
            return;
        }
        let at = self.glove;
        if !busy {
            if press {
                self.mode_press(at, erase);
            }
            return;
        }
        if place_held {
            self.mode_hold(at, cx);
        } else {
            self.mode_release();
        }
    }

    fn mode_press(&mut self, p: [f32; 2], erase: bool) -> bool {
        match self.mode {
            Mode::Water if !erase => self.water_press(p),
            Mode::Scroll => self.scroll_press(p, erase),
            _ => false,
        }
    }

    fn mode_hold(&mut self, p: [f32; 2], cx: &mut crate::Cx) {
        match self.mode {
            Mode::Water => self.water_hold(p),
            Mode::Scroll => self.scroll_hold(p, cx),
            _ => {}
        }
    }

    fn mode_release(&mut self) {
        match self.mode {
            Mode::Water => self.water_release(),
            Mode::Scroll => self.scroll_release(),
            _ => {}
        }
    }

    /// A click on the page in a mode: the mode's own button leaves it, the other mode's button
    /// switches, any other button leaves the mode and does its work (true: a button took it).
    fn mode_page_click(&mut self, p: [f32; 2], cx: &mut crate::Cx) -> bool {
        let Some(page) = &mut self.page else {
            return false;
        };
        let own = [
            format!("{LEFT}/L_WaterLevelBtn_00/B_Btn_00"),
            format!("{LEFT}/L_ScrollEditBtn_00/B_Btn_00"),
        ];
        if let Some(k) = crate::menu::hit(page, self.tick, &own, p) {
            let pane = own[k].trim_end_matches("/B_Btn_00").to_string();
            let water = k == 0;
            if water == (self.mode == Mode::Water) {
                self.ui.sound_button(PAGE, &pane, Event::Decide);
                self.leave_mode();
            } else if (water && self.water_available()) || (!water && self.scroll_available()) {
                self.ui.sound_button(PAGE, &pane, Event::Decide);
                if water {
                    self.enter_water();
                } else {
                    self.enter_scroll();
                }
            } else {
                self.sound(7, "invalid");
            }
            return true;
        }
        let mode = self.mode;
        self.mode = Mode::Edit;
        if self.page_click(p, cx) {
            // the mode ends under what the button did or opened
            let now = self.mode;
            self.mode = mode;
            self.leave_mode();
            self.mode = now;
            self.update_mode_buttons();
            return true;
        }
        self.mode = mode;
        false
    }
}
