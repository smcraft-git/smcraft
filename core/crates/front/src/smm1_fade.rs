//! SMM1's scene fader (research/specs/smm1-100mario-screens.md §8, v208 `Block.rpx`): the
//! fader object `*0x10193748` holds a type and a speed; every scene request through 0x0282de44
//! resets them to type 1 at speed 4.0 (0x0282dce0, 0x1000d4a4) [obs], and a few callers set
//! their own afterwards (NetWorldMapScene: type 4 at 1.0, 0x027689d8; the map's course start:
//! type 7 at 2.0, 0x027089d8) [obs]. The two `Cmn_Fade_00` instances play the type's pair of
//! animations at that speed (cover 0x023ecf00, reveal 0x023ed0a8) with one pane shown
//! (tables 0x1007d820 / 0x1007d840) [obs]:
//!
//! | type | cover (pane)               | reveal (pane)              |
//! |------|----------------------------|----------------------------|
//! | 1    | `In` 100 (`P_Fade_00`)     | `Out` 100 (`P_Fade_00`)    |
//! | 2    | `InCircle` 60 (`W_Circle_00`) | `OutCircle` 60 (`W_Circle_00`) |
//! | 3    | `InCircleW` 60 (`W_CircleW_00`) | `OutCircleW` 60 (`W_CircleW_00`) |
//! | 4    | `InBlock` 56 (`N_Block_00`) | `OutBlock` 56 (`N_Block_00`) |
//! | 5    | `InBlockFade` 56 (`N_Block_00`) | `OutBlockFade` 100 (`P_Fade_00`) |
//! | 6    | `InFadeBlock` 100 (`P_Fade_00`) | `OutFadeBlock` 56 (`N_Block_00`) |
//! | 7    | `InCircleFade` 60 (`W_Circle_00`) | `OutCircleFade` 100 (`P_Fade_00`) |
//!
//! So a type-1 fade covers in 25 ticks and uncovers in 25; the circles close on `N_Pos_00`,
//! which the world map points at Mario while he leaves (0x024f6be0 → 0x02089364) [obs]. The
//! screen stays covered two ticks while the scene changes [inf: the game holds it until the
//! new scene has loaded].

use crate::layout::{Layout, Ui};
use crate::{DrawItem, push_layout};

/// The panes the types show (0x1007d7ec …, registered at 0x023ecb60) [obs].
const PANES: [&str; 4] = ["P_Fade_00", "W_Circle_00", "W_CircleW_00", "N_Block_00"];

/// One of SMM1's fade types with its speed (animation frames per tick).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smm1FadeKind {
    pub kind: u8,
    pub speed: f32,
}

impl Smm1FadeKind {
    /// Every plain scene change: type 1 at 4.0 (0x0282dce0) [obs].
    pub const SCENE: Self = Self {
        kind: 1,
        speed: 4.0,
    };
    /// Course clear → the between-course map (NetWorldMapScene): blocks at 1.0 (0x027689d8,
    /// speed 0x100e4340) [obs].
    pub const TO_MAP: Self = Self {
        kind: 4,
        speed: 1.0,
    };
    /// The between-course map → the next course: the circle closing on Mario, then a fade, at
    /// 2.0 (0x027089d8, speed 0x100d77dc) [obs].
    pub const MAP_TO_COURSE: Self = Self {
        kind: 7,
        speed: 2.0,
    };

