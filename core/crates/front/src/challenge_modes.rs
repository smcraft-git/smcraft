//! Challenge Modes (the smm2recomp's relabel mod): Course World's Endless Challenge button is
//! "Challenge Modes", and behind it ZL / ZR (or a tap on the title bar) switches between the
//! 100 Mario Challenge (`challenge`, on the screens the Mods page chose) and SMM2's own Endless
//! Challenge (`endless_run`). The difficulty tabs stay on L / R. The button opens the mode used
//! last (`mods.challenge_mode`); a switch goes through the black fade [inf: the recomp opens
//! the other flow over the menu]. The title bar reads "ZL < mode > ZR" with the font's own ZL /
//! ZR glyphs; the SMM1 look's select has no such bar, so a message says it once.
//!
//! With Challenge Modes off on the Mods page the button keeps its name and opens SMM2's
//! Endless Challenge alone.

use crate::challenge::{ChallengeEvent, ChallengeFlow, ChallengeGo, Presentation};
use crate::endless_run::{self, EndlessFlow};
use crate::fade::FadeKind;
use crate::host::CourseEntry;
use crate::layout::Ui;
use crate::menu::{Buttons, Script};
use crate::mods::CHALLENGE_MODE_KEY;
use crate::play::{CoursePlay, PlayInput};
use crate::sound::Event;
use crate::{Cx, DrawItem, Go};

/// Course World's button in Challenge Modes.
pub const BUTTON_LABEL: &str = "Challenge Modes";
/// Taps above this (layout units) are on the title bar.
pub const TITLE_BAR_BOTTOM: f32 = 275.0;
const ZL: char = '\u{E0E6}';
const ZR: char = '\u{E0E7}';
const HUNDRED: &str = "100 Mario Challenge";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hundred,
    Endless,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Mode::Hundred => "hundred",
            Mode::Endless => "endless",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Mode::Hundred => HUNDRED,
            Mode::Endless => endless_run::TITLE,
        }
    }

    fn other(self) -> Mode {
        match self {
            Mode::Hundred => Mode::Endless,
            Mode::Endless => Mode::Hundred,
        }
    }
}

/// Deferred work behind the fade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModesGo {
    Switch,
}

pub enum Flow {
    Hundred(Box<ChallengeFlow>),
    Endless(Box<EndlessFlow>),
}

pub struct ChallengeModes {
    smm2: Ui,
    smm1: Option<Ui>,
    presentation: Presentation,
    library: Vec<CourseEntry>,
    pub mode: Mode,
    pub flow: Flow,
    /// Switching is offered (Challenge Modes on).
    pub switching: bool,
    /// A switch waits for its fade.
    switch_pending: bool,
    /// Captures: menu presses at global UI ticks (`--front-script`, e.g. `30:zl`).
    pub script: Script,
}

/// The title bar's text for `mode`.
pub fn header(mode: Mode) -> String {
    format!("{ZL} < {} > {ZR}", mode.name())
}

impl ChallengeModes {
    /// `switching`: Challenge Modes (the mode used last, ZL / ZR switch); else SMM2's Endless
    /// Challenge alone. `library`: the runs' courses; `presentation`: the 100 Mario screens.
    pub fn new(
        cx: &mut Cx,
        smm2: &Ui,
        smm1: Option<&Ui>,
        library: Vec<CourseEntry>,
        presentation: Presentation,
        switching: bool,
    ) -> Self {
        let last = cx.host.load_value(CHALLENGE_MODE_KEY);
        let mode = if !switching || last.as_deref().map(str::trim) == Some("endless") {
            Mode::Endless
        } else {
            Mode::Hundred
        };
        let flow = Self::make(cx, smm2, smm1, &library, presentation, mode, switching);
        Self {
            smm2: smm2.clone(),
            smm1: smm1.cloned(),
            presentation,
            library,
            mode,
            flow,
            switching,
            switch_pending: false,
            script: Script::default(),
        }
    }

    fn make(
        cx: &mut Cx,
        smm2: &Ui,
        smm1: Option<&Ui>,
        library: &[CourseEntry],
        presentation: Presentation,
        mode: Mode,
        switching: bool,
    ) -> Flow {
        let h = switching.then(|| header(mode));
        match mode {
            Mode::Endless => Flow::Endless(Box::new(EndlessFlow::new(cx, smm2, library, h))),
            Mode::Hundred => {
                let mut f = ChallengeFlow::new(cx, smm2, smm1, library, presentation);
                if let Some(h) = h {
                    if f.presentation == Presentation::Smm2 {
                        f.set_header(&h);
                    } else {
                        cx.toast("ZL / ZR: the Endless Challenge");
                    }
                }
                Flow::Hundred(Box::new(f))
            }
        }
    }

