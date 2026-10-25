//! SMW's and NSMBU's own Bowser states [obs v3.0.3]: the KoopaMW action (vtable 0x71028e4060)
//! and the KoopaWU action (0x71028e3648) replace the SMB1 / SMB3 states the decision 0x7101152520
//! leads to. Animation lengths are the models' clips in the owner's RomFS (`MW_Enemy_koopa`'s
//! texture-pattern clips, `WU_Enemy_koopa`'s skeletal ones) at their EnemyModelParams play rates
//! (big: 1.0), an animation ending once its frame reaches its length (the Turn's rounding).
//!
//! - **Walk** (SMW 0x71011564b0 / 0x71011565d0, NSMBU 0x710114ffc0 / 0x7101150120): the
//!   decision's walk (slot 0x308 0x71011524d0) lasts one cycle of Walk, WalkBack (heading against
//!   his facing) or Chase (fast chase): SMW `walk` 40 at 0.75 / 0.75 / 1.2, NSMBU `kp_walk` 108 at
//!   1.2, `bl_walk_back` 81 at 1.2, `kp_walk` at 1.7. Speeds (slot 0x380: SMW 0x71011580c0,
//!   NSMBU 0x7101152dd0): SMW 0.3, fast chase 0.9; NSMBU 0.425 (big 0.85), fast chase 0.595 (big
//!   1.105); none off screen.
//! - **LandOn** (SMW 0x7101156300 / 0x7101156370, NSMBU 0x710114fe40 / 0x710114fe80): stands
//!   through JumpEd (SMW `squat_s` 4 after a little or middle jump, else JumpAttackEd `squat_l2`
//!   16; NSMBU `kp_jump_ed` 40), then chases a player past his front when facing right, else
//!   decides.
//! - **SMW middle branch** (0x7101157dd0): MiddleJump, LittleJump firing in the air, MiddleJump
//!   firing in the air, or Fire. Little / MiddleJump (0x71011566e0 / 0x7101156700, 0x7101156810 /
//!   0x7101156830) squat through JumpSt (`squat_s` 4), then leave (little: vy 1.1 at −0.07,
//!   middle: 3.0 (winged 3.4) at −0.1 at 0.3 px/f, 0.9 in a chase, 0 off screen) into InAir
//!   (0x71011579f0 / 0x7101157ab0), which with the fire flag waits 5 (middle 15) frames, plays
//!   Shoot (`fire` 48) with a flame at frame 10 (0x7101158540) and lands only once that is done.
//! - **SMW Fire** (0x71011569d0 / 0x7101156a40): 10 frames still, Shoot with a flame at frame 10,
//!   then Wait (0x7101157480 / 0x71011574d0) until its frame 20, then the decision.
//! - **SMW FireFalling** (0x7101156b90 / 0x7101156be0 / 0x71011570d0) and its rain (the boss
//!   interface's slot 0x48, 0x710115c660): Upfire1 (`fireup` 57), then five times Upfire2
//!   (`fire2` 20) with a flame at frame 10 going straight up and forward at 50°, 55° … 70°
//!   (from x ± 17, y + 36; big 34 / 72) and 10 still frames after it; the rain manager tracks
//!   the five flames; once every one has left the top of the screen (the flame's BirthFireFall,
//!   0x7100de71c0) a 149-frame rain drops a falling fire every 30 frames at the top of the
//!   screen over the tile columns 216, 184, 152, 120, 88 px ahead of where he stood (big 280 …
//!   88), one per flame. While the manager runs his big branch big-jumps instead.
//! - **SMW BigJump** (0x71011570f0 / 0x7101157110): JumpAttack (`squat_l` 38), then vy 5.9
//!   (winged 6.4) at −0.1875 into InAir, no hip drop; the speed: the gap jump's landing speed
//!   (at most 2.6), none off screen, ±2.0 (winged 1.5) in the fast chase, else half the SMB3 aim
//!   (0x7101155610).
//! - **NSMBU middle branch** (0x7101152ab0): BeforeJump, Fire or JumpFire. BeforeJump
//!   (0x71011502a0 / 0x71011502f0) squats through JumpSt (`kp_jump_st` 59) into Jump
//!   (0x71011503c0 / 0x7101150430): two frames later vy 3.0 (winged 3.4) at −0.1, 0.3 px/f
//!   (1.0 in a chase). Fire (0x71011506b0 / 0x7101150700): Shoot (`kp_fire1` 104), at frame 48
//!   (the interface's slot 0x110 0x710115e280) a flame aimed at the player — the plain
//!   three-lane one off screen (slot 0x138 0x710115e330) — then the decision. JumpFire
//!   (0x71011508c0 / 0x7101150900): JumpAttack (`kp_fire2` 108); at frame 26 vy 3.2 (winged
//!   3.4) at −0.1, at 70 the flame, at 83 JumpAttackEd; landing ends it.
//! - **NSMBU big branch** (0x7101152970): BeforeBigJump (0x7101150d30 / 0x7101150d50: JumpSt)
//!   into BigJump (0x7101150e20 / 0x7101150e90: two frames later vy 5.9 (winged 6.4) at
//!   −0.1875, the SMW big jump's speeds, no hip drop) or SpreadFire (0x71011511f0 /
//!   0x7101151290): Anger (`kp_roar3` 232) and between its frames 70 and 140, every 15 frames,
//!   a straight flame at −10°, 5°, 20°, 35°, 50° (mirrored facing left; slots 0x480 … 0x4a0).
//! - The aimed flame (0x710114b470, a BulletKoopaFire GoStraight): along atan2(dy + 8, dx) to
//!   the player, held within ±45° of level, ±45° up or down by the player's height when he is
//!   behind, −45° with no player; speed 1.5 plus |Bowser's carried speed × cos| (0x7100de1be0).
//!   [inf: it leaves from the engine's mouth point, the game uses the model's mouth joint]
//! - The falling fire (BulletKoopaFireFall; its Fall state 0x71003b46c0 / 0x71003b4740 [obs
//!   v1.0.0]): falls 1.5 px/f, through terrain until it is first clear of it, then lands on the
//!   next floor and burns there 120 frames.
//!
//! [?] The rain's anchor moves to 32 px behind the fifth flame's exit under an area condition
//! (0x710168dcf0 byte 3, actor +0x520 bit 3) not identified; the engine keeps his own x. A
//! flame destroyed before it leaves the screen leaves the manager waiting (the game's too).

