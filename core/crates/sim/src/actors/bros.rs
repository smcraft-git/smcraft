//! The winged Hammer Bro's WingWalk (3-hammer-bro.md "Winged: WingWalk in v3.0.3").
//!
//! A winged Bro runs its Basic action in the Wing context; Fall and Jump land it in WingWalk
//! instead of Walk (the winged flag +0x523 bit 1: slots 0x128 / 0x130 0x71010952f0 /
//! 0x7101095330) [obs v3.0.3: states 0x7101094560, WingWalk enter 0x71010961c0, exe
//! 0x7101096310, exit 0x7101096c50; the Bros action vtables 0x71028bcbd0 (Bros1st: M1, M3,
//! MW) and 0x71028bde10 (BrosU: WU, 3D World), slots 0x120–0x1a8]. Instead of pacing it hops:
//!
//! 0. (BrosU) waits for its `JumpSt` animation (WU: to its end, 12 frames; 3D World: to frame
//!    6, the throw helper's slot 0x98 → 0x7101098d20) [obs 0x7101095610]; Bros1st hops at once.
//! 1. The hop (0x7101096ef0): first the walk turns (0x71010979f0: a wall hit flips the facing
//!    and holds turns for 20 frames, as does a ledge ahead on flat ground; otherwise, past 16 px
//!    from its home x, it faces home); then, unless it landed against a wall, vy 2.0 (1.0 in
//!    water, under water, and at night in the airship and the sky: 0x71012a9180, the enemies'
//!    water-speed rule) and 0.5 px/f forward, or the other way if terrain lies 12 px ahead at
//!    8 px up (both ways blocked: straight up). Airborne until it falls back onto the ground,
//!    where it stops (vx 0).
//! 2. Lands (BrosU: its `JumpEd`, 14 frames), then throws (`Throw`): Bros1st, whose helper holds
//!    the hammer (slot 0xd0 = 1), only while on screen (+0x520 bit 3), releasing it at frame 14
//!    (slot 0x120 0x7101092170) and walking on 20 frames (+0x94, slot 0x180 0x7101091c30);
//!    BrosU creates it at frame 33 (WU, 0x7101099160) / 26 (3D World, 0x7101098a80), none off
//!    screen (0x7101097530), and waits for the animation's end (WU 68 frames, 3D World 60).
//! 3. Then the jump decision with a countdown set to 3 on entry (+0x80; 0x7101097110 with
//!    argument 0 counts it down once a call): every third throw it tries a jump (up 6.5, or
//!    down), else hops again (Bros1st at once, BrosU from `JumpSt`).
//!
//! The throw facing follows the nearest player (more than 1 px away) except while the `Throw`
//! animation plays (0x7101096d40). Its own throw helper, which throws on a timer in Walk, is idle
//! in the Wing context (0x7101098560: in context 1 the helper's slot 0x88, 0x7101091f30 = 1,
//! holds it unless the helper's +0x48 bit 1 is set and actor +0x523 bit 4 clear) [obs v3.0.3].
//! Each throw is a single hammer.
//!
//! The animation lengths are the models' own (`WU_Enemy_bros`: jump_st 12, jump_ed 14, throw 68;
//! `3W_Enemy_bros`: JumpStart 15, JumpEnd 14, Attack 60), played at rate 1 (EnemyModelParams
//! `mAnmAssignParams`; slots 0x19 JumpSt, 0x1b JumpEd, 0x2b Throw of the enemy animation enum).
//!
//! [inf] home x stays where it last landed from a jump (the exe re-latches it on BG bit 8, not
//! decoded); the side-contact flags (+0x658 bits 2 / 3) and the wall-hit flag (bit 14) are both
//! the engine's wall hit of the last frame; the ledge probe is cell-based, 0.1 px past the body;
//! the multiplayer off-screen wait (helper +0x49 bit 0: the `Wait2` 30 frames) is left
//! out. The Sledge Bro (MegaBros) never enters WingWalk: its slots 0x128 / 0x130 send Fall's
//! landing to Walk whatever the wings (0x71011d2710 / 0x71011d2720) [obs v3.0.3]; see
//! `step_sledge_bro`.