    fn at_menu(&self) -> bool {
        match &self.flow {
            Flow::Hundred(f) => f.at_menu(),
            Flow::Endless(f) => f.at_menu(),
        }
    }

    /// Change to the other mode (behind the fade).
    pub fn switch(&mut self, cx: &mut Cx) {
        self.switch_pending = false;
        cx.host.unload_course();
        let mode = self.mode.other();
        cx.host.store_value(CHALLENGE_MODE_KEY, mode.key());
        self.flow = Self::make(
            cx,
            &self.smm2,
            self.smm1.as_ref(),
            &self.library,
            self.presentation,
            mode,
            true,
        );
        self.mode = mode;
    }

    pub fn go(&mut self, cx: &mut Cx, g: ChallengeGo) {
        match &mut self.flow {
            Flow::Hundred(f) => f.go(cx, g),
            Flow::Endless(f) => f.go(cx, g),
        }
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<ChallengeEvent> {
        let mut input = *input;
        input.ui.pressed = input.ui.pressed | self.script.step();
        let input = &input;
        if self.switch_pending {
            return None;
        }
        if self.switching && self.at_menu() {
            let p = input.ui.pressed;
            let tap = input.ui.click.is_some_and(|c| c[1] > TITLE_BAR_BOTTOM);
            if p.has(Buttons::ZL) || p.has(Buttons::ZR) || tap {
                self.smm2
                    .sound_button("Nwk_EndlessModeMenu_00", "L_Btn_00", Event::Decide);
                self.switch_pending = true;
                cx.fade_to(FadeKind::Black, Go::Modes(ModesGo::Switch));
                return None;
            }
        }
        match &mut self.flow {
            Flow::Hundred(f) => f.step(input, cx),
            Flow::Endless(f) => f.step(input, cx),
        }
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        match &mut self.flow {
            Flow::Hundred(f) => f.draw(out),
            Flow::Endless(f) => f.draw(out),
        }
    }

    /// The course being played, if any.
    pub fn playing(&mut self) -> Option<&mut CoursePlay> {
        match &mut self.flow {
            Flow::Hundred(f) => f.playing.as_mut(),
            Flow::Endless(f) => f.playing.as_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Saves;
    use crate::menu::UiInput;
    use crate::mods::tests::MemHost;
    use crate::testing::smm2;

    #[test]
    fn zl_switches_modes_and_the_last_one_is_kept() {
        let Some(ui) = smm2() else { return };
        let mut host = MemHost::default();
        let mut saves = Saves::default();
        let mut online = crate::online::Online::default();
        let mut fade = crate::fade::SceneFade::new(None);
        fade.instant = true;
        let (mut pending, mut now, mut toast) = (None, Vec::new(), None);
        let mut cx = Cx {
            host: &mut host,
            saves: &mut saves,
            online: &mut online,
            fade: &mut fade,
            pending: &mut pending,
            now: &mut now,
            toast: &mut toast,
        };
        let mut m = ChallengeModes::new(&mut cx, &ui, None, Vec::new(), Presentation::Smm2, true);
        assert_eq!(m.mode, Mode::Hundred);
        let idle = PlayInput::default();
        let zl = PlayInput {
            ui: UiInput {
                pressed: Buttons::ZL,
                ..Default::default()
            },
            start: false,
        };
        m.step(&idle, &mut cx);
        m.step(&zl, &mut cx);
        // (instant fades: the switch is queued at once)
        assert_eq!(cx.now.as_slice(), [Go::Modes(ModesGo::Switch)]);
        cx.now.clear();
        m.switch(&mut cx);
        assert_eq!(m.mode, Mode::Endless);
        assert!(matches!(m.flow, Flow::Endless(_)));
        assert_eq!(
            cx.host.load_value(CHALLENGE_MODE_KEY).as_deref(),
            Some("endless")
        );
        // the next visit opens on the Endless Challenge; off: always the Endless Challenge
        let m = ChallengeModes::new(&mut cx, &ui, None, Vec::new(), Presentation::Smm2, true);
        assert_eq!(m.mode, Mode::Endless);
        cx.host.store_value(CHALLENGE_MODE_KEY, "hundred");
        let mut m = ChallengeModes::new(&mut cx, &ui, None, Vec::new(), Presentation::Smm2, false);
        assert_eq!(m.mode, Mode::Endless);
        m.step(&zl, &mut cx);
        assert!(cx.now.is_empty());
    }
}