use super::{CAP, G, bw, bwf};
use crate::actors::{Actor, Kind, Mode};
use crate::level::{Cell, Level, TILE};
use crate::world::VIEW_H;

/// One of FireFalling's five rising flames as the rain manager follows it (+0x268.. handles,
/// +0x290.. states: 0 not fired, 1 gone, 2 rising, 3 left the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Riser {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    state: u8,
}

/// The falling-fire manager (the boss interface's +0x258: 0 idle, 1 shooting, 2 following
/// the flames, 3 raining; +0x25c anchor x, +0x260 facing, +0x2a4 the rain timer).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rain {
    state: u8,
    risers: [Riser; 5],
    timer: i32,
    base: f32,
    dir: f32,
}

/// SMW / NSMBU Bowser's extra state: the current animation's frame, the states' own counters
/// (the action's +0xc1 … +0xc6, the actor's +0x540) and the falling-fire manager.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BowserStyle {
    frame: u16,
    sub: u8,
    count: u8,
    /// SMW: the little / middle jump fires in the air (+0xc1); its fire step (+0xc4: 0 wait,
    /// 1 Shoot, 2 back, 3 done).
    air_fire: bool,
    air_sub: u8,
    /// SMW: the jump LandOn follows (+0xc3: 0 other, 1 little, 2 middle, 3 big).
    land: u8,
    /// The state InAir / Wait was entered from.
    from: u8,
    /// InAir's landing delay (+0x84).
    delay: u8,
    /// The chase's gap jump (+0x90): its speed is the landing speed from the ground the gap
    /// scan found (+0x88 / +0x8c).
    gap: bool,
    gap_from: (f32, f32),
    /// The gravity a little / middle / big jump left with, for InAir.
    gravity: f32,
    rain: Rain,
    spawns: Vec<Actor>,
}

/// SMW / NSMBU style index (`style_index`).
const MW: usize = 2;

// SMW's clips (`MW_Enemy_koopa`, texture-pattern animations; play rate 1.0 unless noted)
const MW_SQUAT_S: f32 = 4.0;
const MW_SQUAT_L: f32 = 38.0;
const MW_SQUAT_L2: f32 = 16.0;
const MW_FIRE: f32 = 48.0;
const MW_FIREUP: f32 = 57.0;
const MW_FIRE2: f32 = 20.0;
const MW_WALK: f32 = 40.0;
// NSMBU's (`WU_Enemy_koopa`, skeletal)
const WU_JUMP_ST: f32 = 59.0;
const WU_JUMP_ED: f32 = 40.0;
const WU_FIRE1: f32 = 104.0;
const WU_ROAR: f32 = 232.0;
const WU_WALK: f32 = 108.0;
const WU_WALK_BACK: f32 = 81.0;
/// NSMBU's flame frame in Shoot (slot 0x110 0x710115e280).
const WU_FIRE_AT: u16 = 48;
/// The straight / aimed flame's own speed (BulletKoopaFire +0x19d4).
const STRAIGHT_SPEED: f32 = 1.5;
/// FireFalling's rain: 149 frames, a drop every 30 (0x710115c660).
const RAIN_FRAMES: i32 = 149;
const RAIN_STEP: i32 = 30;
/// A falling fire burns 120 frames on the floor.
const FALL_BURN: i32 = 120;
/// The falling fire's speed.
const FALL_SPEED: f32 = -1.5;

/// A binary angle in degrees (for the flames' table angles).
const fn deg(d: i32) -> i32 {
    (d as i64 * 0x1_0000_0000 / 360) as i32
}

impl BowserStyle {
    /// The falling-fire manager is busy (helper +600 ≠ 0): no FireFalling.
    fn raining(&self) -> bool {
        self.rain.state != 0
    }
}

impl Actor {
    /// Frames an animation of `len` frames takes at `rate` (big: 1.0).
    fn koopa_anim(&self, len: f32, rate: f32) -> u16 {
        let rate = if self.big { 1.0 } else { rate.abs() };
        (len / rate).ceil() as u16
    }

    /// Spawns queued beyond the one `step` returns (the rain, a second flame).
    pub fn koopa_take_spawns(&mut self) -> Vec<Actor> {
        std::mem::take(&mut self.koopa.spawns)
    }

    fn koopa_spawn(&mut self, a: Actor) {
        self.koopa.spawns.push(a);
    }

    /// The walk speed (slot 0x380) in SMW / NSMBU.
    pub(super) fn koopa_walk_speed(&mut self, st: usize) {
        let mode = self.bowser_mode();
        let v = match (st, mode) {
            (_, 3) => 0.0,
            (MW, 2) => 0.9,
            (MW, _) => 0.3,
            (_, 2) if self.big => 1.105,
            (_, 2) => 0.595,
            _ if self.big => 0.85,
            _ => 0.425,
        };
        self.vx = v * self.heading;
    }

