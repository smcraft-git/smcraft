//! The clear-condition window (`Edit_MissionWindow_00`, research/specs/maker.md C4): the
//! current condition (icon + sentence) on top, the tabs Actions / Parts / Status / Remove, a
//! selector of conditions (`L_Parts_00`, their icons) and, for parts, of the amount
//! (`L_Count_00`), OK. The sentences and icons are the in-course screens' (`screens`), so the
//! window offers the conditions those know [inf: the game's lists per tab and style weren't
//! read; ours follow the condition list's categories and the style's own power-ups].

use mmk_formats::smm2::clear;

use crate::host::Condition;
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, hit};
use crate::screens;
use crate::sound::Event;

const LAYOUT: &str = "Edit_MissionWindow_00";
// (pane paths name parts only: `L_…/T_…`; plain panes by their own name)
const TABS: [&str; 4] = [
    "L_MissionListBtn_00",
    "L_MissionListBtn_01",
    "L_MissionListBtn_02",
    "L_MissionListBtn_03",
];
const PARTS_SEL: &str = "L_Parts_00";
const COUNT_SEL: &str = "L_Count_00";

/// What the window decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionEvent {
    /// Set (category, condition-list index, amount).
    Set(u8, u8, i16),
    Remove,
    Close,
}

/// The runtime form of a header condition, as the HUD shows it (the kinds of
/// `host::Condition`); None for conditions the screens have no sentence or icon for.
pub fn condition(category: u8, index: u8, n: i16) -> Option<Condition> {
    let needed = n.max(0) as u32;
    let (kind, target) = match (category, index) {
        (clear::PARTS, 1..=41) => (1, index as u32),
        (clear::PARTS, 43) => (2, 0),
        (clear::PARTS, 53 | 54) => (4, 0),
        (clear::PARTS, 55) => (5, 0),
        (clear::PARTS, 66) => (3, 0),
        (clear::PARTS, 67) => (3, 10),
        (clear::PARTS, 68) => (3, 30),
        (clear::PARTS, 69) => (3, 50),
        (clear::STATUS, 42) => (7, 0),
        (clear::STATUS, 44) => (8, 0),
        (clear::STATUS, 45) => (6, 0),
        (clear::STATUS, 47) => (13, 0),
        (clear::STATUS, 48) => (14, 0),
        (clear::STATUS, 49) => (15, 0),
        (clear::STATUS, 50) => (18, 0),
        (clear::STATUS, 51) => (20, 0),
        (clear::STATUS, 58) => (11, 0),
        (clear::STATUS, 59) => (17, 0),
        (clear::STATUS, 62) => (12, 0),
        (clear::STATUS, 64) => (19, 0),
        (clear::STATUS, 72) => (16, 0),
        (clear::ACTIONS, 70) => (9, 0),
        (clear::ACTIONS, 71) => (10, 0),
        // the 3.0 update's (formats clear.rs 78–98)
        (clear::PARTS, 78..=87) => (1, index as u32),
        (clear::STATUS, 88..=93) => (30, [12, 17, 13, 14, 15, 16][index as usize - 88]),
        (clear::STATUS, 94..=98) => (31, [2, 1, 3, 5, 4][index as usize - 94]),
        _ => return None,
    };
    Some(Condition {
        kind,
        target,
        count: 0,
        needed,
        met: false,
        violated: false,
    })
}

/// The conditions of tab `tab` (0 actions, 1 parts, 2 status) in style `style`: (category,
/// index).
pub fn choices(tab: usize, style: &str) -> Vec<(u8, u8)> {
    match tab {
        0 => vec![(clear::ACTIONS, 70), (clear::ACTIONS, 71)],
        1 => (1..=41u8)
            .chain(78..=87)
            .chain([43, 53, 54, 55, 66, 67, 68, 69])
            .filter(|&i| {
                // the Red POW Block and the 3D World enemies only where they are; Mechakoopas and
                // the Koopalings not in 3D World [inf]
                match i {
                    54 => style == "3W",
                    31..=36 => style == "3W",
                    80..=87 => style != "3W",
                    _ => true,
                }
            })
            .map(|i| (clear::PARTS, i))
            .collect(),
        _ => {
            // the style's own power-up and rides [inf]
            // (the update's: Link and SMB2 Mario in SMB1, Frog, Balloon, Flying Squirrel,
            // Boomerang Mario and the wearables in 3D World)
            let own: &[u8] = match style {
                "M1" => &[58, 62, 88, 89],
                "M3" => &[47, 58, 62, 90],
                "MW" => &[48, 59, 62, 91],
                "WU" => &[49, 59, 62, 92],
                _ => &[50, 51, 64, 93, 94, 95, 96, 97, 98],
            };
            [42, 44, 45]
                .iter()
                .chain(own)
                .chain(&[72])
                .map(|&i| (clear::STATUS, i))
                .collect()
        }
    }
}

