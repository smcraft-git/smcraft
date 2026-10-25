//! Parachute presets with their own Fall (shared enemy draft §5.2 "Presets"; the EnemyDB
//! `action_parachute` column). The Default Parachute (`Actor::step_parachute`: the swing, −0.4
//! px/f, landing ends it) is kept by most parts; these end it their own way [obs v3.0.3]:
//! - **Thwomp** (Dossun): no Parachute action at all — its own Dossun action (vtable
//!   0x71028c68e8) serves the Parachute context and carries the parachute folded; each Fall
//!   (exe 0x71010c9330) opens it once vy ≤ −4 (−2.5 in water): vy 0, gravity −0.1, max fall
//!   −1.0; Fall's exit (0x71010c99f0) folds it again (gravity −0.25, max fall −5), so every
//!   drop opens it anew. The **Side Thwomp** (SideDossun, vtable 0x710290e888; spawn
//!   0x7101263ef0 folds it too) does the same in its charge (Fall enter / exe / exit
//!   0x7101264e30 / 0x7101265210 / 0x7101265850): once |vx| ≥ 2.3 (2.0 in water or liquid)
//!   it opens — vx 0, then 0.1 px/f² to 1.0 instead of 0.075 to 3.0; the exit folds it.
//! - **Lava Bubble** (Bubble, vtable 0x71028bec00, Fall exe 0x710109a710): the Default fall
//!   until it touches the lava (+0x658 bit 34) or sinks below its hiding level less 16 (big
//!   32) and 16 more for the parachute; it snaps onto that level and leaps as usual.
//! - **Cheep Cheep** (Pukupuku, vtable 0x7102905470, Fall exe 0x7101228b70): ends in water, or
//!   at the area's bottom edge (y ≤ 0).
//! - **Porcupuffer** (Fugumannen, vtable 0x71028cbba0, Fall exe 0x71010dbc80): ends in water,
//!   or 4 px below the bottom edge (16 px in the castle and forest themes); terrain collision is
//!   on while it falls (Fall enter 0x71010dbbd0 / exit 0x71010dbe00 clear and set +0x524 bit 4).
//! - **Wiggler** (Hanachan, vtable 0x71028d62a0, Fall 0x710110c900 / 0x710110c9b0 /
//!   0x710110cad0): the Default fall; its body segments take the head's swing rotation and
//!   their follow mode (0x710110bfa0, modes 2 / 3) changes — drawn only, so the Default here.
//!
//! "In water" for the Cheep Cheep and Porcupuffer is the +0x658 bit 31 test, ending unless the
//! bit-32 / previous-frame pair says it is only at the surface [inf: taken as covered by water].

use super::{Actor, Kind};
use crate::level::Level;

/// The Thwomp's parachute opens at this fall speed (−2.5 in water), then falls at −0.1 / −1.0.
const THWOMP_OPEN: f32 = -4.0;
const THWOMP_OPEN_WET: f32 = -2.5;
/// The Side Thwomp's opens at this charge speed (2.0 in water), then charges at 0.1 / 1.0
/// (folded: 0.075 / 3.0) [obs v3.0.3 0x7101265210, DAT 0x71023190c4].
const SIDE_OPEN: f32 = 2.3;
const SIDE_OPEN_WET: f32 = 2.0;
pub(super) const SIDE_CHARGE: (f32, f32) = (0.075, 3.0);
const SIDE_CHUTE_CHARGE: (f32, f32) = (0.1, 1.0);
pub(super) const THWOMP_CHUTE_GRAVITY: f32 = -0.1;
pub(super) const THWOMP_CHUTE_FALL: f32 = -1.0;

impl Actor {
    /// Parts whose own action carries the parachute (folded) instead of the Parachute
    /// context's fall: the Thwomp and the Side Thwomp (their EnemyDB `action_parachute` is
    /// their own Dossun / SideDossun action).
    pub fn folds_chute(kind: Kind) -> bool {
        matches!(kind, Kind::Thwomp { .. })
    }

    /// A Thwomp's parachute is open (it carries one, `chute`, and this fall opened it).
    pub fn thwomp_chute_open(&self) -> bool {
        Self::folds_chute(self.kind) && self.chute && self.hops == 1
    }

    /// The Thwomp's Fall (0x71010c9330): open the parachute once falling fast enough. Returns
    /// the gravity and fall cap for this frame.
    pub(super) fn thwomp_chute(&mut self) -> (f32, f32) {
        if self.chute && self.hops == 0 {
            let open = if self.wet() {
                THWOMP_OPEN_WET
            } else {
                THWOMP_OPEN
            };
            if self.vy <= open {
                self.hops = 1;
                self.vy = 0.0;
            }
        }
        if self.thwomp_chute_open() {
            (THWOMP_CHUTE_GRAVITY, THWOMP_CHUTE_FALL)
        } else {
            (-0.25, -5.0)
        }
    }

