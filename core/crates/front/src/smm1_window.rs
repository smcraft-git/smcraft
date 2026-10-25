//! SMM1's message window (`Cmn_CmnWindow_00`, the class at 0x0241c510, research/specs/
//! smm1-100mario-screens.md §8c): a message with an OK button or a No / Yes pair over the screen.
//! The 100 Mario screens use it for the pause's Save & Quit question (message 0xb, the pause menu's
//! `Msg_0006`, 0x02413274) and the ending's "every costume" notices (0x33 / 0x34, `Msg_0051` /
//! `Msg_0052`, 0x0259f5ac).
//!
//! The window's states (registered in 0x0241c510): cStart plays `In` (20 frames) at 1.0 with the
//! `Type` frame of its kind (1 one button, 2 two) and fires SLink `In` (0x0241e164); when `In`
//! ends cSelectWait reads the buttons (0x0241e96c); a decided button goes to cDecideEnd — the
//! window stays up for its owner to close — when the owner asked for that (window +0x285, set by
//! the pause), else to cOut: `Out` (10) and SLink `Out`, or `Cancel` and SLink `Cancel` when it
//! closes without a button (0x0241ef4c); at its end the window is closed (+0x283) [obs].

use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, UiInput, hit};
use crate::{DrawItem, push_layout};

pub const LAYOUT: &str = "Cmn_CmnWindow_00";

/// The window's buttons (its `Type` frame, window +0x27c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// One centred button (`L_BtnC_00`, `Type` 1).
    Ok,
    /// `L_BtnL_00` (No) and `L_BtnR_00` (Yes), `Type` 2.
    YesNo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowState {
    In,
    Select,
    /// A button was decided and the owner closes the window (cDecideEnd).
    DecideEnd,
    Out,
    Closed,
}

/// What a step tells the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    /// A button was decided: Yes / OK (true) or No (false). The window then closes by itself
    /// unless it was opened to stay ([`MessageWindow::open`]'s `stay`).
    Decided(bool),
    /// The window's Out has ended.
    Closed,
}

/// The window's buttons in its button table (window +0x20c: L, R, C).
const BUTTONS: [&str; 3] = ["L_BtnL_00", "L_BtnR_00", "L_BtnC_00"];

pub struct MessageWindow {
    layout: Layout,
    pub kind: WindowKind,
    pub state: WindowState,
    pub tick: i32,
    state_start: i32,
    /// The button under the cursor (an index of [`BUTTONS`]).
    pub cursor: usize,
    /// Stay up after a decision (cDecideEnd) instead of closing.
    stay: bool,
    /// The decided answer, once there is one.
    pub answer: Option<bool>,
}

impl MessageWindow {
    /// The window with `text` and button labels `buttons` (No, Yes for [`WindowKind::YesNo`];
    /// one label for OK); `stay` keeps it up after a decision. None without the layout.
    pub fn open(
        ui: &Ui,
        kind: WindowKind,
        text: &str,
        buttons: &[&str],
        stay: bool,
    ) -> Option<Self> {
        let mut l = ui.layout(LAYOUT)?;
        l.text("T_Message_00", text);
        let parts: &[&str] = match kind {
            WindowKind::Ok => &BUTTONS[2..],
            WindowKind::YesNo => &BUTTONS[..2],
        };
        for (part, label) in parts.iter().zip(buttons) {
            l.text(&format!("{part}/T_Btn_00"), label);
        }
        // the progress bar and the A icon belong to the other kinds (loading, wait) [obs
        // 0x0241e164: the A icon's Out at once]
        l.show("W_ProgressBarBG_00", false);
        l.hold_end("L_ABtnIcon_00/Out");
        l.hold(
            "Type",
            match kind {
                WindowKind::Ok => 1.0,
                WindowKind::YesNo => 2.0,
            },
        );
        l.play("In", 0);
        // SLink action 0 (In) on the window's user at once (0x0241e164)
        crate::smm1::fire_key(&l, None, "In");
        let mut w = Self {
            layout: l,
            kind,
            state: WindowState::In,
            tick: 0,
            state_start: 0,
            // the cursor starts on the first button of the table (0x0241e164 → 0x02829198):
            // No of a question [obs; which pane is the table's first: inf from its order]
            cursor: match kind {
                WindowKind::Ok => 2,
                WindowKind::YesNo => 0,
            },
            stay,
            answer: None,
        };
        w.focus(w.cursor);
        Some(w)
    }

    fn set(&mut self, s: WindowState) {
        self.state = s;
        self.state_start = self.tick;
    }

    fn focus(&mut self, i: usize) {
        self.cursor = i;
        let t = self.tick;
        for (k, part) in BUTTONS.iter().enumerate() {
            let a = format!("{part}/OnOff");
            if k == i {
                self.layout.play(&a, t);
            } else {
                self.layout.hold(&a, 0.0);
            }
        }
    }