pub struct MissionWindow {
    ui: Ui,
    pub layout: Layout,
    pub tab: usize,
    pub sel: usize,
    pub count: i16,
    /// The amount selector has the focus (else the condition selector).
    pub on_count: bool,
    style: String,
    style_index: usize,
    /// The course's condition when the window opened.
    current: (u8, u8, i16),
    pub tick: i32,
}

impl MissionWindow {
    pub fn new(ui: &Ui, style: &str, current: (u8, u8, i16)) -> Option<Self> {
        let mut layout = ui.layout(LAYOUT)?;
        for (pane, label) in [
            ("T_Title_02", "T_Title_02"),
            ("T_Title_00", "T_Title_00"),
            ("T_Mission_01", "T_Mission_01"),
            (
                "L_MissionDecideBtn_00/T_Btn_00",
                "L_MissionDecideBtn_00-T_Btn_00",
            ),
        ] {
            layout.text_from(pane, label);
        }
        for (k, t) in TABS.iter().enumerate() {
            layout.text_from(
                &format!("{t}/T_Text_00"),
                &format!("L_MissionListBtn_{k:02}-T_Text_00"),
            );
        }
        layout.show("L_MissionDecideBtn_00/T_KeyBtn_00", false);
        layout.play("In", 0);
        layout.play("InContent", 0);
        let style_index = screens::STYLES
            .iter()
            .position(|s| *s == style)
            .unwrap_or(0);
        // open on the current condition's tab
        let (cat, idx, n) = current;
        let tab = match cat {
            clear::ACTIONS => 0,
            clear::STATUS => 2,
            _ => 1,
        };
        let mut w = Self {
            ui: ui.clone(),
            layout,
            tab,
            sel: 0,
            count: n.max(1),
            on_count: false,
            style: style.to_string(),
            style_index,
            current,
            tick: 0,
        };
        w.sel = w
            .list()
            .iter()
            .position(|&(c, i)| c == cat && i == idx)
            .unwrap_or(0);
        w.refresh();
        Some(w)
    }

    fn list(&self) -> Vec<(u8, u8)> {
        choices(self.tab, &self.style)
    }

    /// The choice under the selector, as (category, index, amount).
    pub fn chosen(&self) -> Option<(u8, u8, i16)> {
        let (c, i) = *self.list().get(self.sel)?;
        Some((c, i, if c == clear::PARTS { self.count } else { 0 }))
    }

    fn sentence(&self, c: (u8, u8, i16)) -> Option<String> {
        condition(c.0, c.1, c.2).map(|k| screens::sentence(&self.ui, &k, self.style_index))
    }

    /// Show the tab, the selectors and the sentences.
    fn refresh(&mut self) {
        let current = self.current;
        let now = if current.0 == clear::NONE {
            None
        } else {
            condition(current.0, current.1, current.2)
        };
        let now_text = self.sentence(current);
        let chosen = self.chosen();
        let chosen_text = chosen.and_then(|c| self.sentence(c));
        let (tab, on_count, count, style) = (self.tab, self.on_count, self.count, self.style_index);
        let l = &mut self.layout;
        // the current condition (none: just the title)
        // IconType 1: no current condition, no icon [obs data]
        l.hold("IconType", if now.is_some() { 0.0 } else { 1.0 });
        l.wrapped_text(
            "T_NowMission_00",
            &now_text.unwrap_or_else(|| "---".into()),
            560.0,
            2,
        );
        if let Some(c) = &now {
            screens::set_icon(l, c, "L_MissionIcon_00/P_DummyIcon_00", style);
        }
        for (k, t) in TABS.iter().enumerate() {
            l.hold(&format!("{t}/Select"), if k == tab { 2.0 } else { 0.0 });
        }
        // Type: 1 a condition with an amount (both selectors), 2 without, 3 remove [obs data:
        // what each frame shows]
        let parts = chosen.is_some_and(|c| c.0 == clear::PARTS);
        let ty = match (tab, parts) {
            (3, _) => 3.0,
            (_, true) => 1.0,
            _ => 2.0,
        };
        l.hold("Type", ty);
        if let Some(c) = chosen.and_then(|c| condition(c.0, c.1, c.2)) {
            screens::set_icon(
                l,
                &c,
                &format!("{PARTS_SEL}/L_CenterButton_00/L_MissionIcon_00/P_DummyIcon_00"),
                style,
            );
        }
        l.wrapped_text("T_Mission_00", &chosen_text.unwrap_or_default(), 560.0, 2);
        l.text(
            &format!("{COUNT_SEL}/L_CenterButton_00/T_Count_00"),
            &count.to_string(),
        );
        for (sel, on) in [(PARTS_SEL, !on_count), (COUNT_SEL, on_count)] {
            l.hold(
                &format!("{sel}/L_CenterButton_00/Select"),
                if on { 2.0 } else { 0.0 },
            );
        }
    }