use super::{Actor, ENEMY_GRAVITY, MAX_FALL};
use crate::level::{Cell, Level};
use crate::world::{VIEW_H, VIEW_W};

/// The hop: rise, slow rise (the water-speed rule) and forward speed [obs 0x7101096ef0].
const HOP_VY: f32 = 2.0;
const HOP_VY_SLOW: f32 = 1.0;
const HOP_VX: f32 = 0.5;
/// The hop's terrain probe: this far ahead, this far above the feet [obs 0x7101096ef0].
const PROBE_AHEAD: f32 = 12.0;
const PROBE_UP: f32 = 8.0;
/// A turn holds further turns this long (+0x84) [obs 0x71010979f0].
const TURN_HOLD: i32 = 20;
/// Past this far from home x, a hop sets off toward it [obs 0x71010979f0].
const LEASH: f32 = 16.0;
/// The jump countdown set on entry: the third throw tries a jump [obs 0x71010961c0].
const JUMP_CYCLES: i32 = 3;
/// Bros1st: the held hammer leaves at this `Throw` frame, then it waits this long [obs
/// 0x7101092170, 0x7101091be0 / 0x7101091c30].
const FIRST_RELEASE: i32 = 14;
const FIRST_AFTER: i32 = 20;
/// BrosU: `JumpEd`'s length [obs model data].
const JUMP_ED: i32 = 14;

/// The Bro's action class by style: Bros1st (M1, M3, MW) or BrosU (WU and 3D World).
#[derive(Clone, Copy, PartialEq)]
enum Style {
    First,
    Wu,
    W3,
}

impl Style {
    fn of(style: &str) -> Self {
        match style {
            "WU" => Style::Wu,
            "3W" => Style::W3,
            _ => Style::First,
        }
    }

    /// Frames of `JumpSt` before the hop (Bros1st none).
    fn hop_wait(self) -> i32 {
        match self {
            Style::First => 0,
            Style::Wu => 12,
            Style::W3 => 6,
        }
    }

    /// BrosU: the `Throw` frame the hammer appears at, and the animation's length.
    fn throw(self) -> (i32, i32) {
        match self {
            Style::W3 => (26, 60),
            _ => (33, 68),
        }
    }
}

/// The WingWalk state (the action's +0x68 sub-phase and its timers).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BroWing {
    /// In WingWalk (else still in Fall, or landing from a jump).
    pub on: bool,
    /// 0 wait for the hop, 1 airborne, 2 landed, 3 throwing, 4 after the throw.
    pub phase: u8,
    /// Frames into the current animation; Bros1st's 20-frame wait in phase 4.
    pub frames: i32,
    /// Landed against a wall: no hop (+0x90 bit 0).
    pub stuck: bool,
    /// Wall contact at the end of last frame (the BG flags the exe reads).
    pub wall: bool,
}

