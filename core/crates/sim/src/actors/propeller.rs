//! NSMBU's Propeller Mushroom (part 44 in NSMBU: ItemPropeller, action vtable 0x71029264c0)
//! [obs v3.0.3: Move enter 0x710134ca70, exe 0x710134c2c0 → 0x710134c3c0, limits 0x710134c880;
//! the integrator in the enemy update 0x7101290e30]. Its Move runs through terrain (+0x524 bit 4)
//! and with its own water rules (+0x530 bit 17 turns the integrator's water halving off):
//! - **Out of a block** (the Block context's enter 0x710134d550 hands it straight to the Basic
//!   context, so there is no slow rise): 8 px up into the block (16 px down out of a downward
//!   one), facing right. Up: vy 3.5 (1.75 in water), gravity −0.09375 (0x7102bee200, its
//!   _INIT), falling at most 0.5 (0.25 in water; the gravity halves too), no sideways speed (the
//!   Block context's exit 0x7101347550 zeroes the acceleration) until it starts to fall; then it
//!   stops dead for a frame. Down: vx 2.5 braking by 0.0625 to 0 while vy is held at 1.8 (0.9 in
//!   water; gravity −0.046875 / −0.0234375), then the same stop.
//! - **After that stop** (and for a placed one, from rest): the bobbing descent. Sideways it eases (0.03125 a frame)
//!   toward 0.7 px/f the way it faces (×0.7 in water; in a course of game version 0 the drift's
//!   push is always +0.03125, so a left-facing one turns right). Vertically six (cap, gravity)
//!   pairs from 0x71023cead4 / 0x71023ceaec: (−1.4, −0.03125), (1.0, −0.03125), (−1.4,
//!   −0.03125), (1.4, −0.03125), (−1.4, −0.03125), (4.0, −0.046875; −0.125 once rising). Each
//!   time vy reaches its cap the next pair is taken and the gravity's sense flips (+0x530 bit 16:
//!   vy = min(vy − g, cap) instead of max(vy + g, cap)): it falls, rises, falls, rises, falls
//!   (its lowest, at the second trough, about 94 px below a placed one's start) and then climbs
//!   away at 4 px/f. In water the caps are ×0.7 (the
//!   last ×0.4).
//! - A placed one starts in the same Move from rest, through terrain like the rest: it bobs down
//!   through the floor under it and climbs away [obs v3.0.3: a new actor's base context is the
//!   Basic one (0x7101298b90: not winged, not the Moon context), and with no event the Basic
//!   context's enter 0x71013466f0 picks Move (state 2; the item state table 0x710133e820 counts
//!   from None = 0) — there is no Fall or Wait first].
//!
//! Not modelled: the multiplayer scatter of several items from one block (vfunc 0x218
//! 0x710134c200: spawn bits 0x1800, ±0.5 / ±1.0 sideways), the 15-frame flag after a downward
//! exit in newer courses (+0x80 / +0x528 bit 20), the leaf-like hop of a bumped one (+0x6c = 1,
//! the ItemLeaf Move 0x7101347170 / 0x7101346980), a Lakitu's throw (the Jugem context's Move
//! keeps its speeds) and the night variant (ItemPropellerMoon).

use super::Actor;
use crate::level::Level;

/// Rise out of a block: launch speed (water 1.75), gravity (0x7102bee200) and fall cap.
const RISE_VY: f32 = 3.5;
const RISE_G: f32 = -0.09375;
const RISE_CAP: f32 = -0.5;
/// The downward exit: sideways start, braking, gravity and the held rise (0x71023188b4).
const SLIDE_VX: f32 = 2.5;
const SLIDE_BRAKE: f32 = 0.0625;
const SLIDE_G: f32 = -0.046875;
const SLIDE_CAP: f32 = 1.8;
/// The descent's (cap, gravity) pairs [obs 0x71023cead4 / 0x71023ceaec].
const CAPS: [f32; 6] = [-1.4, 1.0, -1.4, 1.4, -1.4, 4.0];
const GRAVS: [f32; 6] = [-0.03125, -0.03125, -0.03125, -0.03125, -0.03125, -0.046875];
/// The last pair's gravity once it rises (0xbe000000).
const CLIMB_G: f32 = -0.125;
/// Sideways drift (0x7102319d48) and its easing (0x3d000000).
const DRIFT: f32 = 0.7;
const DRIFT_ACCEL: f32 = 0.03125;
/// The game's float tolerance (0x7102319590).
const EPS: f32 = 1.192_092_9e-7;