    /// The middle branch (slot 0x368): SMW 0x7101157dd0, NSMBU 0x7101152ab0.
    pub(super) fn koopa_middle_branch(&mut self, st: usize) {
        let mode = self.bowser_mode();
        if st == MW {
            let mut u = match mode {
                1 | 3 => 1 + self.rand(3),
                2 if self.big => self.rand(3),
                _ => self.rand(4),
            };
            if self.spring_over.is_some_and(|h| h <= self.spring_reach_mid) {
                u = 3;
            }
            match u {
                0 => self.koopa_enter(bw::MIDDLE_JUMP, st, false),
                1 => self.koopa_enter(bw::LITTLE_JUMP, st, true),
                2 => self.koopa_enter(bw::MIDDLE_JUMP, st, true),
                _ => self.koopa_enter(bw::FIRE, st, false),
            }
            return;
        }
        let u = match mode {
            1 => 1 + self.rand(4),
            3 => 1 + self.rand(2),
            2 if self.big => 0,
            _ => self.rand(3),
        };
        let low = self
            .spring_over
            .is_some_and(|h| h <= self.spring_reach_mid || h <= self.spring_reach_third);
        let state = match u {
            _ if low => bw::FIRE,
            0 => bw::BEFORE_JUMP,
            1 | 3 => bw::FIRE,
            _ => bw::JUMP_FIRE,
        };
        self.koopa_enter(state, st, false);
    }

    /// The big branch (slot 0x358): SMW 0x7101157c70, NSMBU 0x7101152970. The coin: always in
    /// the slow chase; in the fast chase when he walks against his facing, else when he met a
    /// wall; random otherwise.
    pub(super) fn koopa_big_branch(&mut self, st: usize) {
        let coin = match self.bowser_mode() {
            1 => true,
            2 => self.heading as i32 != self.dir || self.wall,
            _ => self.rand(2) == 1,
        };
        let spring = self.spring_over.is_some_and(|h| h <= self.spring_reach_big);
        self.koopa.gap = false;
        if st == MW {
            let raining = self.koopa.raining();
            if spring && raining {
                self.bowser_walk(st);
            } else if spring || (coin && !raining) {
                self.koopa_enter(bw::FIRE_FALLING, st, false);
            } else {
                self.koopa_enter(bw::BIG_JUMP, st, false);
            }
            return;
        }
        let state = if coin || spring {
            bw::SPREAD_FIRE
        } else {
            bw::BEFORE_BIG_JUMP
        };
        self.koopa_enter(state, st, false);
    }

    /// The chase's jumps in SMW / NSMBU (slots 0x310 / 0x318 / 0x320): a gap or a high wall
    /// the big jump (NSMBU after BeforeBigJump; a gap with the landing speed), a low wall the
    /// middle jump (NSMBU BeforeJump).
    pub(super) fn koopa_chase_jump(&mut self, st: usize, big: bool, gap: Option<(f32, f32)>) {
        let state = match (st, big || gap.is_some()) {
            (MW, true) => bw::BIG_JUMP,
            (MW, false) => bw::MIDDLE_JUMP,
            (_, true) => bw::BEFORE_BIG_JUMP,
            (_, false) => bw::BEFORE_JUMP,
        };
        self.koopa_enter(state, st, false);
        self.koopa.gap = gap.is_some();
        if let Some(from) = gap {
            self.koopa.gap_from = from;
        }
    }

    /// Enters a state with the SMW / NSMBU rules (`fire`: SMW's in-air fire flag).
    pub(super) fn koopa_enter(&mut self, state: u8, st: usize, fire: bool) {
        let from = self.phase;
        self.phase = state;
        self.koopa.frame = 0;
        self.koopa.sub = 0;
        self.speed = 0.0;
        match state {
            bw::WALK => {
                self.koopa_walk_speed(st);
            }
            bw::LAND_ON => {
                self.vx = 0.0;
            }
            bw::LITTLE_JUMP | bw::MIDDLE_JUMP | bw::BIG_JUMP => {
                self.vx = 0.0;
                self.koopa.air_fire = fire;
                if state == bw::BIG_JUMP {
                    self.bowser_cancel_breath();
                }
            }
            bw::FIRE => {
                self.vx = 0.0;
                // SMW: 10 frames still first (+0x53c)
                self.timer = if st == MW { 10 } else { 0 };
            }
            bw::FIRE_FALLING => {
                self.vx = 0.0;
                self.koopa.count = 0;
                self.timer = 0;
            }
            bw::WAIT => {
                self.vx = 0.0;
                self.timer = 10;
                self.koopa.from = from;
            }
            bw::IN_AIR => {
                self.koopa.from = from;
                self.koopa.air_sub = 0;
                self.koopa.delay = 2;
                if self.koopa.air_fire {
                    self.timer = if from == bw::LITTLE_JUMP { 5 } else { 15 };
                }
            }
            bw::BEFORE_JUMP | bw::BEFORE_BIG_JUMP | bw::SPREAD_FIRE | bw::JUMP_FIRE => {
                self.vx = 0.0;
                self.koopa.count = 0;
                self.timer = 0;
                self.koopa.sub = 1;
            }
            bw::JUMP => {
                self.timer = 3;
            }
            _ => {}
        }
    }