impl Actor {
    /// One frame of a winged (unstacked) Hammer Bro: Fall, WingWalk, or its jump.
    pub(super) fn step_bro_wing(&mut self, level: &Level) -> Option<Actor> {
        let style = Style::of(&level.style);
        let mut w = self.bro_wing;
        let throwing = w.on
            && match style {
                Style::First => w.phase == 3,
                _ => w.phase >= 3,
            };
        let (px, _) = self.target;
        if !throwing && (px - self.x).abs() > 1.0 {
            self.dir = if px <= self.x { -1 } else { 1 };
        }
        // (drawn with the throw pose while throwing)
        self.timer = if throwing { 0 } else { 100 };
        let mut out = None;
        if self.hops != 0 {
            // the Bros' jump; its landing enters WingWalk again
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
            if self.bro_ghost_move(level, ENEMY_GRAVITY) {
                self.home_x = self.x;
                w.on = false;
            }
            self.bro_wing = w;
            return None;
        }
        // placed on the ground, it starts in WingWalk and its first update runs it (Bros1st:
        // the hop at once) [obs: Ryujinx whole trace X_wbros_M1: vy 2.0 − 0.1875 and 0.5 across
        // on the first update]
        if !w.on && self.age == 1 && level.cell_at(self.x, self.y - 0.5).is_solid() {
            self.on_ground = true;
            self.bro_wing_enter(&mut w);
        }
        if !w.on {
            // Fall (0x7101095810): |vx| ≤ 0.5; grounded → WingWalk
            if self.on_ground {
                self.bro_wing_enter(&mut w);
            } else {
                self.vx = self.vx.clamp(-0.5, 0.5);
            }
        } else {
            match w.phase {
                0 => {
                    w.frames += 1;
                    if w.frames >= style.hop_wait() {
                        self.bro_hop(level, &mut w);
                        w.phase = 1;
                    }
                }
                1 => {
                    if self.vy <= 0.0 && self.on_ground {
                        self.vx = 0.0;
                        w.stuck = w.wall;
                        w.phase = 2;
                        w.frames = 0;
                    }
                }
                2 => {
                    w.frames += 1;
                    let ready = match style {
                        Style::First => self.bro_in_view(),
                        _ => w.frames >= JUMP_ED,
                    };
                    if ready {
                        // the throw needs a unit of the area's projectile budget (0x7101096310);
                        // refused, slot 0x170: Bros1st waits its 20 frames (0x7101091b90), BrosU
                        // goes on to the jump decision or the next hop (0x7101095510) [obs v3.0.3]
                        if self.budget_request(1) {
                            w.phase = 3;
                            w.frames = 0;
                        } else if style == Style::First {
                            w.phase = 4;
                            w.frames = FIRST_AFTER;
                        } else if !self.bro_wing_jump(level, &mut w) {
                            w.phase = 0;
                            w.frames = 0;
                        }
                    }
                }
                3 => {
                    w.frames += 1;
                    match style {
                        Style::First => {
                            if w.frames >= FIRST_RELEASE {
                                out = Some(self.make_hammer());
                                w.phase = 4;
                                w.frames = FIRST_AFTER;
                            }
                        }
                        _ => {
                            if w.frames >= style.throw().0 {
                                if self.bro_in_view() {
                                    out = Some(self.make_hammer());
                                }
                                w.phase = 4;
                            }
                        }
                    }
                }
                _ => {
                    let ready = match style {
                        Style::First => {
                            let t = w.frames;
                            w.frames = (t - 1).max(0);
                            t < 2
                        }
                        _ => {
                            w.frames += 1;
                            w.frames >= style.throw().1
                        }
                    };
                    if ready && !self.bro_wing_jump(level, &mut w) {
                        if style == Style::First {
                            self.bro_hop(level, &mut w);
                            w.phase = 1;
                        } else {
                            w.phase = 0;
                            w.frames = 0;
                        }
                    }
                }
            }
        }
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        if self.hops == 0 {
            self.collide(level);
        } else {
            // the jump moves from the frame it is decided [obs: Ryujinx whole trace
            // X_wbros_M1 frame 168: 16 → 22.3125, vy 6.5 − 0.1875]
            self.bro_ghost_move(level, ENEMY_GRAVITY);
        }
        w.wall = self.wall;
        self.bro_wing = w;
        out
    }

    /// WingWalk's enter (0x71010961c0): the jump countdown, a random facing; on the ground
    /// clear of walls it stops and waits for the hop, else it is airborne.
    fn bro_wing_enter(&mut self, w: &mut BroWing) {
        self.pause = JUMP_CYCLES;
        self.phase = self.rand(2) as u8;
        *w = BroWing {
            on: true,
            wall: w.wall,
            ..BroWing::default()
        };
        if self.on_ground && !w.wall {
            self.vx = 0.0;
        } else {
            w.phase = 1;
        }
    }