    pub fn step(
        &mut self,
        pressed: Buttons,
        tabs: i32,
        click: Option<[f32; 2]>,
    ) -> Option<MissionEvent> {
        self.tick += 1;
        let mut decide = pressed.has(Buttons::CONFIRM);
        let mut tab_step = tabs;
        let mut value_step = 0i32;
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(LAYOUT, "normal");
            return Some(MissionEvent::Close);
        }
        if let Some(p) = click {
            let tabs: Vec<String> = TABS.iter().map(|t| format!("{t}/B_Btn_00")).collect();
            let buttons = [
                format!("{PARTS_SEL}/L_PlusButton_00"),
                format!("{PARTS_SEL}/L_MinusButton_00"),
                format!("{COUNT_SEL}/L_PlusButton_00"),
                format!("{COUNT_SEL}/L_MinusButton_00"),
                "L_MissionDecideBtn_00/B_Btn_00".to_string(),
                "L_CloseBtn_00".to_string(),
            ];
            if let Some(t) = hit(&mut self.layout, self.tick, &tabs, p) {
                tab_step = t as i32 - self.tab as i32;
            } else {
                match hit(&mut self.layout, self.tick, &buttons, p) {
                    Some(0) => (self.on_count, value_step) = (false, 1),
                    Some(1) => (self.on_count, value_step) = (false, -1),
                    Some(2) => (self.on_count, value_step) = (true, 1),
                    Some(3) => (self.on_count, value_step) = (true, -1),
                    Some(4) => decide = true,
                    Some(_) => return Some(MissionEvent::Close),
                    None => {
                        let window = ["L_BdMissionWindow_00".to_string()];
                        if hit(&mut self.layout, self.tick, &window, p).is_none() {
                            return Some(MissionEvent::Close);
                        }
                    }
                }
            }
        }
        if pressed.has(Buttons::LEFT) || pressed.has(Buttons::RIGHT) {
            let parts = self.chosen().is_some_and(|c| c.0 == clear::PARTS);
            if parts {
                // between the condition and its amount
                self.on_count = pressed.has(Buttons::RIGHT);
            } else {
                tab_step = if pressed.has(Buttons::RIGHT) { 1 } else { -1 };
            }
        }
        if pressed.has(Buttons::UP) {
            value_step = 1;
        } else if pressed.has(Buttons::DOWN) {
            value_step = -1;
        }
        if tab_step != 0 {
            self.tab = (self.tab as i32 + tab_step).rem_euclid(4) as usize;
            self.sel = 0;
            self.on_count = false;
            self.ui.sound_button(LAYOUT, TABS[self.tab], Event::Decide);
        }
        if value_step != 0 && self.tab != 3 {
            if self.on_count {
                // amounts 1–999 [inf; the header holds up to 9999]
                self.count = (self.count as i32 + value_step).clamp(1, 999) as i16;
            } else {
                let n = self.list().len().max(1) as i32;
                self.sel = (self.sel as i32 + value_step).rem_euclid(n) as usize;
            }
            self.ui.sound_button(LAYOUT, "L_PlusButton", Event::Decide);
        }
        if tab_step != 0 || value_step != 0 {
            self.refresh();
        }
        if !decide {
            return None;
        }
        self.ui
            .sound_button(LAYOUT, "L_MissionDecideBtn", Event::Decide);
        if self.tab == 3 {
            return Some(MissionEvent::Remove);
        }
        self.chosen()
            .map(|(c, i, n)| MissionEvent::Set(c, i, n))
            .or(Some(MissionEvent::Close))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_condition_has_a_sentence_and_is_valid() {
        for style in ["M1", "M3", "MW", "WU", "3W"] {
            for tab in 0..3 {
                let list = choices(tab, style);
                assert!(!list.is_empty());
                for (c, i) in list {
                    assert!(clear::categories(i).contains(&c), "{style} {c} {i}");
                    assert!(condition(c, i, 3).is_some(), "{style} {c} {i}");
                }
            }
        }
    }
}
