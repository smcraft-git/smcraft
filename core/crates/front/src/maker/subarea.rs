//! The sub-area's orientation window (`Edit_SubAreaWindow_00` [obs layout and text]: "Choose an
//! orientation for the sub-area", Horizontal / Vertical), asked when the maker goes to an empty
//! sub-area [inf: when the game asks].

use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, hit};
use crate::sound::Event;

const LAYOUT: &str = "Edit_SubAreaWindow_00";
pub const BUTTONS: [&str; 2] = ["L_Btn_00", "L_Btn_01"];

pub enum SubAreaEvent {
    /// Vertical (true) or horizontal.
    Choose(bool),
    Close,
}

pub struct SubAreaWindow {
    ui: Ui,
    pub layout: Layout,
    /// The button with the cursor: 0 Horizontal, 1 Vertical.
    pub sel: usize,
    pub tick: i32,
}

impl SubAreaWindow {
    pub fn new(ui: &Ui, vertical: bool) -> Option<Self> {
        let mut layout = ui.layout(LAYOUT)?;
        layout.text_from("T_Message_00", "T_Message_00");
        for b in BUTTONS {
            layout.text_from(&format!("{b}/T_Btn_00"), &format!("{b}-T_Btn_00"));
        }
        layout.play("In", 0);
        Some(Self {
            ui: ui.clone(),
            layout,
            sel: vertical as usize,
            tick: 0,
        })
    }

    pub fn step(&mut self, pressed: Buttons, click: Option<[f32; 2]>) -> Option<SubAreaEvent> {
        self.tick += 1;
        if let Some(p) = click {
            let panes: Vec<String> = BUTTONS.iter().map(|b| format!("{b}/B_Btn_00")).collect();
            self.sel = hit(&mut self.layout, self.tick, &panes, p)?;
            return Some(self.decide());
        }
        if pressed.has(Buttons::LEFT) || pressed.has(Buttons::RIGHT) {
            let k = pressed.has(Buttons::RIGHT) as usize;
            if k != self.sel {
                self.sel = k;
                self.ui.sound_button(LAYOUT, BUTTONS[k], Event::Select);
            }
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(LAYOUT, "normal");
            return Some(SubAreaEvent::Close);
        }
        pressed.has(Buttons::CONFIRM).then(|| self.decide())
    }

    fn decide(&mut self) -> SubAreaEvent {
        self.ui
            .sound_button(LAYOUT, BUTTONS[self.sel], Event::Decide);
        SubAreaEvent::Choose(self.sel == 1)
    }
}