    /// The hop launch (0x7101096ef0, after the walk turns 0x71010979f0).
    fn bro_hop(&mut self, level: &Level, w: &mut BroWing) {
        if w.stuck {
            self.vx = 0.0;
        }
        if self.on_ground {
            w.stuck = w.wall;
            if w.stuck {
                self.vx = 0.0;
            }
        }
        if w.wall {
            self.phase ^= 1;
            self.turn_lock = TURN_HOLD;
        }
        if self.on_ground && !w.wall && self.bro_ledge_ahead(level) {
            self.phase ^= 1;
            self.turn_lock = TURN_HOLD;
        }
        if self.turn_lock == 0 && self.on_ground && (self.x - self.home_x).abs() > LEASH {
            self.phase = if self.x <= self.home_x { 0 } else { 1 };
        }
        if w.stuck {
            return;
        }
        if self.vy <= 0.0 {
            let slow = self.slow_rule(level);
            self.vy = if slow { HOP_VY_SLOW } else { HOP_VY };
        }
        let mut speed = HOP_VX;
        if self.bro_blocked_ahead(level) {
            self.phase ^= 1;
            if self.bro_blocked_ahead(level) {
                speed = 0.0;
            }
        }
        self.vx = speed * self.bro_facing();
    }

    /// The jump decision after a throw (0x7101097110 with argument 0): only on the ground
    /// clear of walls and slopes, and only once the entry countdown has run out. True when it
    /// jumped.
    fn bro_wing_jump(&mut self, level: &Level, w: &mut BroWing) -> bool {
        let flat = level
            .slope_at(self.x)
            .is_none_or(|(top, _)| top < self.y - 8.0);
        if w.stuck || !self.on_ground || w.wall || !flat {
            return false;
        }
        if self.pause != 0 {
            self.pause -= 1;
            if self.pause > 0 {
                return false;
            }
        }
        let Some(down) = self.bro_jump_kind(level) else {
            return false;
        };
        // (the exit 0x7101096c50 clears the wall stop and lets the budget's unit go)
        w.stuck = false;
        self.budget_release();
        self.hops = if down { 3 } else { 2 };
        self.vy = if down { 2.5 } else { 6.5 };
        self.vx = 0.0;
        self.speed = self.y;
        self.on_ground = false;
        true
    }

    /// The movement facing (+0x26c: `phase` 0 right, 1 left) as a sign.
    fn bro_facing(&self) -> f32 {
        if self.phase == 0 { 1.0 } else { -1.0 }
    }

    /// Terrain 12 px ahead at 8 px up (the hop's line probe from the centre; cells are wider
    /// than the probe, so its far end decides).
    fn bro_blocked_ahead(&self, level: &Level) -> bool {
        let x = self.x + self.bro_facing() * PROBE_AHEAD;
        level.cell_at(x, self.y + PROBE_UP).is_solid()
    }

    /// No floor just past the body ahead, within its half width below the feet (0x710129acb0).
    fn bro_ledge_ahead(&self, level: &Level) -> bool {
        let reach = self.half_w() + 0.1;
        let x = self.x + self.bro_facing() * reach;
        let c = level.cell_at(x, self.y - 1.0);
        let floor = c.is_solid() || c == Cell::SemiTop;
        let slope = level.slope_at(x).is_some_and(|(s, _)| s >= self.y - reach);
        !floor && !slope
    }