    /// (cover animation, its pane, reveal animation, its pane) [obs tables above].
    fn anims(self) -> Option<(&'static str, usize, &'static str, usize)> {
        Some(match self.kind {
            1 => ("In", 0, "Out", 0),
            2 => ("InCircle", 1, "OutCircle", 1),
            3 => ("InCircleW", 2, "OutCircleW", 2),
            4 => ("InBlock", 3, "OutBlock", 3),
            5 => ("InBlockFade", 3, "OutBlockFade", 0),
            6 => ("InFadeBlock", 0, "OutFadeBlock", 3),
            7 => ("InCircleFade", 1, "OutCircleFade", 0),
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Cover,
    Covered,
    Reveal,
}

/// The fader over the SMM1 screens: [`Smm1Fade::cover`], then [`Smm1Fade::step`] reports the
/// tick the screen is covered (change the scene then); the reveal follows by itself.
pub struct Smm1Fade {
    layout: Option<Layout>,
    kind: Smm1FadeKind,
    phase: Phase,
    tick: i32,
    phase_start: i32,
    center: [f32; 2],
}

impl Smm1Fade {
    pub fn new(ui: &Ui) -> Self {
        Self {
            layout: ui.layout("Cmn_Fade_00"),
            kind: Smm1FadeKind::SCENE,
            phase: Phase::Idle,
            tick: 0,
            phase_start: 0,
            center: [0.0, 0.0],
        }
    }

    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// The screen is (being) covered or still covered: input waits.
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Cover | Phase::Covered)
    }

    pub fn kind(&self) -> Smm1FadeKind {
        self.kind
    }

    /// Ticks an animation of the current kind takes at its speed.
    fn ticks(&mut self, anim: &str) -> i32 {
        let speed = self.kind.speed.max(0.01);
        let len = self
            .layout
            .as_mut()
            .map_or(0.0, |l| l.length(anim).max(0.0));
        (len / speed).ceil() as i32
    }

    /// The ticks the cover of `kind` takes (0 without the fade layout).
    pub fn cover_ticks(&mut self, kind: Smm1FadeKind) -> i32 {
        let was = self.kind;
        self.kind = kind;
        let n = kind.anims().map_or(0, |a| self.ticks(a.0));
        self.kind = was;
        n
    }

    /// Start covering the screen with `kind`, its circle on `center` (layout units). False
    /// when there is nothing to fade with (change the scene at once then).
    pub fn cover(&mut self, kind: Smm1FadeKind, center: [f32; 2]) -> bool {
        let Some((cover, pane, ..)) = kind.anims() else {
            return false;
        };
        let Some(l) = &mut self.layout else {
            return false;
        };
        if !l.has(cover) {
            return false;
        }
        self.kind = kind;
        self.center = center;
        self.tick = 0;
        self.phase_start = 0;
        self.phase = Phase::Cover;
        Self::show_only(l, pane, center);
        l.play_ex(cover, 0, 0.0, None, false, kind.speed);
        true
    }

    /// Start covered (another fader covered the screen with `kind`): the reveal follows.
    pub fn hold_covered(&mut self, kind: Smm1FadeKind) {
        let Some((cover, pane, ..)) = kind.anims() else {
            return;
        };
        let Some(l) = &mut self.layout else { return };
        self.kind = kind;
        self.tick = 0;
        self.phase_start = 0;
        self.phase = Phase::Covered;
        Self::stop_all(l);
        Self::show_only(l, pane, self.center);
        l.hold_end(cover);
    }

    fn show_only(l: &mut Layout, pane: usize, center: [f32; 2]) {
        for (i, p) in PANES.iter().enumerate() {
            l.show(p, i == pane);
        }
        l.set_translate("N_Pos_00", center);
    }

    fn stop_all(l: &mut Layout) {
        for k in 1..=7 {
            if let Some((a, _, b, _)) = (Smm1FadeKind {
                kind: k,
                speed: 1.0,
            })
            .anims()
            {
                l.stop(a);
                l.stop(b);
            }
        }
    }

    /// One tick; true on the tick the screen has become covered.
    pub fn step(&mut self) -> bool {
        if self.phase == Phase::Idle {
            return false;
        }
        self.tick += 1;
        let since = self.tick - self.phase_start;
        let Some((cover, _, reveal, rpane)) = self.kind.anims() else {
            self.phase = Phase::Idle;
            return false;
        };
        match self.phase {
            Phase::Cover => {
                if since >= self.ticks(cover) {
                    self.phase = Phase::Covered;
                    self.phase_start = self.tick;
                    return true;
                }
            }
            Phase::Covered => {
                if since >= 2 {
                    let (t, speed, center) = (self.tick, self.kind.speed, self.center);
                    if let Some(l) = &mut self.layout {
                        Self::stop_all(l);
                        Self::show_only(l, rpane, center);
                        l.play_ex(reveal, t, 0.0, None, false, speed);
                    }
                    self.phase = Phase::Reveal;
                    self.phase_start = self.tick;
                }
            }
            Phase::Reveal => {
                if since >= self.ticks(reveal) {
                    self.phase = Phase::Idle;
                    if let Some(l) = &mut self.layout {
                        Self::stop_all(l);
                    }
                }
            }
            Phase::Idle => {}
        }
        false
    }

    /// Ticks until the reveal ends (0 when idle): the screens below wait for it before they
    /// take input (the scenes' `cFadeEndWait` states, 0x0282dc80) [obs].
    pub fn remaining(&mut self) -> i32 {
        let since = self.tick - self.phase_start;
        let Some((cover, _, reveal, _)) = self.kind.anims() else {
            return 0;
        };
        match self.phase {
            Phase::Idle => 0,
            Phase::Cover => self.ticks(cover) - since + 2 + self.ticks(reveal),
            Phase::Covered => 2 - since + self.ticks(reveal),
            Phase::Reveal => (self.ticks(reveal) - since).max(0),
        }
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if self.phase == Phase::Idle {
            return;
        }
        if let Some(l) = &mut self.layout {
            push_layout(out, l, self.tick);
        }
    }
}