    fn decide(&mut self, i: usize) -> WindowEvent {
        let t = self.tick;
        let part = BUTTONS[i];
        self.layout.play(&format!("{part}/Decide"), t);
        // each button's own Decide (No nwk_close, Yes SE_CmnYes, OK SE_paletteDecide) [obs data]
        crate::smm1::fire_key(&self.layout, Some(part), "Decide");
        let yes = i != 0;
        self.answer = Some(yes);
        // a decided Yes / OK stays when asked (+0x285); No always leaves with Out (+0x286 is
        // never set by these callers) [obs 0x0241e96c]
        if self.stay && yes {
            self.set(WindowState::DecideEnd);
        } else {
            self.close(false);
        }
        WindowEvent::Decided(yes)
    }

    /// Start the Out (`cancel`: no button decided — `Cancel` and its SLink action instead).
    fn close(&mut self, cancel: bool) {
        let t = self.tick;
        self.layout.stop("In");
        let (anim, key) = if cancel {
            ("Cancel", "Cancel")
        } else {
            ("Out", "Out")
        };
        self.layout.play(anim, t);
        crate::smm1::fire_key(&self.layout, None, key);
        self.set(WindowState::Out);
    }

    /// The owner closes a window that stayed after its decision.
    pub fn dismiss(&mut self) {
        if self.state == WindowState::DecideEnd {
            self.close(false);
        }
    }

    pub fn closed(&self) -> bool {
        self.state == WindowState::Closed
    }

    pub fn step(&mut self, input: &UiInput) -> Option<WindowEvent> {
        self.tick += 1;
        let t = self.tick;
        match self.state {
            WindowState::In => {
                if self.layout.finished("In", t) {
                    self.set(WindowState::Select);
                }
            }
            WindowState::Select => {
                if self.kind == WindowKind::YesNo {
                    if input.pressed.has(Buttons::LEFT) && self.cursor != 0 {
                        self.select(0);
                    }
                    if input.pressed.has(Buttons::RIGHT) && self.cursor != 1 {
                        self.select(1);
                    }
                    // B answers No with the window's Cancel [inf: the B path isn't traced; the
                    // Cancel exit is 0x0241ef4c's]
                    if input.pressed.has(Buttons::BACK) {
                        self.answer = Some(false);
                        self.close(true);
                        return Some(WindowEvent::Decided(false));
                    }
                }
                if let Some(c) = input.click {
                    let panes: Vec<String> = match self.kind {
                        WindowKind::Ok => vec![format!("{}/B_Btn_00", BUTTONS[2])],
                        WindowKind::YesNo => BUTTONS[..2]
                            .iter()
                            .map(|p| format!("{p}/B_Btn_00"))
                            .collect(),
                    };
                    if let Some(i) = hit(&mut self.layout, t, &panes, c) {
                        let i = if self.kind == WindowKind::Ok { 2 } else { i };
                        self.focus(i);
                        return Some(self.decide(i));
                    }
                }
                if input.pressed.has(Buttons::CONFIRM) {
                    return Some(self.decide(self.cursor));
                }
            }
            WindowState::DecideEnd => {}
            WindowState::Out => {
                let anim = if self.layout.is_playing("Cancel") {
                    "Cancel"
                } else {
                    "Out"
                };
                if self.layout.finished(anim, t) {
                    self.set(WindowState::Closed);
                    return Some(WindowEvent::Closed);
                }
            }
            WindowState::Closed => {}
        }
        None
    }

    fn select(&mut self, i: usize) {
        crate::smm1::fire_key(&self.layout, Some(BUTTONS[i]), "TouchSelect");
        self.focus(i);
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if self.state != WindowState::Closed {
            push_layout(out, &mut self.layout, self.tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::smm1;

    fn press(b: Buttons) -> UiInput {
        UiInput {
            pressed: b,
            ..Default::default()
        }
    }

    /// The owner's import (skipped without): a question opens on No, answers Yes after its In,
    /// and stays when asked to; No closes with Out.
    #[test]
    fn owner_question_window_answers() {
        let Some(ui) = smm1() else { return };
        let mut w = MessageWindow::open(&ui, WindowKind::YesNo, "Exit game?", &["No", "Yes"], true)
            .expect("Cmn_CmnWindow_00");
        assert_eq!(w.cursor, 0);
        // the In (20 frames) reads no buttons
        assert_eq!(w.step(&press(Buttons::CONFIRM)), None);
        for _ in 0..20 {
            w.step(&UiInput::default());
        }
        assert_eq!(w.state, WindowState::Select);
        w.step(&press(Buttons::RIGHT));
        assert_eq!(w.cursor, 1);
        assert_eq!(
            w.step(&press(Buttons::CONFIRM)),
            Some(WindowEvent::Decided(true))
        );
        assert_eq!(w.state, WindowState::DecideEnd);
        let cues = ui.take_cues();
        assert!(cues.iter().any(|c| c.name == "SE_CmnYes"), "{cues:?}");

        let mut w = MessageWindow::open(&ui, WindowKind::YesNo, "Exit game?", &["No", "Yes"], true)
            .unwrap();
        for _ in 0..21 {
            w.step(&UiInput::default());
        }
        assert_eq!(
            w.step(&press(Buttons::CONFIRM)),
            Some(WindowEvent::Decided(false))
        );
        let closed = (0..30).find(|_| w.step(&UiInput::default()) == Some(WindowEvent::Closed));
        // Out: 10 frames
        assert_eq!(closed, Some(9));
        assert!(w.closed());
    }
}
