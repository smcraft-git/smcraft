//! SMM1's costume window after a 100 Mario run's credits (research/specs/smm1-ending.md §4.7):
//! the Mystery Mushroom list `Cmn_CMListWindow_00` over `Cmn_CMListContents_00` in its "get"
//! form, opened by the credits manager's cCharaMarioDemoIn (0x0259f4c4 → the CMList handler's
//! 0x025479a4 → the window's 0x023ea380 with mode 1, the contents' 0x023e75ac).
//!
//! - Window (class 0x023e8f14; states cInit, cIn 0x023ea53c, cWait 0x023ea630 / 0x023ea714,
//!   cDecide 0x023eaa68, cOut 0x023eaafc): `In` (10 frames) with `Default` looping (the NEW!
//!   badge), `Type` at frame 1 (the get form: NEW!, the "unlocked" line, no scroll bar),
//!   `BaloonPos` at the costume's column (0x023e9cbc), the balloon's title and name from
//!   `Cmn_CharaMarioTitle` / `Cmn_CharaMarioName` (0x023e9d44). In cWait it counts 180 frames,
//!   then the A icon comes in (`L_ABtnIcon_00`, class 0x025253c8: `In` 3, then its wait); with
//!   the icon waiting, A plays the icon's `Decide` (17) and goes to cDecide, a tap on
//!   `B_CancelArea_00` fires `TouchClose` and goes there too; cDecide waits for the icon to be
//!   back in its wait, then the icon, the contents and the window play `Out` (10) [obs].
//! - Contents (class 0x023e5744; states cInit, cIn 0x023e79e8, cGetDemo 0x023e7a68, cWait,
//!   cOut 0x023e7e48): 60 buttons (`Cmn_CMListBtn_00`, ten a row) over the 200 list places,
//!   scrolled so the costume's row is the second shown (the first two rows and the last four
//!   stay put, 0x023e7008); a place shows its number (place + 1), the costume's `wait.0` when
//!   it is owned (0x023e70ac → 0x0278cf38), else `InValid` without the icon. `In` (10), then
//!   cGetDemo: the costume's button plays `Valid` (30) and the SLink state's trumpet; after 11
//!   frames (and the save) the list waits [obs].
//!
//! The window reads no B button [obs: only the A icon's decide and the tap area].

use std::collections::BTreeSet;

use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, UiInput, hit};
use crate::{DrawItem, push_layout};

pub const WINDOW: &str = "Cmn_CMListWindow_00";
pub const CONTENTS: &str = "Cmn_CMListContents_00";
const ICON: &str = "L_ABtnIcon_00";

/// Frames in cWait before the A icon comes in (0x023ea714: 0xb4) [obs].
pub const A_ICON_AFTER: i32 = 180;
/// cGetDemo's least length (0x023e7a68: frames in state ≥ 11) [obs].
pub const GET_DEMO: i32 = 11;
/// The list's places (0x023e70ac hides places from 200) and its buttons (0x023e6efc: % 60).
pub const PLACES: usize = 200;
pub const BUTTONS: usize = 60;
/// Rows in view (`N_Area_00`, 388 high, rows 90 apart) and the last top row (0x023e7008).
const ROWS_SHOWN: usize = 4;
const LAST_TOP_ROW: usize = 16;

/// One costume of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListCostume {
    pub id: u16,
    /// The resource name (the message tables' label).
    pub res: String,
    /// The sprite cache's model (`SMM1_Costume_<id>`), when it has art.
    pub model: Option<String>,
    /// Its place in the list (0-based).
    pub place: usize,
}

