//! The one-time tips over the maker (`Cmn_HelpWindow_00` [obs layout and text]): "In this
//! Course Theme, you can change the height of the water or lava. Give it a try!" (label 0001,
//! after the first switch to a theme with a liquid) and "Set the autoscroll to custom to
//! fine-tune the speed and direction that the screen moves. Try it out!" (0003, after the first
//! choice of custom autoscroll). Behind the window the screen dims except a window round the
//! button the tip is about, and a red arrow (`Cmn_Pointer_00`'s `Lsn_Arrow_00`, red with a white
//! edge as its material makes it) bobs above it pointing down [obs: the reference frames,
//! research/specs/maker.md C13; the game's `Cmn_Spotlight_00` cuts its window with a blend ours
//! doesn't draw: a dim with a hole stands in]. OK, confirm,
//! back or a tap anywhere closes it; each tip shows once (kept as `maker.tip.<name>`) [inf:
//! the game keeps it in the save data].

use crate::layout::{Layout, Ui};
use crate::menu::Buttons;
use crate::sound::Event;

pub const LAYOUT: &str = "Cmn_HelpWindow_00";
const POINTER: &str = "Cmn_Pointer_00";
/// How dark the screen goes round the lit button [obs: the reference frames].
const DIM: u8 = 110;

/// The tips: their message label and the key that remembers they were shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    WaterLevel,
    CustomScroll,
}

impl Tip {
    pub fn label(self) -> &'static str {
        match self {
            Tip::WaterLevel => "0001",
            Tip::CustomScroll => "0003",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tip::WaterLevel => "maker.tip.water_level",
            Tip::CustomScroll => "maker.tip.custom_scroll",
        }
    }
}

pub struct HelpTip {
    ui: Ui,
    pub tip: Tip,
    pub layout: Option<Layout>,
    pub tick: i32,
}

impl HelpTip {
    pub fn new(ui: &Ui, tip: Tip) -> Self {
        let mut layout = ui.layout(LAYOUT);
        if let Some(l) = &mut layout {
            if let Some(t) = l.message(tip.label()) {
                l.text("T_Message_00", &t);
            }
            l.text_from("L_Btn_00/T_Btn_00", "L_Btn_00-T_Btn_00");
            l.play("In", 0);
            // the cursor is on OK: its selected (yellow) look
            l.hold_end("L_Btn_00/Select");
        }
        ui.sound_key(LAYOUT, "In", &[]);
        Self {
            ui: ui.clone(),
            tip,
            layout,
            tick: 0,
        }
    }

    /// A tick: true when the tip is closed.
    pub fn step(&mut self, pressed: Buttons, click: Option<[f32; 2]>) -> bool {
        self.tick += 1;
        // the window's In first (a press that opened it can't close it)
        if self.tick < 12 {
            return false;
        }
        // (a tap anywhere closes it, OK or not [inf])
        if click.is_some() || pressed.has(Buttons::CONFIRM) || pressed.has(Buttons::BACK) {
            if let Some(l) = &mut self.layout {
                l.play("L_Btn_00/Decide", self.tick);
            }
            self.ui.sound_button(LAYOUT, "L_Btn_00", Event::Decide);
            return true;
        }
        false
    }
}

impl HelpTip {
    /// The page's button the tip is about.
    pub fn button(&self) -> &'static str {
        match self.tip {
            Tip::WaterLevel => "L_LeftParts_00/L_WaterLevelBtn_00/B_Btn_00",
            Tip::CustomScroll => "L_LeftParts_00/L_ScrollEditBtn_00/B_Btn_00",
        }
    }

    /// The dim round the lit button (its rectangle `target`: centre, size, where the page
    /// has it now) and the arrow over it (drawn under the window).
    pub fn quads(&self, target: Option<([f32; 2], [f32; 2])>) -> Vec<mmk_ui::Quad> {
        use super::shapes;
        let mut q = Vec::new();
        let dim = [0, 0, 0, DIM];
        let Some((c, size)) = target else {
            shapes::rect(&mut q, -640.0, -360.0, 640.0, 360.0, dim);
            return q;
        };
        let (hw, hh) = (size[0] / 2.0 + 12.0, size[1] / 2.0 + 12.0);
        let (x0, y0, x1, y1) = (c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh);
        shapes::rect(&mut q, -640.0, -360.0, 640.0, y0, dim);
        shapes::rect(&mut q, -640.0, y1, 640.0, 360.0, dim);
        shapes::rect(&mut q, -640.0, y0, x0, y1, dim);
        shapes::rect(&mut q, x1, y0, 640.0, y1, dim);
        shapes::rect(&mut q, x0, y0, x1, y1, [255, 255, 255, 40]);
        // the arrow: Lsn_Arrow_00 points up, drawn upside down; it bobs 10 units over 30
        // ticks (`Loop`) [obs data]
        let tex = self
            .ui
            .cache_mut()
            .texture_file(POINTER, "Lsn_Arrow_00^t")
            .map(|t| t.id);
        if let Some(t) = tex {
            let phase = (self.tick % 30) as f32 / 30.0;
            let bob = -10.0 * (phase * std::f32::consts::PI).sin();
            let s = 40.0;
            let (ax, ay) = (c[0], y1 + 8.0 + s / 2.0 + bob);
            let mut a = super::view::quad(
                [ax - s / 2.0, ay - s / 2.0, ax + s / 2.0, ay + s / 2.0],
                t,
                [0.0, 1.0, 1.0, 0.0],
                [255; 4],
            );
            a.black = [255, 255, 255, 0];
            a.white = [255, 10, 10, 255];
            q.push(a);
        }
        q
    }
}
