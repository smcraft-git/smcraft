//! The attribute balloon over a placed part (`Edit_BalloonPartsChange_00`): wings, a parachute,
//! the big size and the part's forms. SMM2 has no Wing or Parachute part: holding a placed part
//! still for a moment (the edit part's 15-frame check) opens this balloon above it, and wings
//! and parachutes go on and come off there (research/specs/maker.md C10, private).
//!
//! The balloon's buttons are `L_Btn_00…10` (`Edit_BalloonPartsBtn_00`): the picture of the part
//! as the choice would make it, `Check` on the current form, `Invalid` when a part limit forbids
//! the change; the layout's `Type` frame n lays out n + 1 buttons [obs data]. The d-pad moves
//! between them, confirm chooses, back closes; a tap chooses a button, a tap elsewhere closes.

use mmk_editor::attr::{Attr, AttrOption};

use super::{MakerFlow, Mode};
use crate::layout::Layout;
use crate::menu::{Buttons, hit};
use crate::sound::Event;

pub const LAYOUT: &str = "Edit_BalloonPartsChange_00";
/// Ticks a part is held still before its balloon opens: the game opens it once the press is
/// more than 15 frames old [obs, research/specs/maker.md C10].
pub const HOLD: i32 = 16;
/// Buttons in the layout.
pub const BUTTONS: usize = 11;
/// The balloon's side (`Direction`) from its part's place on the screen (layout units, 1280 ×
/// 720 centred, y up): to the left of a part right of x 20, below one above y 40
/// (0x71018bb490) [obs v3.0.3].
const SIDE_X: f32 = 20.0;
const SIDE_Y: f32 = 40.0;

/// `Direction` frame for a balloon anchored at `at`: 0 up and right, 1 up and left, 2 down and
/// left, 3 down and right (the frames mirror the frame through x and / or y).
pub fn direction(at: [f32; 2], side_x: f32, side_y: f32) -> u8 {
    match (at[0] > side_x, at[1] >= side_y) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    }
}

pub struct Balloon {
    /// The part (object index) and its id.
    pub index: usize,
    pub part: i16,
    pub options: Vec<AttrOption>,
    /// The rest of a multigrab selection the choice also goes to.
    pub others: Vec<usize>,
    pub sel: usize,
    pub layout: Option<Layout>,
    /// The press that opened the balloon is still down (its release chooses nothing).
    pub press_held: bool,
    /// Something was changed (the press's undo step is kept).
    pub changed: bool,
}

impl Balloon {
    /// The flags part `self.part` with flags `flags` would have after button `k`.
    pub fn flags_after(&self, k: usize, flags: u32) -> u32 {
        self.options
            .get(k)
            .map_or(flags, |o| o.attr.apply_to(self.part, flags))
    }
}

/// A button's pane.
pub fn button(k: usize) -> String {
    format!("L_Btn_{k:02}")
}

impl MakerFlow {
    /// Open the balloon of part `index` (the hold on it ended the carry). False when the part
    /// has nothing to choose.
    pub(super) fn open_attr(&mut self, index: usize) -> bool {
        self.open_attr_parts(vec![index])
    }

    /// Open the balloon over `parts`: the held one first, then the rest of its multigrab
    /// selection, which wings and parachute choices also go to (maker spec C10).
    pub(super) fn open_attr_parts(&mut self, parts: Vec<usize>) -> bool {
        let Some((&index, others)) = parts.split_first() else {
            return false;
        };
        let options = if others.is_empty() {
            self.editor.attr_options(index)
        } else {
            self.editor.attr_options_many(&parts)
        };
        let Some(part) = self.editor.object(index).map(|o| o.id) else {
            return false;
        };
        if options.is_empty() {
            return false;
        }
        // the first choice that changes something
        let sel = options
            .iter()
            .position(|o| !o.checked && o.enabled)
            .unwrap_or(0);
        self.drag = None;
        self.mode = Mode::Attr;
        self.attr = Some(Balloon {
            index,
            part,
            options: options.into_iter().take(BUTTONS).collect(),
            others: others.to_vec(),
            sel,
            layout: self.ui.layout(LAYOUT),
            press_held: true,
            changed: false,
        });
        self.ui.sound_key(LAYOUT, "In", &[]);
        self.layout_attr(true);
        true
    }

    /// The options of the open balloon (captures, tests).
    pub fn attr_options_shown(&self) -> Option<Vec<Attr>> {
        self.attr
            .as_ref()
            .map(|b| b.options.iter().map(|o| o.attr).collect())
    }