/// The Move's sub-phase (+0x6f of the action).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PropPhase {
    /// Not started (the Move enter has not run).
    #[default]
    New,
    /// Up out of a block, until it falls (1).
    Rise,
    /// Down out of a block, until it stops sideways (2).
    Slide,
    /// Stopped for a frame (3).
    Stop,
    /// The bobbing descent and the climb away (4).
    Bob,
}

/// The Propeller Mushroom's Move fields (action +0x6f… and the actor's motion fields
/// +0x278 / +0x27c / +0x280 / +0x284, the gravity sense +0x530 bit 16).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prop {
    pub phase: PropPhase,
    /// The descent's pair index (+0x70; 6 once climbing).
    pub step: u8,
    /// Gravity's sense reversed.
    pub up: bool,
    pub cap: f32,
    pub g: f32,
    /// Sideways target and easing.
    pub target: f32,
    pub accel: f32,
    /// The drift speed the target follows (+0x7c).
    pub drift: f32,
    /// In water last frame (BG +0x660 bit 31; read by the ItemLeaf Move too).
    pub wet_prev: bool,
}

impl Actor {
    /// The Move enter. `block`: out of a block, `Some(down)`; `None`: placed.
    pub(super) fn propeller_enter(&mut self, block: Option<bool>) {
        let wet = self.wet();
        match block {
            Some(down) => {
                self.y += if down { -16.0 } else { 8.0 };
                self.dir = 1;
                let p = &mut self.prop;
                if down {
                    p.target = 0.0;
                    p.g = SLIDE_G;
                    p.accel = SLIDE_BRAKE;
                    self.vx = SLIDE_VX;
                    self.vy = 0.0;
                    p.cap = SLIDE_CAP;
                    p.phase = PropPhase::Slide;
                } else {
                    p.target = 1.0;
                    self.vy = if wet { RISE_VY * 0.5 } else { RISE_VY };
                    p.g = RISE_G;
                    p.accel = 0.0;
                    self.vx = 0.0;
                    p.cap = RISE_CAP;
                    p.phase = PropPhase::Rise;
                }
                self.propeller_limits();
            }
            None => self.propeller_bob_start(),
        }
    }

    /// Into the descent from its first pair (case 3 of the exe, or the enter of a placed one).
    fn propeller_bob_start(&mut self) {
        let wet = self.wet();
        self.vx = 0.0;
        self.vy = 0.0;
        let p = &mut self.prop;
        p.phase = PropPhase::Bob;
        p.step = 0;
        p.cap = CAPS[0];
        p.g = GRAVS[0];
        p.accel = DRIFT_ACCEL;
        p.drift = if self.dir < 0 { -DRIFT } else { DRIFT };
        p.target = if wet { p.drift * 0.7 } else { p.drift };
    }

    /// The limits 0x710134c880: each frame's cap (and gravity) for the phase, in water or not.
    fn propeller_limits(&mut self) {
        let wet = self.wet();
        let p = &mut self.prop;
        match p.phase {
            PropPhase::Rise => {
                p.cap = if wet { RISE_CAP * 0.5 } else { RISE_CAP };
                p.g = if wet { RISE_G * 0.5 } else { RISE_G };
            }
            PropPhase::Slide => {
                p.cap = if wet { SLIDE_CAP * 0.5 } else { SLIDE_CAP };
                p.g = if wet { SLIDE_G * 0.5 } else { SLIDE_G };
            }
            PropPhase::Bob if p.step <= 5 => {
                let k = if !wet {
                    1.0
                } else if p.step == 5 {
                    0.4
                } else {
                    0.7
                };
                p.cap = CAPS[p.step as usize] * k;
            }
            _ => {}
        }
    }