    /// The Side Thwomp's charge (0x7101265210): open the parachute once charging fast
    /// enough. Returns the acceleration and top speed for this frame.
    pub(super) fn side_thwomp_chute(&mut self) -> (f32, f32) {
        if self.chute && self.hops == 0 {
            let open = if self.wet() || self.in_lava() {
                SIDE_OPEN_WET
            } else {
                SIDE_OPEN
            };
            if self.vx.abs() >= open {
                self.hops = 1;
                self.vx = 0.0;
            }
        }
        if self.thwomp_chute_open() {
            SIDE_CHUTE_CHARGE
        } else {
            SIDE_CHARGE
        }
    }

    /// In water (the world senses the liquid into `w3`).
    pub(super) fn wet(&self) -> bool {
        self.w3.flooded || self.w3.liquid.is_some_and(|(y, water)| water && self.y < y)
    }

    /// The water-speed rule (0x71012a9180, or the in-water flag +0x658 bit 31 [obs v3.0.3]): in
    /// water, an underwater course or a night airship / sky course.
    pub(super) fn slow_rule(&self, level: &Level) -> bool {
        self.wet() || level.theme == 4 || (level.night && matches!(level.theme, 3 | 8))
    }

    pub(super) fn in_lava(&self) -> bool {
        self.w3
            .liquid
            .is_some_and(|(y, water)| !water && self.y < y)
    }

    /// A preset's own end of the parachute fall, checked after the Default fall's step (whose
    /// landing ends it for every preset). True: drop the parachute and return to normal.
    pub(super) fn chute_fall_ends(&mut self, theme: u8) -> bool {
        match self.kind {
            Kind::LavaBubble => {
                let s = if self.big { 2.0 } else { 1.0 };
                let level = self.floor_ref - 16.0 * s - 16.0;
                if self.in_lava() || self.y < level {
                    self.y = self.y.max(level);
                    // its leap cycle starts over from its hiding level
                    self.phase = 0;
                    self.timer = 120;
                    self.vx = 0.0;
                    true
                } else {
                    false
                }
            }
            Kind::CheepCheep { .. } => self.wet() || self.y <= 0.0,
            Kind::Porcupuffer => {
                let bottom = if matches!(theme, 2 | 9) { -16.0 } else { -4.0 };
                self.wet() || self.y <= bottom
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Mode;
    use crate::level::Level;

    fn air() -> Level {
        Level::flat(40)
    }

    #[test]
    fn thwomp_opens_its_parachute_on_each_drop() {
        let mut t = Actor::new(
            Kind::Thwomp {
                side: false,
                right: false,
            },
            200.0,
            300.0,
        );
        t.chute = true;
        t.home_y = 300.0;
        t.phase = 1; // falling
        let mut speeds = Vec::new();
        for _ in 0..400 {
            t.step(&air(), 200.0);
            speeds.push(t.vy);
            if t.phase != 1 {
                break;
            }
        }
        // accelerates at 0.25 to −4, the parachute opens (vy 0), then −0.1 to at most −1
        assert!(speeds.iter().all(|&v| v >= -4.0 - 1e-4), "{speeds:?}");
        assert!(speeds.contains(&0.0));
        assert!((speeds[speeds.len() - 2] + 1.0).abs() < 1e-4, "{speeds:?}");
        assert_eq!(t.phase, 2);
        assert!(
            !t.thwomp_chute_open() && t.chute,
            "folded again, still carried"
        );
    }

    #[test]
    fn cheep_cheep_parachute_ends_in_water() {
        let mut c = Actor::new(Kind::CheepCheep { red: false }, 100.0, 200.0);
        c.mode = Mode::parachute(false);
        c.w3.flooded = true;
        c.step(&air(), 0.0);
        assert_eq!(c.mode, Mode::Normal);
        let mut d = Actor::new(Kind::CheepCheep { red: false }, 100.0, 200.0);
        d.mode = Mode::parachute(false);
        d.step(&air(), 0.0);
        assert!(matches!(d.mode, Mode::Parachute { .. }));
    }

    #[test]
    fn lava_bubble_parachute_ends_at_its_hiding_level() {
        let mut b = Actor::new(Kind::LavaBubble, 100.0, 60.0);
        b.mode = Mode::parachute(false);
        b.floor_ref = 64.0; // lava surface
        b.w3.liquid = Some((64.0, false));
        b.step(&air(), 0.0);
        assert_eq!(b.mode, Mode::Normal);
        assert_eq!((b.phase, b.timer), (0, 120));
    }

    #[test]
    fn side_thwomp_opens_its_parachute_in_the_charge() {
        let mut t = Actor::new(
            Kind::Thwomp {
                side: true,
                right: true,
            },
            200.0,
            300.0,
        );
        t.chute = true;
        t.phase = 1;
        t.dir = 1;
        t.home_x = t.x;
        let mut speeds = Vec::new();
        for _ in 0..200 {
            t.step(&air(), 0.0);
            speeds.push(t.vx);
        }
        // 0.075 a frame up to 2.3 (31 frames), opens (vx 0), then 0.1 a frame to 1.0
        let open = speeds.windows(2).position(|w| w[1] < w[0]).unwrap() + 1;
        assert_eq!(open, 31, "{speeds:?}");
        assert!((speeds[open] - 0.1).abs() < 1e-5);
        assert!(speeds[open..].iter().all(|&v| v <= 1.0));
        assert_eq!(*speeds.last().unwrap(), 1.0);
        assert!(t.thwomp_chute_open());
    }
}