    /// On screen (actor flag +0x520 bit 3): its box overlaps the view.
    fn bro_in_view(&self) -> bool {
        let (x0, y0, x1, y1) = self.bbox();
        let (cx, cy) = self.camera;
        x1 > cx && x0 < cx + VIEW_W && y1 > cy && y0 < cy + VIEW_H
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Kind;
    use crate::level::TILE;

    fn bro(style: &str) -> (Level, Actor) {
        let mut level = Level::flat(80);
        level.style = style.to_string();
        let mut b = Actor::new(Kind::HammerBro, 40.0 * TILE + 8.0, 32.0);
        b.wings = true;
        b.home_x = b.x;
        b.target = (b.x + 64.0, 32.0);
        b.camera = (b.x - 100.0, 0.0);
        (level, b)
    }

    /// Frames until each hammer, and the highest point of the hops in between.
    fn run(level: &Level, b: &mut Actor, frames: usize) -> (Vec<usize>, f32) {
        let mut throws = Vec::new();
        let mut top: f32 = 0.0;
        for f in 0..frames {
            if let Some(h) = b.step_spawning(level) {
                assert_eq!(h.kind, Kind::Hammer);
                throws.push(f);
            }
            if b.hops == 0 {
                top = top.max(b.y - 32.0);
            }
        }
        (throws, top)
    }

    #[test]
    fn a_winged_bro_hops_and_throws_on_each_landing() {
        let (level, mut b) = bro("M1");
        // (no jump: the first decision comes on the third throw)
        let (throws, top) = run(&level, &mut b, 150);
        // a hop at vy 2.0 under −0.1875 peaks about 10 px up and lasts 11 + 11 frames; the held
        // hammer leaves 14 frames after the landing, then it waits 20 frames and hops again
        assert!((9.0..12.0).contains(&top), "{top}");
        assert!(throws.len() >= 2, "{throws:?}");
        let gap = throws[1] - throws[0];
        assert!((54..=60).contains(&gap), "{throws:?}");
        // it moved off its spot (0.5 px/f while airborne) but stays near home
        assert!((b.x - b.home_x).abs() <= LEASH + 12.0);
    }

    #[test]
    fn a_winged_bro_jumps_after_its_third_throw() {
        let (level, mut b) = bro("M1");
        let mut jumped_at = None;
        let mut throws = 0;
        for f in 0..400 {
            if b.step_spawning(&level).is_some() {
                throws += 1;
            }
            if b.hops != 0 && jumped_at.is_none() {
                jumped_at = Some((f, throws));
            }
        }
        let (_, n) = jumped_at.expect("jumps");
        assert_eq!(n, 3);
    }

    #[test]
    fn wu_and_3w_bros_wait_out_their_animations() {
        // WU: JumpSt 12 + hop ≈ 22 + JumpEd 14 + Throw 68 (hammer at 33) a cycle
        let (level, mut b) = bro("WU");
        let (throws, _) = run(&level, &mut b, 400);
        let gap = throws[1] - throws[0];
        assert!((112..=120).contains(&gap), "{throws:?}");
        // 3D World: JumpStart to frame 6, Attack 60 (hammer at 26)
        let (level, mut b) = bro("3W");
        let (throws, _) = run(&level, &mut b, 400);
        let gap = throws[1] - throws[0];
        assert!((98..=106).contains(&gap), "{throws:?}");
    }

    #[test]
    fn the_held_hammer_waits_until_the_bro_is_on_screen() {
        let (level, mut b) = bro("M1");
        b.camera = (b.x + 1000.0, 0.0);
        let (throws, _) = run(&level, &mut b, 200);
        assert!(throws.is_empty());
        assert_eq!(b.bro_wing.phase, 2, "waits after landing");
        b.camera = (b.x - 100.0, 0.0);
        let (throws, _) = run(&level, &mut b, 20);
        assert_eq!(throws.len(), 1);
    }

    #[test]
    fn a_wall_ahead_turns_the_hop() {
        let (mut level, mut b) = bro("M1");
        // a wall 12 px ahead on the right; it sets off left
        level.set_solid(41, 2);
        b.on_ground = true;
        b.phase = 0;
        b.bro_wing = BroWing {
            on: true,
            ..BroWing::default()
        };
        b.step_spawning(&level);
        assert_eq!(b.phase, 1);
        assert!(b.vx < 0.0);
    }

    #[test]
    fn under_water_the_hop_is_lower() {
        let (mut level, mut b) = bro("M1");
        level.theme = 4;
        let (_, top) = run(&level, &mut b, 60);
        assert!(top < 4.0, "{top}");
    }

    #[test]
    fn a_stacked_or_wingless_bro_keeps_walking() {
        let (level, mut b) = bro("M1");
        b.wings = false;
        b.step_spawning(&level);
        assert!(!b.bro_wing.on);
        let (level, mut b) = bro("M1");
        b.stacked = true;
        b.step_spawning(&level);
        assert!(!b.bro_wing.on);
    }
}