    /// Lay the balloon out over its part: as many buttons as choices, each with its picture,
    /// the current form checked, the forbidden ones greyed, the selection raised.
    pub(super) fn layout_attr(&mut self, opening: bool) {
        let style = self.style();
        let tick = self.tick;
        let Some(b) = &self.attr else { return };
        let Some(o) = self.editor.object(b.index).cloned() else {
            return;
        };
        let pictures: Vec<Option<String>> = (0..b.options.len())
            .map(|k| self.icon_key_flags(&style, b.part, b.flags_after(k, o.flags)))
            .collect();
        // from the part's top centre, on the side the game picks from its screen place
        // (0x71018bb490; the anchor is ours [inf])
        let fp = mmk_editor::parts::footprint(&o);
        let at = self.to_layout(
            (fp.x as f32 + fp.w as f32 / 2.0) * 16.0,
            (fp.y + fp.h) as f32 * 16.0,
        );
        let Some(b) = &mut self.attr else { return };
        let (n, sel) = (b.options.len(), b.sel);
        let options = b.options.clone();
        let Some(l) = &mut b.layout else { return };
        l.set_translate("N_InOut_00", at);
        l.hold("Type", n.saturating_sub(1) as f32);
        let direction = direction(at, SIDE_X, SIDE_Y);
        let (left, below) = (matches!(direction, 1 | 2), direction >= 2);
        l.hold("Direction", direction as f32);
        for k in 0..BUTTONS {
            l.clear_pane(&format!("N_Btn_{k:02}"));
        }
        if left || below {
            // the frames mirror the frame; the buttons are turned back
            let scale = [
                if left { -1.0 } else { 1.0 },
                if below { -1.0 } else { 1.0 },
            ];
            for k in 0..BUTTONS {
                l.set_pane(
                    &format!("N_Btn_{k:02}"),
                    mmk_ui::PaneOverride {
                        scale: Some(scale),
                        ..Default::default()
                    },
                );
            }
        }
        for k in 0..BUTTONS {
            let btn = button(k);
            let (Some(opt), Some(picture)) = (options.get(k), pictures.get(k)) else {
                l.show(&btn, false);
                continue;
            };
            l.visible(&btn, None);
            let icon = format!("{btn}/P_Icon_00");
            match picture {
                Some(name) => {
                    l.show(&icon, true);
                    l.image(&icon, "PartIcon", name);
                }
                None => l.show(&icon, false),
            }
            l.show(&format!("{btn}/L_NewIcon_00"), false);
            l.hold(&format!("{btn}/Type"), 0.0);
            l.hold(&format!("{btn}/Check"), if opt.checked { 1.0 } else { 0.0 });
            l.hold(
                &format!("{btn}/Invalid"),
                if opt.enabled { 0.0 } else { 1.0 },
            );
            if k == sel {
                if opening {
                    l.hold_end(&format!("{btn}/Select"));
                } else {
                    l.play(&format!("{btn}/Select"), tick);
                }
            } else {
                l.hold(&format!("{btn}/Select"), 0.0);
            }
        }
        if opening {
            l.play("In", tick);
        }
    }

    /// A tick with the balloon open.
    pub(super) fn attr_tick(
        &mut self,
        input: &super::MakerInput,
        pressed: Buttons,
        cx: &mut crate::Cx,
    ) {
        let tick = self.tick;
        let place_held = input.ui.held.has(Buttons::CONFIRM) || input.down;
        // a click, or a tap that isn't the closing tap of a finger's press (as in the edit)
        let pointer_pressed = input.down && !self.down_was;
        if input.pointer.is_some() {
            self.pointer_tick = tick;
        }
        let tap = input
            .ui
            .click
            .filter(|_| pointer_pressed || tick - self.pointer_tick > super::TAP_AFTER);
        let click = tap.or(if pointer_pressed { input.pointer } else { None });
        let Some(b) = &mut self.attr else {
            self.mode = Mode::Edit;
            return;
        };
        if b.press_held {
            // the opening press: its release (and the tap it makes) choose nothing
            if !place_held {
                b.press_held = false;
            }
            return;
        }
        if let Some(p) = click {
            let panes: Vec<String> = (0..b.options.len())
                .map(|k| format!("{}/B_Btn_00", button(k)))
                .collect();
            let found = b.layout.as_mut().and_then(|l| hit(l, tick, &panes, p));
            match found {
                Some(k) => {
                    b.sel = k;
                    self.choose_attr(cx);
                }
                None => self.close_attr(),
            }
            return;
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(LAYOUT, "normal");
            self.close_attr();
            return;
        }
        if let Some(d) = pressed.direction()
            && d[0] != 0.0
        {
            let n = b.options.len() as i32;
            let k = (b.sel as i32 + d[0].signum() as i32).rem_euclid(n.max(1)) as usize;
            if k != b.sel {
                b.sel = k;
                self.ui.sound_button(LAYOUT, &button(k), Event::Select);
                self.layout_attr(false);
            }
            return;
        }
        if pressed.has(Buttons::CONFIRM) {
            self.choose_attr(cx);
        }
    }

    /// Make the selected choice: the part changes (and makes its sound) and the balloon
    /// closes; a forbidden one is refused and the balloon stays.
    fn choose_attr(&mut self, cx: &mut crate::Cx) {
        let Some(b) = &self.attr else { return };
        let (index, part) = (b.index, b.part);
        let Some(opt) = b.options.get(b.sel).copied() else {
            return;
        };
        let pane = button(b.sel);
        if opt.checked {
            self.close_attr();
            return;
        }
        let others = b.others.clone();
        let result = if others.is_empty() {
            self.editor.apply_attr(index, opt.attr)
        } else {
            let mut parts = vec![index];
            parts.extend(others);
            self.editor.apply_attr_many(&parts, opt.attr).map(|n| n > 0)
        };
        match result {
            Ok(changed) => {
                self.ui.sound_button(LAYOUT, &pane, Event::Decide);
                if let Some(l) = self.attr.as_mut().and_then(|b| b.layout.as_mut()) {
                    l.play(&format!("{pane}/Decide"), self.tick);
                }
                if changed {
                    if let Some(b) = &mut self.attr {
                        b.changed = true;
                    }
                    self.changed();
                    self.sound(part, opt.attr.sound());
                }
                self.close_attr();
            }
            Err(e) => self.refuse(cx, part, &e),
        }
    }

    /// Close the balloon; the press's undo step goes when nothing changed.
    pub(super) fn close_attr(&mut self) {
        if let Some(b) = self.attr.take()
            && !b.changed
        {
            self.editor.discard_checkpoint();
        }
        self.mode = Mode::Edit;
        self.update_page();
    }
}