    /// One frame of an SMW / NSMBU state; false when the shared code handles it.
    pub(super) fn koopa_move(&mut self, level: &Level, st: usize) -> bool {
        self.koopa.frame = self.koopa.frame.saturating_add(1);
        let f = self.koopa.frame;
        let mw = st == MW;
        match self.phase {
            bw::WALK => {
                self.koopa_walk_speed(st);
                self.fall(level, super::BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return true;
                }
                let back = self.heading as i32 != self.dir;
                let (len, rate) = match (mw, self.bowser_mode()) {
                    (true, 2) => (MW_WALK, 1.2),
                    (true, _) => (MW_WALK, 0.75),
                    (false, 1 | 2) if back => (WU_WALK_BACK, 1.2),
                    (false, 2) => (WU_WALK, 1.7),
                    (false, 0) if back => (WU_WALK_BACK, 1.2),
                    (false, _) => (WU_WALK, 1.2),
                };
                if f >= self.koopa_anim(len, rate) {
                    self.bowser_decide(level, st);
                }
            }
            bw::LAND_ON => {
                self.fall(level, super::BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return true;
                }
                let len = match (mw, self.koopa.land) {
                    (true, 1 | 2) => MW_SQUAT_S,
                    (true, _) => MW_SQUAT_L2,
                    (false, _) => WU_JUMP_ED,
                };
                if f >= self.koopa_anim(len, 1.0) {
                    self.koopa.land = 0;
                    if self.bowser_player_ahead_right(st) {
                        self.bowser_chase_dispatch(level, st);
                    } else {
                        self.bowser_decide(level, st);
                    }
                }
            }
            bw::LITTLE_JUMP | bw::MIDDLE_JUMP if mw => {
                self.fall(level, super::BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return true;
                }
                self.vx = 0.0;
                if f < self.koopa_anim(MW_SQUAT_S, 1.0) {
                    return true;
                }
                let little = self.phase == bw::LITTLE_JUMP;
                self.on_ground = false;
                if little {
                    self.vy = 1.1;
                    self.koopa.gravity = -0.07;
                    self.koopa_walk_speed(st);
                } else {
                    self.vy = if self.wings { 3.4 } else { 3.0 };
                    self.koopa.gravity = -0.1;
                    let v = match self.bowser_mode() {
                        1 | 2 => 0.9,
                        3 => 0.0,
                        _ => 0.3,
                    };
                    self.vx = v * self.heading;
                    if self.wings {
                        self.bowser_set_flag(bwf::FROM_BIG, false);
                        self.bowser_enter(bw::FLY, st);
                        return true;
                    }
                }
                self.koopa_enter(bw::IN_AIR, st, self.koopa.air_fire);
            }
            bw::BIG_JUMP => {
                if mw {
                    self.fall_capped(level, G, CAP);
                    if self.bowser_ground_lost(st) {
                        return true;
                    }
                    self.vx = 0.0;
                    if f < self.koopa_anim(MW_SQUAT_L, 1.0) {
                        return true;
                    }
                    self.koopa_launch_big(level, st);
                    if self.wings {
                        self.bowser_set_flag(bwf::FROM_BIG, true);
                        self.bowser_enter(bw::FLY, st);
                    } else {
                        self.koopa.air_fire = false;
                        self.koopa_enter(bw::IN_AIR, st, false);
                    }
                    return true;
                }
                // NSMBU: two frames, then off; landing ends it
                self.fall_capped(level, G, CAP);
                if self.koopa.sub == 0 {
                    self.vx = 0.0;
                    if self.timer == 1 {
                        self.koopa.sub = 1;
                        self.koopa_launch_big(level, st);
                        if self.wings {
                            self.bowser_set_flag(bwf::FROM_BIG, true);
                            self.bowser_enter(bw::FLY, st);
                        }
                    }
                } else if self.timer == 0 && self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            bw::IN_AIR => self.koopa_in_air(level, st),
            bw::FIRE => self.koopa_fire(level, st),
            bw::WAIT => {
                self.fall(level, super::BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return true;
                }
                // after Fire, Wait to its frame 20; else 10 frames and then its frame ≤ 4
                let done = if self.koopa.from == bw::FIRE {
                    f >= 20
                } else {
                    self.timer == 0 && f % 40 <= 4
                };
                if done {
                    self.bowser_decide(level, st);
                }
            }
            bw::FIRE_FALLING => self.koopa_fire_falling(level, st),
            bw::BEFORE_JUMP | bw::BEFORE_BIG_JUMP => {
                self.fall(level, super::BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return true;
                }
                if f >= self.koopa_anim(WU_JUMP_ST, 1.0) {
                    let gap = self.koopa.gap;
                    if self.phase == bw::BEFORE_JUMP {
                        self.koopa_enter(bw::JUMP, st, false);
                    } else {
                        self.koopa_enter(bw::BIG_JUMP, st, false);
                        self.timer = 3;
                    }
                    self.koopa.gap = gap;
                    return true;
                }
            }
            bw::JUMP => {
                self.fall(level, -0.1);
                if self.timer == 1 {
                    self.vy = if self.wings { 3.4 } else { 3.0 };
                    self.on_ground = false;
                    let v = if matches!(self.bowser_mode(), 1 | 2) {
                        1.0
                    } else {
                        0.3
                    };
                    self.vx = v * self.heading;
                    if self.wings {
                        self.bowser_set_flag(bwf::FROM_BIG, false);
                        self.bowser_enter(bw::FLY, st);
                    }
                } else if self.timer == 0 && self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            bw::JUMP_FIRE => self.koopa_jump_fire(level, st),
            bw::SPREAD_FIRE => self.koopa_spread_fire(level, st),
            _ => return false,
        }
        true
    }

    /// The big jump's take-off in SMW / NSMBU: vy 5.9 (winged 6.4) at −0.1875; the gap jump
    /// at the landing speed (at most 2.6), none off screen, ±2.0 (winged 1.5) in the fast
    /// chase, else half the SMB3 aim (0x7101155610).
    fn koopa_launch_big(&mut self, level: &Level, st: usize) {
        let _ = st;
        let base = if self.wings { 1.5 } else { 2.0 };
        self.vy = if self.wings { 6.4 } else { 5.9 };
        self.on_ground = false;
        self.vx = if self.koopa.gap {
            let from = self.koopa.gap_from;
            self.bowser_gap_speed_with(level, from, 2.6, G)
        } else {
            match self.bowser_mode() {
                3 => 0.0,
                2 => base * self.heading,
                _ => self.bowser_aim_big_jump(base) * 0.5,
            }
        };
    }

    /// SMW InAir: with the fire flag, waits, plays Shoot with a flame at frame 10 and only
    /// lands once that is over; landing (after a 2-frame grace) is LandOn.
    fn koopa_in_air(&mut self, level: &Level, st: usize) {
        let g = self.koopa.gravity;
        self.fall_capped(level, g, CAP);
        if self.koopa.air_fire {
            match self.koopa.air_sub {
                0 if self.timer == 0 => {
                    self.koopa.air_sub = 1;
                    self.speed = 0.0;
                }
                1 => {
                    self.speed += 1.0;
                    if self.speed >= 10.0 {
                        let fl = self.bowser_flame(st);
                        self.koopa_spawn(fl);
                        self.koopa.air_sub = 2;
                    }
                }
                2 => {
                    self.speed += 1.0;
                    if self.speed >= self.koopa_anim(MW_FIRE, 1.0) as f32 {
                        self.koopa.air_sub = 3;
                    }
                }
                _ => {}
            }
            if self.koopa.air_sub != 3 {
                return;
            }
        }
        if !self.on_ground {
            return;
        }
        if self.koopa.delay > 0 {
            self.koopa.delay -= 1;
            return;
        }
        self.koopa.land = match self.koopa.from {
            bw::LITTLE_JUMP => 1,
            bw::MIDDLE_JUMP => 2,
            bw::BIG_JUMP => 3,
            _ => 0,
        };
        self.koopa.air_fire = false;
        self.bowser_enter(bw::LAND_ON, st);
    }

    /// Fire: SMW 10 still frames, Shoot (`fire` 48) with the flame at frame 10, then Wait;
    /// NSMBU Shoot (`kp_fire1` 104) with the flame at frame 48 — aimed unless off screen —
    /// then the decision.
    fn koopa_fire(&mut self, level: &Level, st: usize) {
        self.fall(level, super::BOWSER_GRAVITY);
        if self.bowser_ground_lost(st) {
            return;
        }
        let f = self.koopa.frame;
        if st == MW {
            match self.koopa.sub {
                0 => {
                    if self.timer == 0 {
                        self.koopa.sub = 1;
                        self.koopa.frame = 0;
                    }
                }
                1 => {
                    if f >= 10 {
                        self.koopa.sub = 2;
                        let fl = self.bowser_flame(st);
                        self.koopa_spawn(fl);
                    }
                }
                _ => {
                    if f >= self.koopa_anim(MW_FIRE, 1.0) {
                        self.koopa_enter(bw::WAIT, st, false);
                    }
                }
            }
            return;
        }
        if self.koopa.sub == 0 && f >= WU_FIRE_AT {
            self.koopa.sub = 1;
            let fl = if self.bowser_mode() != 3 {
                self.koopa_aimed_flame(st)
            } else {
                self.bowser_flame(st)
            };
            self.koopa_spawn(fl);
        }
        if f >= self.koopa_anim(WU_FIRE1, 1.0) {
            self.bowser_decide(level, st);
        }
    }

    /// NSMBU JumpFire: JumpAttack (`kp_fire2` 108); at 26 vy 3.2 (winged 3.4) at −0.1, at 70
    /// the flame (aimed unless off screen; winged: gravity +0.05 and Fly), at 83 JumpAttackEd;
    /// on the ground after take-off, LandOn.
    fn koopa_jump_fire(&mut self, level: &Level, st: usize) {
        let g = if self.koopa.sub >= 2 { -0.1 } else { G };
        self.fall_capped(level, g, CAP);
        let f = self.koopa.frame;
        if self.koopa.sub >= 2 && self.on_ground {
            self.bowser_enter(bw::LAND_ON, st);
            return;
        }
        match self.koopa.sub {
            1 => {
                if self.bowser_ground_lost(st) {
                    return;
                }
                if f >= 26 {
                    self.koopa.sub = 2;
                    self.vy = if self.wings { 3.4 } else { 3.2 };
                    self.on_ground = false;
                }
            }
            2 => {
                if f >= 70 {
                    self.koopa.sub = 3;
                    let fl = if self.bowser_mode() != 3 {
                        self.koopa_aimed_flame(st)
                    } else {
                        self.bowser_flame(st)
                    };
                    self.koopa_spawn(fl);
                    if self.wings {
                        self.bowser_set_flag(bwf::FROM_BIG, false);
                        self.bowser_enter(bw::FLY, st);
                    }
                }
            }
            3 if f >= 83 => {
                self.koopa.sub = 4;
            }
            _ => {}
        }
    }

    /// NSMBU SpreadFire: Anger (`kp_roar3` 232); between its frames 70 and 140, once the
    /// 13-frame wait after a shot is over and (frame − 70) mod 15 ≤ 2 (not at 70 itself), a
    /// straight flame at −10° + 15° per shot (mirrored facing left); then the decision.
    fn koopa_spread_fire(&mut self, level: &Level, st: usize) {
        self.fall(level, super::BOWSER_GRAVITY);
        if self.bowser_ground_lost(st) {
            return;
        }
        let f = self.koopa.frame as i32;
        if self.timer == 0 && (70..=140).contains(&f) {
            let k = f - 70;
            if k >= 1 && k % 15 <= 2 {
                let left = self.dir < 0;
                let step = deg(15).wrapping_mul(self.koopa.count as i32);
                let a = if left {
                    (i32::MIN).wrapping_sub(deg(-10)).wrapping_sub(step)
                } else {
                    deg(-10).wrapping_add(step)
                };
                let fl = self.koopa_straight_flame(st, a as u32);
                self.koopa_spawn(fl);
                self.timer = 13;
                self.koopa.count += 1;
            }
        }
        if f as u16 >= self.koopa_anim(WU_ROAR, 1.0) {
            self.bowser_decide(level, st);
        }
    }

    /// SMW FireFalling: Upfire1, then five rounds of Upfire2 with a rising flame at frame 10
    /// and 10 still frames after; then the decision.
    fn koopa_fire_falling(&mut self, level: &Level, st: usize) {
        self.fall(level, super::BOWSER_GRAVITY);
        let f = self.koopa.frame;
        match self.koopa.sub {
            0 => {
                if f >= self.koopa_anim(MW_FIREUP, 1.0) {
                    self.koopa.sub = 1;
                    let r = &mut self.koopa.rain;
                    r.state = 1;
                    r.base = self.x;
                    r.dir = self.dir as f32;
                    r.risers = [Riser::default(); 5];
                    r.timer = 0;
                }
            }
            1 => {
                self.koopa.count += 1;
                self.koopa.frame = 0;
                self.koopa.sub = 2;
            }
            2 => {
                if f >= 10 {
                    self.koopa_rising_flame();
                    self.koopa.sub = 3;
                }
            }
            3 => {
                if f >= self.koopa_anim(MW_FIRE2, 1.0) {
                    self.timer = 10;
                    self.koopa.sub = 4;
                }
            }
            _ => {
                if self.timer == 0 {
                    if self.koopa.count > 4 {
                        if self.bowser_ground_lost(st) {
                            return;
                        }
                        if self.koopa.rain.state == 1 {
                            self.koopa.rain.state = 2;
                        }
                        self.bowser_decide(level, st);
                    } else {
                        self.koopa.sub = 1;
                    }
                }
            }
        }
    }

    /// One of FireFalling's rising flames: from x ± 17, y + 36 (big 34 / 72) at
    /// 70° − 5°·(5 − n) (mirrored facing left), n the shot (1–5).
    fn koopa_rising_flame(&mut self) {
        let s = self.scale();
        let d = self.dir as f32;
        let n = self.koopa.count as i32;
        let a = if self.dir < 0 {
            0x4e38_e38f_i32.wrapping_add(0x038e_38e3_i32.wrapping_mul(5 - n))
        } else {
            0x31c7_1c71_i32.wrapping_sub(0x038e_38e3_i32.wrapping_mul(5 - n))
        };
        let mut fl = Actor::new(Kind::BowserFire, self.x + 17.0 * s * d, self.y + 36.0 * s);
        fl.big = self.big;
        self.koopa_aim_straight(&mut fl, a as u32);
        // it leaves the screen at the top (BirthFireFall)
        fl.phase = 2;
        let slot = (n - 1).clamp(0, 4) as usize;
        self.koopa.rain.risers[slot] = Riser {
            x: fl.x,
            y: fl.y,
            vx: fl.vx,
            vy: fl.vy,
            state: 2,
        };
        self.koopa_spawn(fl);
    }

    /// A straight flame (GoStraight, unaimed) along the binary angle `a`.
    fn koopa_straight_flame(&mut self, st: usize, a: u32) -> Actor {
        let s = self.scale();
        let d = self.dir as f32;
        let (mx, my) = super::BOWSER_MOUTH[st];
        let mut fl = Actor::new(Kind::BowserFire, self.x + d * mx * s, self.y + my * s);
        fl.big = self.big;
        self.koopa_aim_straight(&mut fl, a);
        fl
    }

    /// The aimed flame (0x710114b470 with its aim): toward the player within ±45° of level,
    /// ±45° by the player's height when he is behind.
    fn koopa_aimed_flame(&mut self, st: usize) -> Actor {
        let s = self.scale();
        let d = self.dir as f32;
        let (mx, my) = super::BOWSER_MOUTH[st];
        let (fx, fy) = (self.x + d * mx * s, self.y + my * s);
        let (dx, dy) = (self.target.0 - fx, self.target.1 - fy);
        let q = 0x2000_0000_i32;
        let left = self.dir < 0;
        let a = if (dx > 0.0 && !left) || (dx < 0.0 && left) {
            let a = crate::fmath::table_atan2(dy + 8.0, dx) as i32;
            if left {
                let m = i32::MIN.wrapping_sub(a).clamp(-q, q);
                i32::MIN.wrapping_sub(m)
            } else {
                a.clamp(-q, q)
            }
        } else if left {
            if dy <= 0.0 { -0x6000_0000 } else { 0x6000_0000 }
        } else if dy <= 0.0 {
            -q
        } else {
            q
        };
        self.koopa_straight_flame(st, a as u32)
    }

    /// GoStraight's motion (0x7100de1be0): along the angle at 1.5 plus |the carried speed ×
    /// cos|; no lane.
    fn koopa_aim_straight(&self, fl: &mut Actor, a: u32) {
        let d = self.dir as f32;
        let mean = self.bowser_mean_speed();
        let carry = if mean * d >= 0.0 { mean } else { 0.0 };
        let (sn, cs) = crate::fmath::table_sin_cos(a);
        let v = STRAIGHT_SPEED + (cs * (carry * cs)).abs();
        fl.dir = if cs < 0.0 { -1 } else { 1 };
        fl.vx = cs * v;
        fl.vy = sn * v;
        fl.home_y = f32::NAN;
        fl.timer = 600;
    }

    /// The falling-fire manager, each frame (0x710115c660): follows the five rising flames,
    /// then rains once all have left the screen.
    pub(super) fn koopa_rain(&mut self) {
        let top = self.camera.1 + VIEW_H;
        let big = self.big;
        let r = &mut self.koopa.rain;
        let mut drop = None;
        match r.state {
            1 | 2 => {
                for k in &mut r.risers {
                    if k.state == 2 {
                        k.x += k.vx;
                        k.y += k.vy;
                        if k.y > top {
                            k.state = 3;
                        }
                    }
                }
                if r.risers.iter().all(|k| k.state == 3) {
                    r.state = 3;
                    r.timer = 0;
                }
            }
            3 => {
                if r.timer == 0 {
                    r.timer = RAIN_FRAMES;
                    return;
                }
                r.timer -= 1;
                let i = r.timer / RAIN_STEP;
                if r.timer == i * RAIN_STEP && r.risers[(4 - i) as usize].state == 3 {
                    let spacing = if big { 48.0 } else { 32.0 };
                    let v = (spacing * i as f32 * r.dir + r.base + r.dir * 88.0) * 0.0625;
                    let x = v.floor() * 16.0 + if big { 0.0 } else { 8.0 };
                    drop = Some(x);
                }
                if r.timer == 0 {
                    r.state = 0;
                }
            }
            _ => {}
        }
        if let Some(x) = drop {
            let mut fl = Actor::new(Kind::BowserFire, x, top);
            fl.big = big;
            fl.phase = 3;
            fl.vy = FALL_SPEED;
            fl.home_y = f32::NAN;
            fl.timer = i32::MAX;
            self.koopa.spawns.push(fl);
        }
    }

    /// The flames of SMW / NSMBU's own states: a rising flame (`phase` 2) goes when it leaves
    /// the top of the screen; a falling fire (`phase` 3) falls through terrain until it is
    /// first clear of it (`hops` 1), lands and burns 120 frames.
    pub(super) fn koopa_flame_step(&mut self, level: &Level) -> bool {
        match self.phase {
            2 => {
                self.x += self.vx;
                self.y += self.vy;
                if self.y > self.camera.1 + VIEW_H {
                    self.mode = Mode::Dead;
                }
                true
            }
            3 => {
                let s = if self.big { 2.0 } else { 1.0 };
                let solid = |x: f32, y: f32| {
                    let c = level.cell_at(x, y);
                    c.is_solid() || c == Cell::SemiTop
                };
                if self.on_ground {
                    self.timer -= 1;
                    if self.timer <= 0 {
                        self.mode = Mode::Dead;
                    }
                    return true;
                }
                if self.hops == 0 && !(0..=14).any(|k| solid(self.x, self.y + (2.0 + k as f32) * s))
                {
                    self.hops = 1;
                }
                let ny = self.y + self.vy;
                if self.hops == 1 && solid(self.x, ny) {
                    self.y = ((ny / TILE).floor() + 1.0) * TILE;
                    self.vy = 0.0;
                    self.on_ground = true;
                    self.timer = FALL_BURN;
                } else {
                    self.y = ny;
                }
                if self.y < level.bottom - 64.0 {
                    self.mode = Mode::Dead;
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::DetMath;

    /// A Bowser on flat ground (feet at 32) in `style`, facing left at the player.
    fn koopa(style: &str) -> (Level, Actor) {
        let mut level = Level::flat(80);
        level.style = style.to_string();
        let x = 40.0 * TILE;
        let mut a = Actor::new(
            Kind::Bowser {
                key: false,
                clown: false,
            },
            x,
            32.0,
        );
        a.on_ground = true;
        a.dir = -1;
        a.heading = -1.0;
        a.target = (x - 100.0, 32.0);
        a.camera = (x - crate::world::VIEW_W / 2.0, 0.0);
        let _ = a.step_spawning(&level);
        (level, a)
    }

    /// Steps `n` frames, collecting what Bowser spawns (frame, actor).
    fn run(level: &Level, a: &mut Actor, n: u32) -> Vec<(u32, Actor)> {
        let mut out = Vec::new();
        for f in 1..=n {
            a.target.1 = 32.0;
            if let Some(s) = a.step_spawning(level) {
                out.push((f, s));
            }
            for s in a.koopa_take_spawns() {
                out.push((f, s));
            }
        }
        out
    }

    fn angle_deg(fl: &Actor) -> f32 {
        fl.vy.datan2(fl.vx).to_degrees()
    }

    #[test]
    fn smw_and_nsmbu_walk_one_cycle_at_their_speeds() {
        for (style, v, frames) in [("MW", 0.3, 54), ("WU", 0.425, 90)] {
            let (level, mut a) = koopa(style);
            a.koopa_enter(bw::WALK, super::super::style_index(&level), false);
            assert!((a.vx + v).abs() < 1e-6, "{style} {}", a.vx);
            let mut n = 0;
            while a.phase == bw::WALK && n < 400 {
                let _ = run(&level, &mut a, 1);
                n += 1;
            }
            assert_eq!(n, frames, "{style}");
        }
    }

    #[test]
    fn nsmbu_fire_aims_one_flame_at_frame_48() {
        let (level, mut a) = koopa("WU");
        a.koopa_enter(bw::FIRE, 3, false);
        let shots = run(&level, &mut a, 48);
        let (f, fl) = shots.last().expect("a flame");
        assert_eq!(*f, 48);
        assert_eq!(fl.kind, Kind::BowserFire);
        assert!(fl.home_y.is_nan());
        // toward the player on his left, a little down to his feet + 8
        assert!(fl.vx < 0.0);
        let speed = fl.vx.dhypot(fl.vy);
        assert!((speed - 1.5).abs() < 1e-3, "{speed}");
        let a_deg = angle_deg(fl);
        assert!((-180.0..-135.0).contains(&a_deg) || (135.0..=180.0).contains(&a_deg));
        // the rest of Shoot (104 frames), then the decision
        let _ = run(&level, &mut a, 55);
        assert_eq!(a.phase, bw::FIRE);
        let _ = run(&level, &mut a, 1);
        assert_ne!(a.phase, bw::FIRE);
    }

    #[test]
    fn nsmbu_spread_fire_fans_five_flames_every_15_frames() {
        let (level, mut a) = koopa("WU");
        a.dir = 1;
        a.heading = 1.0;
        a.koopa_enter(bw::SPREAD_FIRE, 3, false);
        let shots = run(&level, &mut a, 232);
        let frames: Vec<u32> = shots.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![71, 85, 100, 115, 130]);
        let angles: Vec<i32> = shots
            .iter()
            .map(|(_, fl)| angle_deg(fl).round() as i32)
            .collect();
        assert_eq!(angles, vec![-10, 5, 20, 35, 50]);
        let _ = run(&level, &mut a, 1);
        assert_ne!(a.phase, bw::SPREAD_FIRE);
    }

    #[test]
    fn nsmbu_jump_fire_takes_off_at_26_and_fires_at_70() {
        let (level, mut a) = koopa("WU");
        a.koopa_enter(bw::JUMP_FIRE, 3, false);
        let _ = run(&level, &mut a, 25);
        assert!(a.on_ground);
        let _ = run(&level, &mut a, 1);
        assert!(!a.on_ground);
        let shots = run(&level, &mut a, 44);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].0, 44);
        // lands, then LandOn (kp_jump_ed 40)
        let mut n = 0;
        while a.phase == bw::JUMP_FIRE && n < 200 {
            let _ = run(&level, &mut a, 1);
            n += 1;
        }
        assert_eq!(a.phase, bw::LAND_ON);
        let _ = run(&level, &mut a, 39);
        assert_eq!(a.phase, bw::LAND_ON);
        let _ = run(&level, &mut a, 1);
        assert_ne!(a.phase, bw::LAND_ON);
    }

    #[test]
    fn nsmbu_big_jump_squats_59_frames_and_never_hip_drops() {
        let (level, mut a) = koopa("WU");
        a.koopa_enter(bw::BEFORE_BIG_JUMP, 3, false);
        let _ = run(&level, &mut a, 59);
        assert_eq!(a.phase, bw::BIG_JUMP);
        let _ = run(&level, &mut a, 2);
        assert!(!a.on_ground);
        assert_eq!(a.vy, 5.9);
        let mut hip = false;
        for _ in 0..200 {
            let _ = run(&level, &mut a, 1);
            hip |= a.phase == bw::HIP_DROP;
            if a.phase == bw::LAND_ON {
                break;
            }
        }
        assert!(!hip);
        assert_eq!(a.phase, bw::LAND_ON);
    }

    #[test]
    fn smw_fire_waits_10_shoots_at_10_then_waits_20() {
        let (level, mut a) = koopa("MW");
        a.koopa_enter(bw::FIRE, 2, false);
        let shots = run(&level, &mut a, 70);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].0, 20);
        // a three-lane flame
        assert!(!shots[0].1.home_y.is_nan());
        // Shoot ends 58 frames in, then Wait to its frame 20
        assert_eq!(a.phase, bw::WAIT);
        let _ = run(&level, &mut a, 7);
        assert_eq!(a.phase, bw::WAIT);
        let _ = run(&level, &mut a, 1);
        assert_ne!(a.phase, bw::WAIT);
    }

    #[test]
    fn smw_fire_falling_rains_once_all_five_flames_are_up() {
        let (level, mut a) = koopa("MW");
        a.dir = 1;
        a.heading = 1.0;
        let x0 = a.x;
        a.koopa_enter(bw::FIRE_FALLING, 2, false);
        let mut flames = Vec::new();
        let mut rain = Vec::new();
        for f in 1..=900 {
            for (_, s) in run(&level, &mut a, 1) {
                if s.phase == 2 {
                    flames.push((f, s));
                } else if s.phase == 3 {
                    rain.push((f, s));
                }
            }
        }
        // five flames up and forward at 50° … 70°
        let angles: Vec<i32> = flames
            .iter()
            .map(|(_, fl)| angle_deg(fl).round() as i32)
            .collect();
        assert_eq!(angles, vec![50, 55, 60, 65, 70]);
        let at: Vec<u32> = flames.iter().map(|(f, _)| *f).collect();
        // Upfire1 ends at 57; the next frame starts Upfire2, its frame 10 fires
        assert_eq!(at[0], 57 + 1 + 10);
        // each round: a frame to restart Upfire2, its 20 frames, 10 still
        assert!(at.windows(2).all(|w| w[1] - w[0] == 31), "{at:?}");
        // then five falling fires 30 frames apart over the columns 216 … 88 px ahead
        assert_eq!(rain.len(), 5);
        let xs: Vec<f32> = rain.iter().map(|(_, fl)| fl.x).collect();
        let col = |d: f32| ((x0 + d) / 16.0).floor() * 16.0 + 8.0;
        assert_eq!(
            xs,
            vec![col(216.0), col(184.0), col(152.0), col(120.0), col(88.0)]
        );
        assert!(rain.windows(2).all(|w| w[1].0 - w[0].0 == 30));
        assert_eq!(a.koopa.rain.state, 0);
        // a falling fire drops 1.5 px/f, lands on the floor and burns 120 frames
        let mut fl = rain[0].1.clone();
        let mut landed = None;
        for f in 0..400 {
            fl.step_boss_shot(&level);
            if fl.on_ground && landed.is_none() {
                landed = Some(f);
                assert_eq!(fl.y, 32.0);
            }
            if fl.mode == Mode::Dead {
                assert_eq!(f - landed.unwrap(), 120);
                break;
            }
        }
        assert_eq!(fl.mode, Mode::Dead);
    }

    #[test]
    fn smw_little_jump_fires_in_the_air_before_it_lands() {
        let (level, mut a) = koopa("MW");
        a.koopa_enter(bw::LITTLE_JUMP, 2, true);
        let _ = run(&level, &mut a, 4);
        assert_eq!(a.phase, bw::IN_AIR);
        assert!((a.vy - 1.1).abs() < 1e-6 || a.vy < 1.1);
        // 5 frames, then Shoot: the flame 10 frames in; LandOn only after Shoot's 48
        let shots = run(&level, &mut a, 50);
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].0, 15);
        let mut n = 0;
        while a.phase == bw::IN_AIR && n < 100 {
            let _ = run(&level, &mut a, 1);
            n += 1;
        }
        assert_eq!(a.phase, bw::LAND_ON);
        assert_eq!(a.koopa.land, 1);
    }
}