    /// One frame: the Move exe, then the integrator's motion (through terrain).
    pub(super) fn step_propeller(&mut self, level: &Level) {
        let version = level.clear_pipe_paths.version;
        if self.prop.phase == PropPhase::New {
            self.propeller_enter(None);
        } else {
            self.propeller_exe(version);
        }
        self.propeller_move();
    }

    /// The integrator's motion for the Move: the sideways speed eases to its target, vy to its
    /// cap in the gravity's sense; through terrain (+0x524 bit 4, set by the Move enter).
    pub(super) fn propeller_move(&mut self) {
        let p = self.prop;
        let a = p.accel.abs();
        if self.vx < p.target {
            self.vx = (self.vx + a).min(p.target);
        } else if self.vx > p.target {
            self.vx = (self.vx - a).max(p.target);
        }
        self.vy = if p.up {
            (self.vy - p.g).min(p.cap)
        } else {
            (self.vy + p.g).max(p.cap)
        };
        self.x += self.vx;
        self.y += self.vy;
        self.on_ground = false;
    }

    fn propeller_exe(&mut self, version: u32) {
        let wet = self.wet();
        match self.prop.phase {
            PropPhase::New => {}
            PropPhase::Rise => {
                if self.vy >= 0.0 {
                    self.propeller_limits();
                } else {
                    (self.vx, self.vy) = (0.0, 0.0);
                    self.prop.phase = PropPhase::Stop;
                }
            }
            PropPhase::Slide => {
                if self.vx > EPS || self.vx < -EPS {
                    self.propeller_limits();
                } else {
                    (self.vx, self.vy) = (0.0, 0.0);
                    self.prop.phase = PropPhase::Stop;
                }
            }
            // (its wait count +0x71 is 0: one frame)
            PropPhase::Stop => self.propeller_bob_start(),
            PropPhase::Bob => {
                let p = &mut self.prop;
                let d = self.vy - p.cap;
                let mut next = true;
                if (-EPS..=EPS).contains(&d) {
                    p.step += 1;
                    p.up = !p.up;
                    if p.step < 6 {
                        p.cap = CAPS[p.step as usize];
                        p.g = GRAVS[p.step as usize];
                    } else {
                        p.step = 6;
                        next = false;
                    }
                }
                if next && p.step == 5 && self.vy >= 0.0 {
                    p.g = CLIMB_G;
                }
                // the drift's push: toward the facing in courses of version 1 on, always + before
                let push = if version > 0 && self.dir < 0 {
                    -p.accel
                } else {
                    p.accel
                };
                p.drift += push;
                let s = p.drift;
                let off = s.abs() - DRIFT;
                if !(-EPS..=EPS).contains(&off) {
                    let mut s = s;
                    if s >= 0.0 {
                        if s > DRIFT {
                            s -= p.accel;
                            if s <= DRIFT {
                                s = DRIFT;
                            }
                        }
                    } else if s < -DRIFT {
                        s += p.accel;
                        if s >= -DRIFT {
                            s = -DRIFT;
                        }
                    }
                    p.drift = s;
                    p.target = if wet { s * 0.7 } else { s };
                }
                self.propeller_limits();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Mode};
    use super::*;
    use crate::level::TILE;

    fn wu() -> Level {
        let mut level = Level::flat(60);
        level.style = "WU".into();
        level
    }

    #[test]
    fn placed_it_bobs_down_then_climbs_away_at_4() {
        let level = wu();
        let mut a = Actor::new(Kind::StyleItem, 200.0, 300.0);
        let mut low = f32::MAX;
        let mut turns = 0;
        let mut last = 0.0f32;
        for _ in 0..600 {
            a.step(&level, 0.0);
            low = low.min(a.y);
            if a.vy != 0.0 {
                if last != 0.0 && a.vy.signum() != last.signum() {
                    turns += 1;
                }
                last = a.vy;
            }
        }
        // falls, rises, falls, rises, falls, climbs: five turns of direction
        assert_eq!(turns, 5);
        // lowest at the second trough, about 94 px down
        assert!((300.0 - low - 93.7).abs() < 0.5, "dropped {}", 300.0 - low);
        assert_eq!(a.vy, 4.0);
        assert_eq!(a.prop.step, 6);
        // through the floor: no landing
        assert!(!a.on_ground);
        // 0.7 px/f to the right
        assert!((a.vx - 0.7).abs() < 1e-6, "vx {}", a.vx);
    }

    #[test]
    fn placed_on_the_ground_it_bobs_down_through_it() {
        let level = wu();
        let mut a = Actor::new(Kind::StyleItem, 200.0, 32.0);
        for _ in 0..60 {
            a.step(&level, 0.0);
        }
        // 45 frames to its first cap −1.4, then 15 more at it: through the floor
        assert!(a.y < 0.0, "y {}", a.y);
        assert!(!a.on_ground);
        assert_eq!(a.prop.phase, PropPhase::Bob);
    }

    #[test]
    fn first_fall_reaches_its_cap_in_45_frames() {
        let level = wu();
        let mut a = Actor::new(Kind::StyleItem, 200.0, 300.0);
        a.step(&level, 0.0);
        assert_eq!(a.vy, -0.03125);
        for _ in 1..45 {
            a.step(&level, 0.0);
        }
        assert_eq!(a.vy, -1.4);
        assert_eq!(a.prop.step, 0);
        // the next frame takes the second pair: rising toward 1.0
        a.step(&level, 0.0);
        assert_eq!(a.prop.step, 1);
        assert!(a.prop.up);
        assert!(a.vy > -1.4);
    }

    #[test]
    fn out_of_a_block_it_shoots_up_then_stops_and_bobs() {
        let level = wu();
        let mut a = Actor::new(Kind::StyleItem, 20.0 * TILE + 8.0, 96.0);
        a.vy = 0.5;
        a.mode = Mode::Emerge { until: 112.0 };
        a.step(&level, 0.0);
        assert_eq!(a.mode, Mode::Normal);
        // 8 px into the block, then 3.5 − 0.09375
        assert_eq!(a.y, 104.0 + 3.5 - 0.09375);
        let mut top = a.y;
        let mut frames = 1;
        while a.prop.phase != PropPhase::Bob && frames < 200 {
            a.step(&level, 0.0);
            top = top.max(a.y);
            if a.prop.phase != PropPhase::Bob {
                assert_eq!(a.vx, 0.0);
            }
            frames += 1;
        }
        // 3.5 at −0.09375: 37 frames up, 63.6 px
        assert!((top - 104.0 - 63.59).abs() < 0.01, "rose {}", top - 104.0);
        assert!((38..=41).contains(&frames), "{frames}");
    }

    #[test]
    fn a_left_facing_one_turns_right_only_in_version_0_courses() {
        for (version, end) in [(0, DRIFT), (1, -DRIFT)] {
            let mut level = wu();
            level.clear_pipe_paths.version = version;
            let mut a = Actor::new(Kind::StyleItem, 200.0, 300.0);
            a.dir = -1;
            for _ in 0..200 {
                a.step(&level, 0.0);
            }
            assert!((a.vx - end).abs() < 1e-6, "version {version}: vx {}", a.vx);
        }
    }

    #[test]
    fn downward_exit_slides_right_while_rising() {
        let level = wu();
        let mut a = Actor::new(Kind::StyleItem, 200.0, 96.0);
        a.vy = -1.0;
        a.mode = Mode::Emerge { until: 80.0 };
        a.step(&level, 0.0);
        assert_eq!(a.prop.phase, PropPhase::Slide);
        assert_eq!((a.vx, a.vy), (2.5 - 0.0625, 1.8));
        assert_eq!(a.y, 96.0 - 16.0 + 1.8);
        // 2.5 / 0.0625 = 40 frames of braking, then the stop and the descent
        let mut frames = 1;
        while a.prop.phase == PropPhase::Slide {
            a.step(&level, 0.0);
            frames += 1;
        }
        assert_eq!(frames, 41);
    }
}