/// The costume list of the SMM1 sprite cache next to the UI import (`sprites/manifest.json`):
/// a table costume's place is its list number − 1 (`CharaMarioTable`'s `ListNo`); an amiibo
/// costume's (id 100 + n) is its id [inf: the list lookup 0x02732568 wasn't traced past 100].
pub fn costume_list(ui: &Ui) -> Vec<ListCostume> {
    let Some(doc) = ui.cache_mut().json_file("sprites/manifest.json") else {
        return Vec::new();
    };
    doc["costumes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = u16::try_from(c["id"].as_u64()?).ok()?;
            let place = match c["list_no"].as_u64() {
                Some(n) if n >= 1 => n as usize - 1,
                _ if id >= 100 => id as usize,
                _ => return None,
            };
            Some(ListCostume {
                id,
                res: c["res"].as_str().unwrap_or_default().to_string(),
                model: c["art"]
                    .as_bool()
                    .unwrap_or(true)
                    .then(|| c["model"].as_str().map(str::to_string))
                    .flatten(),
                place,
            })
        })
        .filter(|c| c.place < PLACES)
        .collect()
}

/// The list's first row shown for the costume at `place` (0x023e7008): its row less one, from
/// 0 to the last top row.
pub fn top_row(place: usize) -> usize {
    (place / 10).saturating_sub(1).min(LAST_TOP_ROW)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostumeState {
    /// The window's and the contents' In.
    In,
    /// The new costume's reveal (contents cGetDemo), the window waiting.
    GetDemo,
    /// cWait: the A icon comes after [`A_ICON_AFTER`] frames, then A closes.
    Wait,
    /// The A icon's Decide plays (cDecide).
    Decide,
    Out,
    Closed,
}

pub struct CostumeWindow {
    window: Layout,
    contents: Layout,
    pub tick: i32,
    pub state: CostumeState,
    state_start: i32,
    /// cWait's frame count (window +0x260).
    wait_frames: i32,
    /// The A icon is in (its In has ended) and A works.
    icon_ready: bool,
    /// The costume's button.
    button: Option<usize>,
    pub costume: u16,
}

impl CostumeWindow {
    /// The window for costume `id`, with `owned` (which includes it) shown in the list. None
    /// without the layouts or the costume.
    pub fn open(ui: &Ui, id: u16, owned: &BTreeSet<u16>) -> Option<Self> {
        let list = costume_list(ui);
        let new = list.iter().find(|c| c.id == id)?.clone();
        let mut window = ui.layout(WINDOW)?;
        let mut contents = ui.layout(CONTENTS)?;
        // the balloon: the costume's title and name (0x023e9d44 → Cmn_CharaMarioTitle /
        // Cmn_CharaMarioName by its name label); its tail over its column (BaloonPos)
        if let Some(t) = ui.message("Cmn_CharaMarioTitle", &new.res) {
            window.text("T_Title_00", &t);
        }
        if let Some(n) = ui.message("Cmn_CharaMarioName", &new.res) {
            window.text("T_Name_00", &n);
        }
        window.hold("BaloonPos", ((new.place % BUTTONS) % 10) as f32);
        window.hold("Type", 1.0);
        window.play_loop("Default", 0);
        window.play("In", 0);
        // the A icon starts out (its cInit holds `Out` at its end, 0x02525b38)
        window.hold_end(&format!("{ICON}/Out"));
        crate::smm1::fire_key(&window, None, "In");
        // the list: four rows from the top row, each place's button (place % 60)
        let top = top_row(new.place);
        let mut button = None;
        let by_place: std::collections::HashMap<usize, &ListCostume> =
            list.iter().map(|c| (c.place, c)).collect();
        for b in 0..BUTTONS {
            let part = format!("L_SelectBtn_{b:02}");
            // the row this button shows: the one of the shown rows ≡ its row (mod 6)
            let row = (top..top + 6).find(|r| r % 6 == b / 10).unwrap_or(top);
            let place = row * 10 + b % 10;
            let shown = row < top + ROWS_SHOWN && place < PLACES;
            contents.show(&part, shown);
            if !shown {
                continue;
            }
            let k = (row - top) as f32;
            contents.set_translate(&part, [-405.0 + 90.0 * (b % 10) as f32, 135.0 - 90.0 * k]);
            contents.text(&format!("{part}/T_Num_00"), &(place + 1).to_string());
            let icon = format!("{part}/P_Icon_00");
            let c = by_place.get(&place);
            if c.is_some_and(|c| c.id == id) {
                button = Some(b);
            }
            match c {
                Some(c) if owned.contains(&c.id) || c.id == id => {
                    if let Some(m) = &c.model {
                        contents.sprite_image(&icon, m, "wait.0");
                    }
                    // the new one stays as it was until its Valid (0x023e70ac) [obs]
                    if c.id == id {
                        contents.hold(&format!("{part}/InValid"), 0.0);
                    } else {
                        contents.hold_end(&format!("{part}/Valid"));
                    }
                }
                // not owned (or no costume there): InValid, no icon (0x0253eb70)
                _ => {
                    contents.hold_end(&format!("{part}/InValid"));
                    contents.show(&icon, false);
                }
            }
        }
        contents.play("In", 0);
        let ui2 = contents.ui().clone();
        ui2.push_cues(ui2.smm1_state(CONTENTS, None, "cIn", &[]));
        Some(Self {
            window,
            contents,
            tick: 0,
            state: CostumeState::In,
            state_start: 0,
            wait_frames: 0,
            icon_ready: false,
            button,
            costume: id,
        })
    }

    fn set(&mut self, s: CostumeState) {
        self.state = s;
        self.state_start = self.tick;
    }

    pub fn closed(&self) -> bool {
        self.state == CostumeState::Closed
    }

    /// One tick; true on the tick the window has closed.
    pub fn step(&mut self, input: &UiInput) -> bool {
        self.tick += 1;
        let t = self.tick;
        match self.state {
            CostumeState::In => {
                if self.contents.finished("In", t) && self.window.finished("In", t) {
                    // contents cGetDemo: the new costume's Valid, SLink getNewChara (0x023e7a68)
                    if let Some(b) = self.button {
                        let part = format!("L_SelectBtn_{b:02}");
                        self.contents.stop(&format!("{part}/InValid"));
                        self.contents.play(&format!("{part}/Valid"), t);
                    }
                    let ui = self.contents.ui().clone();
                    ui.push_cues(ui.smm1_state(CONTENTS, None, "cGetDemo", &[]));
                    self.set(CostumeState::GetDemo);
                }
            }
            CostumeState::GetDemo => {
                if t - self.state_start >= GET_DEMO {
                    if let Some(b) = self.button {
                        // the costume's button is the list's selection [inf]
                        self.contents.play(&format!("L_SelectBtn_{b:02}/OnOff"), t);
                    }
                    self.wait_frames = 0;
                    self.set(CostumeState::Wait);
                }
            }
            CostumeState::Wait => {
                self.wait_frames += 1;
                if self.wait_frames == A_ICON_AFTER {
                    self.window.stop(&format!("{ICON}/Out"));
                    self.window.play(&format!("{ICON}/In"), t);
                }
                if self.wait_frames > A_ICON_AFTER
                    && !self.icon_ready
                    && self.window.finished(&format!("{ICON}/In"), t)
                {
                    self.icon_ready = true;
                    self.window.play_loop(&format!("{ICON}/Loop"), t);
                }
                if !self.icon_ready {
                    return false;
                }
                if input.pressed.has(Buttons::CONFIRM) {
                    self.window.play(&format!("{ICON}/Decide"), t);
                    let ui = self.window.ui().clone();
                    ui.push_cues(ui.smm1_state(WINDOW, Some(ICON), "cDecide", &[]));
                    self.set(CostumeState::Decide);
                } else if let Some(c) = input.click
                    && hit(&mut self.window, t, &["B_CancelArea_00".to_string()], c).is_some()
                {
                    // SLink 0x5e TouchClose (0x023ea714)
                    crate::smm1::fire_key(&self.window, None, "TouchClose");
                    self.out();
                }
            }
            CostumeState::Decide => {
                if self.window.finished(&format!("{ICON}/Decide"), t) {
                    self.out();
                }
            }
            CostumeState::Out => {
                if self.window.finished("Out", t) && self.contents.finished("Out", t) {
                    self.set(CostumeState::Closed);
                    return true;
                }
            }
            CostumeState::Closed => {}
        }
        false
    }

    /// cDecide's end: the A icon, the contents and the window leave (0x023eaa68).
    fn out(&mut self) {
        let t = self.tick;
        for a in ["In", "Loop", "Decide"] {
            self.window.stop(&format!("{ICON}/{a}"));
        }
        self.window.play(&format!("{ICON}/Out"), t);
        self.contents.stop("In");
        self.contents.play("Out", t);
        self.window.stop("In");
        self.window.play("Out", t);
        crate::smm1::fire_key(&self.window, None, "Out");
        self.set(CostumeState::Out);
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if self.closed() {
            return;
        }
        push_layout(out, &mut self.window, self.tick);
        push_layout(out, &mut self.contents, self.tick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::smm1;

    #[test]
    fn the_list_scrolls_the_costume_to_its_second_row() {
        assert_eq!(top_row(0), 0);
        assert_eq!(top_row(15), 0);
        assert_eq!(top_row(25), 1);
        assert_eq!(top_row(99), 8);
        assert_eq!(top_row(199), LAST_TOP_ROW);
    }

    /// The owner's import (skipped without): the window opens on the costume, reveals it, lets
    /// A close it only after the A icon came, and closes after the Outs.
    #[test]
    fn owner_costume_window_runs_on_its_timers() {
        let Some(ui) = smm1() else { return };
        let list = costume_list(&ui);
        if list.is_empty() {
            eprintln!("no SMM1 sprite cache next to the UI import; skipping");
            return;
        }
        let peach = list.iter().find(|c| c.res == "Peach").expect("Peach");
        // her list icon: the sprite cache's `wait.0`
        let model = peach.model.as_deref().expect("her art");
        assert!(ui.cache_mut().sprite_texture(model, "wait.0").is_some());
        let owned: BTreeSet<u16> = [0, peach.id].into();
        let mut w = CostumeWindow::open(&ui, peach.id, &owned).expect("the CMList layouts");
        let cues = ui.take_cues();
        assert!(cues.iter().any(|c| c.name == "SE_CMOpen"), "{cues:?}");
        assert_eq!(w.button, Some(peach.place % BUTTONS));
        let press = UiInput {
            pressed: Buttons::CONFIRM,
            ..Default::default()
        };
        let mut t = 0;
        while w.state != CostumeState::Wait {
            w.step(&press);
            t += 1;
            assert!(t < 100);
        }
        // In 10, the reveal 11
        assert_eq!(t, 10 + GET_DEMO);
        // Peach (place 1) is on the first row shown, her art in, an unowned place without
        let tick = w.tick + 30;
        w.contents.apply(tick);
        let b = format!("L_SelectBtn_{:02}", peach.place % BUTTONS);
        let r = w.contents.rect(&b).expect("her button");
        assert!(r.visible && (r.center[1] - 137.0).abs() < 1.0, "{r:?}");
        let icon = |w: &CostumeWindow, b: &str| {
            w.contents
                .rect(&format!("{b}/P_Icon_00"))
                .is_some_and(|r| r.visible && r.alpha > 0.5)
        };
        assert!(icon(&w, &b));
        assert!(!icon(&w, "L_SelectBtn_09"));
        let cues = ui.take_cues();
        assert!(cues.iter().any(|c| c.name == "SE_trumpet"), "{cues:?}");
        // A does nothing until the A icon has come (180 frames, then its In)
        for _ in 0..A_ICON_AFTER {
            w.step(&press);
        }
        assert_eq!(w.state, CostumeState::Wait);
        let mut n = 0;
        while !w.step(&press) {
            n += 1;
            assert!(n < 100);
        }
        assert!(w.closed());
        let cues = ui.take_cues();
        assert!(cues.iter().any(|c| c.name == "SE_CMChange"), "{cues:?}");
    }
}
