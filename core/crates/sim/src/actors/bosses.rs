//! Bosses: Bowser (62-bowser.md), Boom Boom and Pom Pom (77-boom-boom.md) and Bowser Jr.
//! (98-bowser-jr.md). Facts are observed in the binary unless marked; the specs cite the address
//! for each one and say why the few remaining marked choices could not be read from it.
//!
//! State layout on the shared actor fields: HP in `hops`; `phase` is the state (0 = not set up
//! yet); `timer` is the actor timer (counted down at the start of every boss step, states wait
//! for it to reach 0); `pause` is a second timer (Bowser: the fire timer; Boom Boom: the action
//! timer; Bowser Jr.: the shot / Clown Car attack timer). `chain` holds small counters, one per
//! byte (see each boss). Bowser walks along `heading` (±1) while facing `dir`. `kick_lock`
//! (counted down by the generic step) is the hurt flash, and for a boss in a Clown Car also its
//! invulnerability.

use super::{Actor, Kind, MAX_FALL, Mode};
use crate::fmath::DetMath;
use crate::level::{Cell, Level, TILE};
use crate::world::{VIEW_H, VIEW_W};

mod styles;
pub use styles::BowserStyle;

/// The attacks the engine can deliver to a boss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossAttack {
    Stomp,
    /// A small player fireball.
    Fire,
    /// A sliding shell.
    Shell,
    /// A Bob-omb explosion.
    Blast,
    /// A block bumped from below under the boss.
    BlockHit,
}

/// What a hit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossHit {
    /// No effect. A stomp on Bowser is "Invalid": the touch then hurts the player.
    Ignored,
    /// Stunned or spinning: a stomp bounces off harmlessly.
    Guarded,
    /// Taken without losing HP (a fireball drained the counter; a block bumped Bowser).
    Absorbed,
    /// Lost HP.
    Hurt,
    /// HP ran out: knocked off (DieFall).
    Defeated,
}

/// The shared enemy gravity and fall cap that the bosses' states switch back to.
const G: f32 = -0.1875;
const CAP: f32 = -4.0;

/// Style index of the level: 0 SMB1, 1 SMB3, 2 SMW, 3 NSMBU (3D World uses the NSMBU rules).
fn style_index(level: &Level) -> usize {
    match level.style.as_str() {
        "M1" => 0,
        "M3" => 1,
        "MW" => 2,
        _ => 3,
    }
}

// ---------------------------------------------------------------- Bowser constants

/// HP (helper +0x18): 20, big 40.
const BOWSER_HP: u8 = 20;
const BOWSER_PATROL: f32 = 64.0;
const BOWSER_WALK: f32 = 0.3;
/// Chase-mode walk speed: SMB1 0.7, the other styles 0.9.
const BOWSER_CHASE: [f32; 2] = [0.7, 0.9];
const BOWSER_GRAVITY: f32 = -0.1;
/// Background-collision half-width per style (the "player behind" sensor), ×2 when big.
const BOWSER_BG_HALF: [f32; 4] = [13.0, 12.0, 14.0, 15.9];
/// Where a flame leaves, per style: ahead of / above Bowser's origin (×2 big).
const BOWSER_MOUTH: [(f32, f32); 4] = [(28.0, 21.0), (24.0, 24.0), (24.0, 21.0), (20.0, 24.0)];
/// The fire animation (Shoot, anim 0x2a): its length, and the frame at which the flame leaves
/// (SMB1: at its end). Lengths are the models' animations at their EnemyModelParams play rate
/// (1.0 for Shoot in every style and size): SMB1 `fire_ftp` 35, SMB3 / SMW `fire` 48, NSMBU
/// `kp_fire1` 104. Fire frames: table 0x7101c8baa0.
const BOWSER_BREATH: [(u8, u8); 4] = [(35, 35), (48, 36), (48, 10), (104, 48)];
/// The flame's own speed (BulletKoopaFire +0x183c); Bowser's forward speed is added.
const BOWSER_FIRE_SPEED: f32 = 1.5;
/// A flame's lane: 17 px (big 34) above or below the mouth, or level.
const FIRE_LANE: f32 = 17.0;
/// The Turn animation (anim 13, non-SMB1) before he turns round: the model's animation length
/// and its EnemyModelParams play rate — SMB3 `turn` 30 and SMW `turn` 45 at 2.0, NSMBU
/// `lkp_walk` 160 at 3.0; a big Bowser plays it at 1.0 in every style. Frames = length / rate,
/// rounded up. The Turn exe waits for the anim controller's end test (slot 0xc8; Turn exe
/// 0x7101157320 / 0x710115a230, the latter not in SMB1) [obs v3.0.3], so `lkp_walk` is played to
/// its end like a one-shot although its FSKA loop bit is clear like every clip of the model's
/// (walks included, which loop): the bit does not decide [inf: the end test's rounding].
const BOWSER_TURN: [(f32, f32); 4] = [(0.0, 1.0), (30.0, 2.0), (45.0, 2.0), (160.0, 3.0)];
/// The flame's carried speed is the mean of Bowser's own x steps over the last 16 frames
/// (actor ring +0x560, capacity 16 set by the enemy base 0x7100786710, filled by 0x710078adb0).
const SPEED_HISTORY: usize = 16;

/// Bowser states (numbering ours; names from the state table).
mod bw {
    pub const FALL: u8 = 1;
    pub const LAND_ON: u8 = 2;
    pub const WALK: u8 = 3;
    pub const LITTLE_JUMP: u8 = 4;
    pub const MIDDLE_JUMP: u8 = 5;
    pub const BIG_JUMP: u8 = 6;
    pub const HIP_DROP: u8 = 7;
    pub const LOOK_AROUND: u8 = 8;
    pub const TURN: u8 = 9;
    pub const FLY: u8 = 10;
    pub const HIT_BLOCK: u8 = 11;
    pub const CLOWN: u8 = 12;
    /// SMW's and NSMBU's own states (actors/bosses/styles.rs).
    pub const BEFORE_JUMP: u8 = 13;
    pub const JUMP: u8 = 14;
    pub const FIRE: u8 = 15;
    pub const JUMP_FIRE: u8 = 16;
    pub const FIRE_FALLING: u8 = 17;
    pub const BEFORE_BIG_JUMP: u8 = 18;
    pub const SPREAD_FIRE: u8 = 19;
    pub const WAIT: u8 = 20;
    pub const IN_AIR: u8 = 21;
}

/// Bowser's `chain` byte 3 flags.
mod bwf {
    pub const MODE: u8 = 0x03;
    pub const SUB: u8 = 0x0c;
    pub const BREATHING: u8 = 0x10;
    pub const FIRED: u8 = 0x20;
    pub const FELL_OFF: u8 = 0x40;
    /// Fly entered from a big jump (else from a middle jump).
    pub const FROM_BIG: u8 = 0x80;
}

// ---------------------------------------------------------------- Boom Boom constants

/// Walk speed by hits taken (0, 1, 2): table indexed by 3 − HP.
const BB_SPEED: [f32; 3] = [0.75, 0.75, 1.5];
/// HP 3 when the action timer runs out: a 2.0 dash that brakes in Defend.
const BB_CHARGE: f32 = 2.0;
const BB_SLOW: f32 = 0.35;
/// The slow walk after Defend.
const BB_SLOW_FRAMES: i32 = 60;
/// Jump: 30-frame wind-up, then vy 5.5 at 1.7 px/f, 30 frames on landing.
const BB_JUMP: f32 = 5.5;
const BB_JUMP_VX: f32 = 1.7;
const BB_WINDUP: i32 = 30;
/// WallJump: vy 3.75, the third in a row 5.0.
const BB_WALL_JUMP: f32 = 3.75;
const BB_WALL_JUMP_HIGH: f32 = 5.0;
const BB_WALL_BUMPS: u8 = 3;
const BB_DAMAGE: i32 = 70;
const BB_DEFEND_HOP: f32 = 2.75;
/// Defend brakes by 0.05/frame, waits 30 frames on the ground, then hops at 1.0 px/f.
const BB_DEFEND_BRAKE: f32 = 0.05;
const BB_DEFEND_WAIT: i32 = 30;
/// Squat and stand-up animations (12 frames each): guarded from squat frame 7 to stand-up 9.
const BB_ANIM: u8 = 12;
/// The boss forgets a player beyond 432 px.
const BB_RANGE: f32 = 432.0;
/// Winged: cruise toward a point 1.2 tiles ahead of the player and 3.5 tiles above the view
/// centre at ±2.0 px/f (0.05 px/f²), vertically ±1.0 (0.075 steps).
const BB_FLY_SPEED: f32 = 2.0;
const BB_SWOOP_ACCEL: f32 = 0.025;

mod bb {
    pub const DEFEND: u8 = 1;
    pub const WALK: u8 = 2;
    pub const JUMP: u8 = 3;
    pub const WALL_JUMP: u8 = 4;
    pub const DAMAGE_DOWN: u8 = 5;
    // winged (Wing action)
    pub const W_WAIT: u8 = 6;
    pub const W_FLY: u8 = 7;
    pub const W_ATTACK: u8 = 8;
    pub const W_DAMAGE: u8 = 9;
    pub const W_DEFEND: u8 = 10;
    pub const CLOWN: u8 = 11;
    // Pom Pom (3D World alternate)
    pub const POM_WAIT: u8 = 20;
    pub const POM_JUMP: u8 = 21;
    pub const POM_THROW_SIGN: u8 = 22;
    pub const POM_THROW: u8 = 23;
    pub const POM_LAPSE: u8 = 24;
    pub const POM_DAMAGE: u8 = 25;
    pub const POM_THROW_END: u8 = 26;
    pub const POM_HIDE: u8 = 27;
    pub const POM_LAPSE_MOVE: u8 = 28;
    pub const POM_APPEAR: u8 = 29;
}

/// Pom Pom's state lengths: the 3W_Enemy_punpun animations (play rate 1.0), [on foot, winged].
const POM_THROW_SIGN: [i32; 2] = [42, 18];
const POM_THROW: i32 = 14;
const POM_THROW_END: [i32; 2] = [37, 19];
/// After ThrowEnd's animation she waits 60 frames (0x7100757c40) before hiding.
const POM_AFTER_THROW: i32 = 60;
const POM_HIDE: i32 = 56;
const POM_LAPSE: i32 = 120;
const POM_LAPSE_MOVE: i32 = 60;
const POM_APPEAR: i32 = 58;
/// DamageDown: `PressDown` after a stomp, else `FireBallDown` then `FireBallDownEnd`.
const POM_PRESS: i32 = 59;
const POM_DAMAGE_HOP: i32 = 40;
const POM_DAMAGE_END: i32 = 76;
/// The throw target sits 11.87 px above the player's feet (0x7101c8a8d0).
const POM_AIM_UP: f32 = 11.871_2;

// ---------------------------------------------------------------- Bowser Jr. constants

const JR_WAIT: i32 = 20;
const JR_WALK_FRAMES: i32 = 120;
/// Walk speed: 1.25 px/f, 0.8 with wings, in 0.1 steps; stops 24 px from home.
const JR_WALK: f32 = 1.25;
const JR_WALK_WINGS: f32 = 0.8;
const JR_WALK_STEP: f32 = 0.1;
const JR_TURN_RANGE: f32 = 24.0;
/// Wait's look-ahead before a hop (80 in water).
const JR_PROBE: f32 = 30.0;
/// Shift jump: straight up at 5.5 (big 6.0); at vy ≤ 0.4 the hip drop starts (winged: Hover).
const JR_SHIFT_JUMP: f32 = 5.5;
const JR_SHIFT_JUMP_BIG: f32 = 6.0;
const JR_HOVER_AT: f32 = 0.4;
/// The Hop and ShiftJump wind-up before the stored jump starts (anim tables at 0x7100186f90:
/// none in SMB1, JumpSt in SMB3 / SMW, QuakeSt / JumpSt in NSMBU): SMB3 / SMW `squat` 4
/// frames, NSMBU `bl_quake_st` / `bl_jump_st` 25, all at play rate 1.0 (EnemyModelParams).
const JR_WINDUP: [i32; 4] = [0, 4, 4, 25];
/// `jr_sub` value while a wind-up plays.
const JR_WINDING: u8 = 2;
const JR_HIP_GRAVITY: f32 = -0.75;
const JR_HIP_CAP: f32 = -8.0;
const JR_HIP_LAND: i32 = 50;
/// DamageDown (non-NSMBU): bounce vy 3.0, 15 frames.
const JR_DAMAGE: i32 = 15;
/// The Shell context (BossBase Shell, states 0x710216e4a0): ShellWait 20 frames still; its
/// exit arms a 200-frame slide counter (0x7100614a80) that ShellSlide and ShellJump count
/// down. ShellSlide (0x7100614b10) ramps its acceleration to 0.1 by 0.02 per frame and its
/// speed to 1.6 by that; a player behind him or the area edge (16 px) makes him brake by 0.1
/// per frame (0.3 above 1.6) and turn, with 10 frames before the next turn; a wall bounces
/// the speed back. Once the counter is spent and he stands on the floor, ShellWakeup.
const JR_SHELL_WAIT: i32 = 20;
const JR_SHELL_FRAMES: i32 = 200;
const JR_SHELL_TOP: f32 = 1.6;
const JR_SHELL_ACCEL: f32 = 0.1;
const JR_SHELL_JERK: f32 = 0.02;
const JR_SHELL_TURN: u8 = 10;
/// ShellSlide sub-phases (+0x90): 1 settle and aim, 2 slide, 3 the obstacle hop, 4 brake and
/// turn.
const SL_AIM: u8 = 1;
const SL_SLIDE: u8 = 2;
const SL_HOP: u8 = 3;
const SL_BRAKE: u8 = 4;
/// ShellSlide's hop over an obstacle: vy 3.0 at 1.5 px/f, gravity −0.1875 [obs v3.0.3
/// 0x710108aa50]; the obstacle analysis looks h = 16 px over his feet + 8 (32 under the
/// water-speed rule, 0x7102318d6c).
const JR_SHELL_HOP_VY: f32 = 3.0;
const JR_SHELL_HOP_VX: f32 = 1.5;
/// ShellWakeup (0x7100617170): hops out at vy 3.0; at vy ≤ 0.4 the Basic action resumes.
const JR_WAKE_HOP: f32 = 3.0;
const JR_WAKE_END: f32 = 0.4;
/// Fire: 3 shots, each one 26 frames into a 60-frame animation.
const JR_FIRE_ANIM: i32 = 60;
const JR_FIRE_AT: i32 = 26;
/// Hammers (winged): 4 thrown 36, 52, 68 and 84 frames into a 110-frame animation.
const JR_THROW_ANIM: i32 = 110;
const JR_THROW_AT: [i32; 4] = [36, 52, 68, 84];
/// A fireball is aimed at the player but stays within 25° of level.
const JR_AIM: f32 = 25.0;

mod jr {
    pub const WAIT: u8 = 1;
    pub const FALL: u8 = 2;
    pub const HOP: u8 = 3;
    pub const WALK: u8 = 4;
    pub const SHOT: u8 = 5;
    pub const SHIFT_JUMP: u8 = 6;
    pub const HOVER: u8 = 7;
    pub const PREV_HIP_DROP: u8 = 8;
    pub const HIP_DROP: u8 = 9;
    pub const HIP_LAND: u8 = 10;
    pub const DAMAGE_DOWN: u8 = 11;
    pub const SHELL_WAIT: u8 = 12;
    pub const SHELL: u8 = 13;
    pub const SHELL_OUT: u8 = 14;
    // Clown Car machine
    pub const CLOWN: u8 = 15;
    pub const C_PRE_THROW: u8 = 16;
    pub const C_THROW: u8 = 17;
    pub const C_POST_THROW: u8 = 18;
    pub const C_SHOT: u8 = 19;
    pub const C_DAMAGE: u8 = 20;
}

/// Bowser Jr.'s Clown DamageDown (0x71006e8fe0 / 0x71006e9130, outside 3D World): 4 + 18
/// frames, then he ducks into the car (y scale −0.1 per frame to 0.01, 10 frames), stays down
/// 36 frames and comes back up (to 0.9 at +0.1, 9 frames) → Wait. Guarded throughout.
const JR_CLOWN_DAMAGE: i32 = 4 + 18 + 10 + 36 + 9;
/// Clown Wait (0x71006e84b0): once the player has been near, frames away from him count down
/// from 150, clearing the attack timer at each wrap (the next approach attacks at once).
const JR_CLOWN_FORGET: u8 = 150;

/// Bowser Jr. in a Clown Car: an attack every 150 frames while the player is near.
const JR_CLOWN_ATTACK: i32 = 150;
/// Clown Car Chase steering (0x71006dd810): top speed 1.0 px/f, acceleration 0.02 px/f².
const CLOWN_SPEED: f32 = 1.0;
const CLOWN_ACCEL: f32 = 0.02;

impl Kind {
    pub fn is_boss(self) -> bool {
        matches!(
            self,
            Kind::Bowser { .. } | Kind::BoomBoom { .. } | Kind::BowserJr { .. }
        )
    }

    /// Rides a Koopa Clown Car (flag 0x200).
    pub fn boss_clown(self) -> bool {
        match self {
            Kind::Bowser { clown, .. }
            | Kind::BoomBoom { clown, .. }
            | Kind::BowserJr { clown, .. } => clown,
            _ => false,
        }
    }
}

impl Actor {
    fn byte(&self, n: u32) -> u8 {
        (self.chain >> (8 * n)) as u8
    }

    fn set_byte(&mut self, n: u32, v: u8) {
        self.chain = (self.chain & !(0xff << (8 * n))) | ((v as u32) << (8 * n));
    }

    fn toward_target(&self) -> i32 {
        if self.target.0 < self.x { -1 } else { 1 }
    }

    fn scale(&self) -> f32 {
        if self.big { 2.0 } else { 1.0 }
    }

    fn fall(&mut self, level: &Level, g: f32) {
        self.fall_capped(level, g, MAX_FALL);
    }

    fn fall_capped(&mut self, level: &Level, g: f32, cap: f32) {
        self.vy = (self.vy + g).max(cap);
        self.collide(level);
    }

    /// Standing at a drop: the floor ends just ahead in direction `dir`.
    fn ledge_ahead(&self, level: &Level, dir: f32) -> bool {
        if !self.on_ground {
            return false;
        }
        let px = self.x + dir * (self.half_w() + 0.1);
        let slope = level.slope_at(px).is_some_and(|(s, _)| s >= self.y - 8.0);
        level.cell_at(px, self.y - 1.0) == Cell::Empty && !slope
    }

    /// The boss's box overlaps the camera view (actor flag 0x510 bit 3).
    fn in_view(&self) -> bool {
        let (x0, y0, x1, y1) = self.bbox();
        let (cx, cy) = self.camera;
        x1 > cx && x0 < cx + VIEW_W && y1 > cy && y0 < cy + VIEW_H
    }

    fn view_centre(&self) -> (f32, f32) {
        (self.camera.0 + VIEW_W / 2.0, self.camera.1 + VIEW_H / 2.0)
    }

    /// Moves `v` toward `target` by at most `step`.
    fn approach(v: f32, target: f32, step: f32) -> f32 {
        v + (target - v).clamp(-step, step)
    }

    /// Hit points left (0 before the first step).
    pub fn boss_hp(&self) -> u8 {
        self.hops
    }

    /// The fireball counter of Boom Boom / Bowser Jr.
    pub fn boss_fire_counter(&self) -> u8 {
        self.byte(0)
    }

    /// Removes the held Key (if any) so it drops once; true if there was one.
    pub fn take_boss_key(&mut self) -> bool {
        match &mut self.kind {
            Kind::Bowser { key, .. } | Kind::BoomBoom { key, .. } | Kind::BowserJr { key, .. } => {
                std::mem::take(key)
            }
            _ => false,
        }
    }

    /// Pom Pom has vanished (Lapse): no contact at all.
    pub fn boss_hidden(&self) -> bool {
        matches!(self.kind, Kind::BoomBoom { pom: true, .. })
            && matches!(self.phase, bb::POM_LAPSE | bb::POM_LAPSE_MOVE)
    }

    /// Immune; stomps bounce off. Boom Boom: from DamageDown (or the start of Defend) until 9
    /// frames into the stand-up hop. Bowser Jr.: DamageDown and the shell (the Shell context's
    /// damage slot 0x7100614380 takes no HP for the attacks the engine delivers; see
    /// `jr_shell_hit`).
    pub fn boss_guarding(&self) -> bool {
        if self.kind.boss_clown() && self.kick_lock > 0 {
            return true;
        }
        match self.kind {
            Kind::BoomBoom { pom: false, .. } => {
                matches!(self.phase, bb::DAMAGE_DOWN | bb::W_DAMAGE) || self.bb_guard()
            }
            Kind::BoomBoom { pom: true, .. } => self.phase == bb::POM_DAMAGE,
            Kind::BowserJr { .. } => matches!(
                self.phase,
                jr::DAMAGE_DOWN | jr::SHELL_WAIT | jr::SHELL | jr::SHELL_OUT | jr::C_DAMAGE
            ),
            _ => false,
        }
    }

    /// Bowser Jr. tucked into his shell (drawn as the shell).
    pub fn boss_in_shell(&self) -> bool {
        matches!(self.kind, Kind::BowserJr { .. })
            && matches!(self.phase, jr::SHELL_WAIT | jr::SHELL | jr::SHELL_OUT)
    }

    /// Bowser is breathing fire.
    pub fn boss_breathing(&self) -> bool {
        matches!(self.kind, Kind::Bowser { .. }) && self.byte(2) > 0
    }

    /// Animation hints for the renderer: the state number (see the state modules here).
    pub fn boss_state(&self) -> u8 {
        self.phase
    }

    fn boss_setup(&mut self, level: &Level) {
        let st = style_index(level);
        match self.kind {
            Kind::Bowser { clown, .. } => {
                self.hops = if self.big { 2 * BOWSER_HP } else { BOWSER_HP };
                self.home_x = self.x;
                self.dir = self.toward_target();
                self.heading = self.dir as f32;
                self.chain = 0;
                self.pause = self.bowser_fire_interval();
                self.bowser_reset_big(st);
                self.bowser_reset_jump(st);
                if clown {
                    self.phase = bw::CLOWN;
                } else if self.target.0 <= self.x && !self.in_view() {
                    // waits off screen (mode 3) until it scrolls into view
                    self.bowser_set_mode(3, st);
                } else if !self.on_ground {
                    self.bowser_enter(bw::FALL, st);
                } else if self.bowser_player_ahead_right(st) {
                    self.bowser_chase_dispatch(level, st);
                } else {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            Kind::BoomBoom { clown, pom, .. } => {
                self.hops = 3;
                self.set_byte(0, if self.big { 10 } else { 5 });
                self.pause = 0;
                if pom {
                    self.phase = bb::POM_WAIT;
                    self.timer = 60;
                } else if clown {
                    self.phase = bb::CLOWN;
                } else if self.wings {
                    self.phase = bb::W_WAIT;
                } else {
                    // a fresh Boom Boom (3 HP) starts in Defend
                    self.bb_enter_defend(false);
                }
            }
            Kind::BowserJr { clown, .. } => {
                self.hops = 3;
                self.set_byte(0, if self.big { 10 } else { 5 });
                self.home_x = self.x;
                self.pause = 0;
                self.jr_rearm_cooldown();
                if clown {
                    // the Clown Wait's "has been near" flag and away counter (bytes 1, 2)
                    self.set_byte(1, 0);
                    self.set_byte(2, 0);
                    self.phase = jr::CLOWN;
                } else {
                    self.jr_enter_wait_or_fall();
                }
            }
            _ => {}
        }
    }

    /// A hit from `attack` (coming from `from_x`), through the boss HP rules.
    ///
    /// Bowser (helper 0x71006b4c20): fire 1, shell 3, Bob-omb blast 3 (the blast is the shell
    /// attack: while the Explode flag is up the collider update swaps the bomb's body box for a
    /// cc 6 box [obs v1.0.0 0x7100732640, world.rs `explosions`]; the v3.0.3 Mechakoopa missile's
    /// blast profile 0x7102a62d28 is attack type 6 too [obs]); a block hit only
    /// bumps him (HitBlock); stomps are Invalid. Boom Boom / Bowser Jr. (boss helper
    /// 0x71006116d0): a fireball drains the counter (5, big 10) and costs 1 HP when it empties
    /// (then refills); every other hit costs 1 HP; none while guarded. Star, POW quake and lava
    /// kill outright and are handled by the generic paths.
    pub fn boss_hit(&mut self, attack: BossAttack, from_x: f32) -> BossHit {
        if !self.alive() || !self.kind.is_boss() || self.boss_hidden() {
            return BossHit::Ignored;
        }
        if self.phase == 0 {
            // hit before its first step: set up with the SMB1 rules
            let mut lvl = Level::flat(1);
            lvl.style = "M1".into();
            self.boss_setup(&lvl);
        }
        if let Kind::Bowser { .. } = self.kind {
            let dmg: u8 = match attack {
                BossAttack::Stomp => return BossHit::Ignored,
                BossAttack::Fire => 1,
                BossAttack::Shell | BossAttack::Blast => 3,
                BossAttack::BlockHit => {
                    if self.phase != bw::CLOWN {
                        self.bowser_enter_hit_block();
                    }
                    return BossHit::Absorbed;
                }
            };
            self.hops = self.hops.saturating_sub(dmg);
            if self.hops == 0 {
                self.die_fall(from_x);
                return BossHit::Defeated;
            }
            self.kick_lock = 16;
            return BossHit::Hurt;
        }
        if matches!(self.kind, Kind::BowserJr { .. })
            && matches!(self.phase, jr::SHELL_WAIT | jr::SHELL | jr::SHELL_OUT)
        {
            return self.jr_shell_hit(attack);
        }
        if self.boss_guarding() {
            return if attack == BossAttack::Stomp {
                BossHit::Guarded
            } else {
                BossHit::Ignored
            };
        }
        if attack == BossAttack::Fire {
            let n = self.byte(0).saturating_sub(1);
            if n > 0 {
                self.set_byte(0, n);
                self.kick_lock = 8;
                return BossHit::Absorbed;
            }
            self.set_byte(0, if self.big { 10 } else { 5 });
        }
        self.hops = self.hops.saturating_sub(1);
        if self.hops == 0 {
            self.die_fall(from_x);
            return BossHit::Defeated;
        }
        self.boss_damage_state(attack);
        BossHit::Hurt
    }

    /// A hit on Bowser Jr. in his shell: the Shell context's own damage slot (0x7100614380).
    /// Fire (and shoe smoke) does nothing; a stomp bounces the player off (vy 3.91, reaction
    /// 5); a block bump (also tail, cape, climb attacks) knocks the shell up into ShellJump
    /// (vy 2.0, gravity −0.1875; not during ShellWakeup); the heel ground pound and attacks
    /// 33 / 34 go to the boss damage (and kill); every other hit only plays the hurt reaction.
    fn jr_shell_hit(&mut self, attack: BossAttack) -> BossHit {
        match attack {
            BossAttack::Stomp => BossHit::Guarded,
            BossAttack::BlockHit if self.phase != jr::SHELL_OUT => {
                if self.phase == jr::SHELL_WAIT {
                    // leaving ShellWait arms the slide counter
                    self.phase = jr::SHELL;
                    self.pause = JR_SHELL_FRAMES;
                    self.speed = 0.0;
                    self.heading = 0.0;
                }
                // ShellJump lands back in the slide, aimed at the player
                self.jr_set_sub(SL_AIM);
                self.vy = 2.0;
                self.on_ground = false;
                BossHit::Absorbed
            }
            _ => BossHit::Ignored,
        }
    }

    /// After losing HP: Boom Boom DamageDown (winged: PataDamageDown, keeping its wings), Bowser
    /// Jr. DamageDown. In a Clown Car Bowser Jr. stays aboard (his helper's damage hand-off
    /// 0x7100619040 skips the Clown context) and plays the Clown DamageDown. Boom Boom's
    /// hand-off (0x71006368d0) has no such check: the game moves him to his Basic / Wing action
    /// and so out of the car [?: not modelled — the engine's car is part of its rider; he only
    /// flashes and stays aboard].
    fn boss_damage_state(&mut self, attack: BossAttack) {
        if let Kind::BowserJr { clown: true, .. } = self.kind {
            self.phase = jr::C_DAMAGE;
            self.timer = JR_CLOWN_DAMAGE;
            return;
        }
        if self.kind.boss_clown() {
            self.kick_lock = 60;
            return;
        }
        self.vx = 0.0;
        match self.kind {
            Kind::BoomBoom { pom: true, .. } => {
                // 0x7100757780: stomp-type attacks (0–3) PressDown in place, else a 2.5 hop
                self.phase = bb::POM_DAMAGE;
                self.trail.clear();
                if attack == BossAttack::Stomp {
                    self.timer = POM_PRESS;
                    self.set_byte(1, 0);
                } else {
                    self.vy = 2.5;
                    self.on_ground = false;
                    self.timer = POM_DAMAGE_HOP;
                    self.set_byte(1, 1);
                }
            }
            Kind::BoomBoom { .. } => {
                self.phase = if self.wings {
                    bb::W_DAMAGE
                } else {
                    bb::DAMAGE_DOWN
                };
                self.vy = 0.0;
                self.timer = BB_DAMAGE;
            }
            Kind::BowserJr { .. } => {
                self.phase = jr::DAMAGE_DOWN;
                self.timer = JR_DAMAGE;
                self.vy = 3.0;
                self.on_ground = false;
            }
            _ => {}
        }
    }

    /// How far above the car's origin a boss rides (car-side placement 0x71006d6ca0: the car
    /// position minus the rider's ride offset +0x838, less 16 px per extra unit of car scale).
    /// Bowser Jr. and Boom Boom set −21 in their Clown entries (0x71006e7b30, 0x7100638910);
    /// Bowser takes his from the generic Clown context (0x71007961e0) out of EnemyModelParams
    /// [inf: the field whose values match, −36 SMB1, −38 SMB3 / SMW, −40 NSMBU].
    pub fn clown_ride_height(&self, style: &str) -> f32 {
        let base = match self.kind {
            Kind::Bowser { .. } => match style {
                "M1" => 36.0,
                "M3" | "MW" => 38.0,
                _ => 40.0,
            },
            _ => 21.0,
        };
        base + 16.0 * (self.scale() - 1.0)
    }

    /// The Koopa Clown Car under a boss (Chase, 0x71006dd810). Within 320 px across and 800 px
    /// up or down of the player (0x71006e0f30) it accelerates by 0.02 px/f² (winged rider:
    /// 0.03, up to 2.0) toward the player's centre and is held to 1.0 px/f; out of that range
    /// it drifts as Free does (0x71006dd230: speed × 0.95, then a ±0.2 vertical bob in 0.01
    /// steps once below 0.3). The rider faces the player when the player is more than 16 px
    /// away. A Fire car (`clown_fire`) halves the top speed, takes 0.75 of the acceleration,
    /// charges and fires (actors/clown.rs: the lean / brake phases once read as Bowser Jr.'s
    /// +0x52c are the Fire car's charge, +0x540 in v3.0.3). Not modelled: water (acceleration
    /// × 0.75) and the damping on contact.
    fn step_clown(&mut self, level: &Level) {
        let ride = self.clown_ride_height(&level.style);
        let (dx, dy) = (
            self.target.0 - self.x,
            self.target.1 + 8.0 - (self.y - ride),
        );
        if dx.abs() <= 320.0 && dy.abs() <= 800.0 {
            let steer = if self.wings {
                (2.0, 0.03)
            } else {
                (CLOWN_SPEED, CLOWN_ACCEL)
            };
            // a Fire car (`clown_fire`) also charges and shoots (actors/clown.rs)
            let car = (self.x, self.y - ride);
            let turn = self.clown_chase((dx, dy), steer, car, self.scale());
            if turn && dx.dhypot(dy) > 16.0 && dx != 0.0 {
                self.dir = if dx < 0.0 { -1 } else { 1 };
            }
            self.heading = 0.0;
        } else {
            self.clown_fire_idle();
            self.vx *= 0.95;
            if self.vx.dhypot(self.vy) < 0.3 {
                // the bob: toward +0.2 or −0.2 by 0.01, turning at each end
                if self.heading == 0.0 {
                    self.heading = if self.vy < 0.0 { -1.0 } else { 1.0 };
                }
                let goal = 0.2 * self.heading;
                self.vy = Self::approach(self.vy, goal, 0.01);
                if self.vy == goal {
                    self.heading = -self.heading;
                }
            } else {
                self.vy *= 0.95;
            }
        }
        self.collide(level);
    }

    /// Flames and shuriken: straight flight through terrain. A Bowser flame with a lane
    /// (`home_y`) climbs or dives at half its forward speed until it is within half a step of
    /// the lane, then flies level (BulletKoopaFire ThreeWayFire 0x71003b2380).
    pub(super) fn step_boss_shot(&mut self, level: &Level) {
        if self.kind == Kind::BowserFire && self.koopa_flame_step(level) {
            return;
        }
        if self.kind == Kind::Shuriken {
            // BulletPunpunShuriken Move (0x71003b9490): while the target (home) is still ahead
            // it turns toward it by at most 2° a frame, staying within 45° of level; speed 1.0
            // (big 1.5), `heading` is its climb angle in degrees
            let d = self.dir as f32;
            let (dx, dy) = (self.home_x - self.x, self.home_y - self.y);
            if dx * d > 0.0 {
                let want = dy.datan2(dx.abs()).to_degrees();
                self.heading = Self::approach(self.heading, want, 2.0).clamp(-45.0, 45.0);
            }
            let (sn, cs) = self.heading.to_radians().dsin_cos();
            self.vx = cs * self.speed * d;
            self.vy = sn * self.speed;
        }
        if self.kind == Kind::BowserFire
            && !self.home_y.is_nan()
            && (self.y - self.home_y).abs() <= (self.vy * 0.5).abs()
        {
            self.vy = 0.0;
        }
        self.x += self.vx;
        self.y += self.vy;
        self.timer -= 1;
        if self.timer <= 0 {
            self.mode = Mode::Dead;
        }
    }

    // ------------------------------------------------------------ Bowser

    fn bowser_mode(&self) -> u8 {
        self.byte(3) & bwf::MODE
    }

    fn bowser_flag(&self, f: u8) -> bool {
        self.byte(3) & f != 0
    }

    fn bowser_set_flag(&mut self, f: u8, on: bool) {
        let b = self.byte(3);
        self.set_byte(3, if on { b | f } else { b & !f });
    }

    fn bowser_sub(&self) -> u8 {
        (self.byte(3) & bwf::SUB) >> 2
    }

    fn bowser_set_sub(&mut self, s: u8) {
        let b = self.byte(3);
        self.set_byte(3, (b & !bwf::SUB) | ((s << 2) & bwf::SUB));
    }

    /// Frames the Turn animation takes (SMB1: none).
    fn bowser_turn_frames(&self, st: usize) -> f32 {
        let (len, rate) = BOWSER_TURN[st];
        let rate = if self.big { 1.0 } else { rate };
        (len / rate).ceil()
    }

    /// Records this frame's position for the speed history (16 steps kept).
    fn bowser_record_step(&mut self) {
        self.trail.push((self.x, self.y));
        if self.trail.len() > SPEED_HISTORY + 1 {
            self.trail.remove(0);
        }
    }

    /// The mean of his own x steps over the last 16 frames (the speed history).
    fn bowser_mean_speed(&self) -> f32 {
        match (self.trail.first(), self.trail.last()) {
            (Some(a), Some(b)) if self.trail.len() > 1 => {
                (b.0 - a.0) / (self.trail.len() - 1) as f32
            }
            _ => 0.0,
        }
    }

    fn bowser_bg_half(&self, st: usize) -> f32 {
        BOWSER_BG_HALF[st] * self.scale()
    }

    /// Fire interval: 40 + random(0…39) frames; chase mode 60 + random(0…39); off-screen
    /// mode 40 + random(0…239).
    fn bowser_fire_interval(&mut self) -> i32 {
        match self.bowser_mode() {
            3 => 40 + self.rand(240) as i32,
            2 => 60 + self.rand(40) as i32,
            _ => 40 + self.rand(40) as i32,
        }
    }

    /// Re-arms the fire timer and forgets any breath (helper +0x8 / +0x10 / +0x12).
    fn bowser_rearm_fire(&mut self) {
        self.pause = self.bowser_fire_interval();
        self.bowser_set_flag(bwf::FIRED | bwf::BREATHING, false);
    }

    /// Jump counter (the decision's slot 0x370), [low, high) decisions by style and mode (0
    /// normal, 1 / 2 the chases, 3 off screen) [obs v3.0.3]: SMB1 / SMB3 (0x710115ac00) SMB1
    /// 3–4, SMB3 1–3, in modes 1–3 4 + the same random part; SMW (0x7101157fa0) 1–3, mode 1
    /// 1–2, mode 2 3–4; NSMBU (0x7101152cb0, tables 0x71023cd664 / 0x71023cd670) 1–3, mode 1
    /// 1–2, mode 2 4–5, mode 3 1–3.
    fn bowser_reset_jump(&mut self, st: usize) {
        let mode = self.bowser_mode();
        let (lo, hi) = match st {
            0 | 1 => {
                let (base, span) = if st == 0 { (3, 2) } else { (1, 3) };
                let base = if mode != 0 { 4 } else { base };
                (base, base + span)
            }
            2 => match mode {
                1 => (1, 3),
                2 => (3, 5),
                _ => (1, 4),
            },
            _ => match mode {
                1 => (1, 3),
                2 => (4, 6),
                _ => (1, 4),
            },
        };
        let j = (lo + self.rand(hi - lo)) as u8;
        self.set_byte(0, j);
    }

    /// Big-jump counter (slot 0x378) [obs v3.0.3]: SMB1 / SMB3 (0x710115aca0) 5 or 6 decisions,
    /// 9 or 10 in modes 1–3; SMW (0x7101158030, tables 0x71023cd67c / 0x71023cd688) 5–8, mode 1
    /// 8–10, mode 2 9–11, mode 3 7–9; NSMBU (0x7101152d40, 0x71023cd64c / 0x71023cd658) 5–8,
    /// mode 1 8–10, modes 2 and 3 7–9.
    fn bowser_reset_big(&mut self, st: usize) {
        let mode = self.bowser_mode();
        let (lo, hi) = match (st, mode) {
            (0 | 1, 0) => (5, 7),
            (0 | 1, _) => (9, 11),
            (_, 0) => (5, 9),
            (_, 1) => (8, 11),
            (2, 2) => (9, 12),
            _ => (7, 10),
        };
        let b = (lo + self.rand(hi - lo)) as u8;
        self.set_byte(1, b);
    }

    /// The walk-speed setter (0x71006b1d20): 0.3 px/f along `heading`; chase mode 0.7 (SMB1)
    /// / 0.9; off-screen mode 0.
    fn bowser_walk_speed(&mut self, st: usize) {
        if st >= 2 {
            return self.koopa_walk_speed(st);
        }
        self.vx = match self.bowser_mode() {
            2 => BOWSER_CHASE[(st != 0) as usize] * self.heading,
            3 => 0.0,
            _ => BOWSER_WALK * self.heading,
        };
    }

    /// The player is behind him: past the back edge of his background box.
    fn bowser_player_behind(&self, st: usize) -> bool {
        let w = self.bowser_bg_half(st);
        if self.dir > 0 {
            self.target.0 < self.x - w
        } else {
            self.x + w < self.target.0
        }
    }

    /// Facing right with the player past his front edge (the cue to start chasing).
    fn bowser_player_ahead_right(&self, st: usize) -> bool {
        self.dir > 0 && self.on_ground && self.x + self.bowser_bg_half(st) < self.target.0
    }

    /// Animation-replacing state entries end a breath in progress; the fire timer then stays
    /// at 0 until something re-arms it (non-SMB1 Turn, LookAround, BigJump, HipDrop).
    fn bowser_cancel_breath(&mut self) {
        self.set_byte(2, 0);
    }

    fn bowser_enter(&mut self, state: u8, st: usize) {
        if st >= 2
            && matches!(
                state,
                bw::WALK | bw::LAND_ON | bw::LITTLE_JUMP | bw::MIDDLE_JUMP | bw::BIG_JUMP
            )
        {
            return self.koopa_enter(state, st, false);
        }
        let m1 = st == 0;
        self.phase = state;
        match state {
            bw::FALL => {
                if self.bowser_mode() == 3 {
                    let b = self.byte(3) & !bwf::MODE;
                    self.set_byte(3, b);
                }
            }
            bw::LAND_ON => {
                self.vx = 0.0;
                self.timer = if m1 {
                    0
                } else if matches!(self.bowser_mode(), 1 | 2) {
                    10
                } else {
                    15
                };
            }
            bw::WALK => {
                self.bowser_walk_speed(st);
                let r = 20 + self.rand(20) as i32;
                self.timer = if self.bowser_mode() == 2 && self.toward_target() != self.dir {
                    20
                } else {
                    r
                };
            }
            bw::LITTLE_JUMP => {
                // SMB1 / SMB3 (the Koopa / KoopaM3 action, vtable 0x71028e44b8): vy 1.1 (slot
                // 0x3a8), but 0.7 — 0.35 under the water-speed rule — when the jump-height
                // search found a Trampoline no higher than the hop's own reach (+0x94, in
                // water +0x98) [obs v3.0.3 LittleJump enter 0x7101158ff0]. SMW's LittleJump
                // (0x71011566e0) is a JumpSt wind-up and NSMBU's (0x7101150240) is empty; the
                // engine keeps this hop for them [inf].
                let low = st <= 1
                    && self
                        .spring_over
                        .is_some_and(|h| h <= self.spring_reach_little);
                self.vy = match (low, self.spring_slow) {
                    (false, _) => 1.1,
                    (true, false) => 0.7,
                    (true, true) => 0.35,
                };
                self.on_ground = false;
                self.bowser_walk_speed(st);
                self.timer = 3;
            }
            bw::MIDDLE_JUMP => {
                self.vy = if self.wings { 3.4 } else { 3.0 };
                self.on_ground = false;
                self.bowser_walk_speed(st);
                self.timer = 3;
                // (a gap jump sets its own speed and flag after this, bowser_chase_jumps)
                self.gap_jump = false;
            }
            bw::BIG_JUMP => {
                self.vx = 0.0;
                self.timer = 25;
                self.bowser_set_sub(0);
                self.bowser_cancel_breath();
            }
            bw::HIP_DROP => {
                self.vx = 0.0;
                // 1.0 under a Trampoline less than his height + 16 over him (HipDrop enter
                // 0x7101159750) [obs v3.0.3]
                self.vy = if self
                    .spring_over
                    .is_some_and(|h| h <= self.spring_reach_body)
                {
                    1.0
                } else {
                    3.0
                };
                self.timer = 0;
                self.bowser_set_sub(0);
                self.bowser_cancel_breath();
            }
            bw::LOOK_AROUND => {
                self.timer = 40;
                self.speed = -1.0;
                self.bowser_cancel_breath();
            }
            bw::TURN => {
                self.vx = 0.0;
                self.speed = 0.0;
                if !m1 {
                    self.bowser_cancel_breath();
                }
            }
            bw::FLY => {
                self.bowser_set_sub(0);
            }
            _ => {}
        }
    }

    /// HitBlock (a block bumped under him): hop vy 2.2 at +0.3 px/f (−0.3 in modes 1 and 3,
    /// the sign as stored by 0x71006a6b10), fire re-armed.
    fn bowser_enter_hit_block(&mut self) {
        self.phase = bw::HIT_BLOCK;
        self.vy = 2.2;
        self.on_ground = false;
        self.timer = 2;
        self.vx = if matches!(self.bowser_mode(), 1 | 3) {
            -0.3
        } else {
            0.3
        };
        self.bowser_rearm_fire();
        self.bowser_cancel_breath();
    }

    /// Mode change (0x71006b18c0): new mode, jump counter and fire timer for it, heading
    /// right, LandOn.
    fn bowser_set_mode(&mut self, mode: u8, st: usize) {
        let b = (self.byte(3) & !bwf::MODE) | (mode & bwf::MODE);
        self.set_byte(3, b);
        self.bowser_reset_jump(st);
        self.bowser_rearm_fire();
        self.heading = 1.0;
        self.bowser_enter(bw::LAND_ON, st);
    }

    /// Bowser (62-bowser.md, shared Koopa action). Walk (SMB1) or little hops (other styles)
    /// between decisions; a decision keeps him within ±64 px of home, turns him to face the
    /// player, and counts down to a middle jump (vy 3.0; winged 3.4 then a flutter) or, outside
    /// SMB1, a big jump (25-frame wind-up, vy 5.9; SMB3 6.2 aimed at the player, then a hip
    /// drop). Once the player gets past him he chases (0.7 / 0.9 px/f). Off screen with the
    /// player on his left he waits in place. The fire timer runs alongside in the ground and
    /// jump states: every 40–79 frames a breath (35 frames in SMB1), then a flame.
    pub(super) fn step_bowser(&mut self, level: &Level) -> Option<Actor> {
        if self.phase == 0 {
            self.boss_setup(level);
        }
        let st = style_index(level);
        if self.timer > 0 {
            self.timer -= 1;
        }
        // the fire animation plays on whatever the state does
        let b = self.byte(2);
        if b > 0 && b < u8::MAX {
            self.set_byte(2, b + 1);
        }
        if self.phase == bw::CLOWN {
            self.step_clown(level);
            self.bowser_record_step();
            return self.bowser_fire(st);
        }
        let state = self.phase;
        self.spring_over = self.bowser_height_search(level);
        if st < 2 || !self.koopa_move(level, st) {
            self.bowser_move(level, st);
        }
        if st == 2 {
            self.koopa_rain();
        }
        self.bowser_record_step();
        // the fire helper runs in Fall, LandOn, Walk, LittleJump, MiddleJump and a Fly that
        // came from a middle jump (0x71006b1de0)
        let fire_state = match state {
            bw::FALL | bw::LAND_ON | bw::WALK | bw::LITTLE_JUMP | bw::MIDDLE_JUMP => true,
            bw::FLY => !self.bowser_flag(bwf::FROM_BIG),
            _ => false,
        };
        if fire_state {
            self.bowser_fire(st)
        } else {
            self.bowser_set_flag(bwf::BREATHING, false);
            None
        }
    }

    /// Leaving a ground state when the floor is gone (walked off a ledge).
    fn bowser_ground_lost(&mut self, st: usize) -> bool {
        if self.on_ground {
            return false;
        }
        self.bowser_set_flag(bwf::FELL_OFF, true);
        self.bowser_enter(bw::FALL, st);
        true
    }

    fn bowser_move(&mut self, level: &Level, st: usize) {
        let m1 = st == 0;
        let m3 = st == 1;
        let breathing = self.bowser_flag(bwf::BREATHING);
        match self.phase {
            bw::FALL => {
                self.fall_capped(level, G, CAP);
                if self.bowser_flag(bwf::FELL_OFF) && self.timer == 0 && self.wall {
                    self.heading = -self.heading;
                    self.vx = self.vx.abs() * self.heading;
                    self.timer = 8;
                }
                if self.on_ground {
                    self.bowser_set_flag(bwf::FELL_OFF, false);
                    self.home_x = self.x;
                    if self.bowser_player_ahead_right(st) {
                        self.bowser_chase_dispatch(level, st);
                    } else {
                        self.bowser_enter(bw::LAND_ON, st);
                    }
                }
            }
            bw::LAND_ON => {
                self.fall(level, BOWSER_GRAVITY);
                if !self.bowser_ground_lost(st) && self.timer == 0 {
                    self.bowser_decide(level, st);
                }
            }
            bw::WALK => {
                self.bowser_walk_speed(st);
                if breathing {
                    self.vx = 0.0;
                }
                self.fall(level, BOWSER_GRAVITY);
                if !self.bowser_ground_lost(st) && self.timer == 0 {
                    self.bowser_decide(level, st);
                }
            }
            bw::LITTLE_JUMP => {
                self.fall(level, -0.07);
                if self.timer == 0 && self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            bw::MIDDLE_JUMP => {
                if m1 && !self.gap_jump {
                    self.bowser_walk_speed(st);
                    if breathing {
                        self.vx = 0.0;
                    }
                }
                self.fall(level, BOWSER_GRAVITY);
                if self.wings {
                    self.bowser_set_flag(bwf::FROM_BIG, false);
                    self.bowser_enter(bw::FLY, st);
                } else if self.timer == 0 && self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            bw::BIG_JUMP => {
                self.fall_capped(level, G, CAP);
                match self.bowser_sub() {
                    0 => {
                        self.vx = 0.0;
                        if self.timer == 0 {
                            self.bowser_launch_big_jump(st);
                            if self.wings {
                                self.bowser_set_flag(bwf::FROM_BIG, true);
                                self.bowser_enter(bw::FLY, st);
                            } else {
                                self.bowser_set_sub(if m3 { 2 } else { 1 });
                            }
                        }
                    }
                    1 => {
                        if self.on_ground {
                            self.bowser_enter(bw::LAND_ON, st);
                        }
                    }
                    _ => {
                        if self.on_ground {
                            self.bowser_enter(bw::LAND_ON, st);
                        } else if self.vy <= 0.0 {
                            self.bowser_enter(bw::HIP_DROP, st);
                        }
                    }
                }
            }
            bw::HIP_DROP => {
                if self.timer == 0 && self.bowser_flag(bwf::SUB) {
                    // landed and the 40 frames are over
                    self.fall_capped(level, G, CAP);
                    if !self.bowser_ground_lost(st) {
                        self.bowser_set_sub(0);
                        self.bowser_enter(bw::LOOK_AROUND, st);
                    }
                    return;
                }
                if self.bowser_sub() == 0 {
                    self.fall_capped(level, -0.4, -8.0);
                    if self.on_ground {
                        // quake: players on the ground in view are stunned and the camera
                        // shakes when he is in view [the stun has no hook in the engine yet]
                        self.bowser_set_sub(1);
                        self.timer = 40;
                    }
                } else {
                    self.fall_capped(level, BOWSER_GRAVITY, CAP);
                }
            }
            bw::LOOK_AROUND => {
                self.vx = 0.0;
                self.fall(level, BOWSER_GRAVITY);
                if self.bowser_ground_lost(st) {
                    return;
                }
                if self.timer == 1 {
                    self.dir = if self.target.0 <= self.x { -1 } else { 1 };
                    self.speed = 0.0;
                } else if self.speed >= 0.0 {
                    // the turn animation; 10 frames in he moves on
                    self.speed += 1.0;
                    if self.speed >= 10.0 {
                        if self.bowser_player_ahead_right(st) {
                            self.bowser_chase_dispatch(level, st);
                        } else {
                            self.bowser_enter(bw::LAND_ON, st);
                        }
                    }
                }
            }
            bw::TURN => {
                self.fall(level, BOWSER_GRAVITY);
                self.speed += 1.0;
                if self.speed < self.bowser_turn_frames(st) {
                    return;
                }
                self.dir = -self.dir;
                if self.bowser_ground_lost(st) {
                    return;
                }
                if self.bowser_player_ahead_right(st) {
                    self.bowser_chase_dispatch(level, st);
                } else {
                    self.bowser_decide(level, st);
                }
            }
            bw::FLY => self.bowser_fly(level, st),
            bw::HIT_BLOCK => {
                self.fall_capped(level, G, CAP);
                if self.timer == 0 && self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            _ => {}
        }
    }

    /// Big-jump take-off: vy 5.9 (SMB3 6.2); horizontal ±2.0 along the heading in chase mode,
    /// aimed at the player in SMB3, otherwise random in [−2, 2) (winged ±1.5).
    fn bowser_launch_big_jump(&mut self, st: usize) {
        let base = if self.wings { 1.5 } else { 2.0 };
        self.vy = if st == 1 { 6.2 } else { 5.9 };
        self.on_ground = false;
        self.vx = if self.bowser_mode() == 2 {
            base * self.heading
        } else if st == 1 {
            self.bowser_aim_big_jump(base)
        } else {
            let span = if self.wings { 3.0 } else { 4.0 };
            span * self.rand(1 << 16) as f32 / 65536.0 - base
        };
    }

    /// SMB3 aim (0x71006ac720): within 128 px and with the player not to his right, half the
    /// time a random [−2, 2); otherwise the speed that reaches the player at the apex,
    /// |dx|·0.1875 / 5.9, at most 5 (halved with wings), toward the player.
    fn bowser_aim_big_jump(&mut self, base: f32) -> f32 {
        let dx = self.target.0 - self.x;
        if dx.abs() < 128.0 && dx <= 0.0 && self.rand(2) == 0 {
            return 2.0 * base * self.rand(1 << 16) as f32 / 65536.0 - base;
        }
        let mut v = (dx.abs() * -G / 5.9).min(5.0);
        if self.wings {
            v *= 0.5;
        }
        if dx < 0.0 { -v } else { v }
    }

    /// Winged Bowser's flutter (Fly 0x71006b1430): falls until vy ≤ −1.0, then gravity +0.05
    /// lifts him until vy ≥ 1.5 (or a ceiling), then −0.1875 to the ground. After an SMB3 big
    /// jump the descent ends in a hip drop (at once when he is within 3 px of the player).
    fn bowser_fly(&mut self, level: &Level, st: usize) {
        let from_big = st == 1 && self.bowser_flag(bwf::FROM_BIG);
        if from_big && (self.target.0 - self.x).abs() < 3.0 {
            self.bowser_enter(bw::HIP_DROP, st);
            return;
        }
        let sub = self.bowser_sub();
        let g = match sub {
            1 => 0.05,
            _ => G,
        };
        let before = self.vy;
        self.fall_capped(level, g, CAP);
        let ceiling = before > 0.0 && self.vy == 0.0 && !self.on_ground;
        match sub {
            0 => {
                if self.on_ground || self.vy <= -1.0 {
                    self.bowser_set_sub(1);
                }
            }
            1 => {
                if from_big {
                    let w = self.bowser_bg_half(st);
                    if (self.target.0 - self.x + w).abs() < 48.0 {
                        self.bowser_set_sub(2);
                        return;
                    }
                }
                if ceiling || self.vy >= 1.5 {
                    self.bowser_set_sub(if from_big { 3 } else { 2 });
                }
            }
            2 => {
                if self.on_ground {
                    self.bowser_enter(bw::LAND_ON, st);
                }
            }
            _ => {
                if self.vy <= 0.0 {
                    self.bowser_enter(bw::HIP_DROP, st);
                }
            }
        }
    }

    /// Decision (vtable slot 0x318, 0x71006a9690): patrol bounds, turn to the player, jump
    /// counters. Chase and off-screen modes have their own (below).
    fn bowser_decide(&mut self, level: &Level, st: usize) {
        match self.bowser_mode() {
            1 | 2 => return self.bowser_chase_dispatch(level, st),
            3 => return self.bowser_idle_decide(level, st),
            _ => {}
        }
        if self.x <= self.home_x - BOWSER_PATROL {
            self.heading = 1.0;
        } else if self.x >= self.home_x + BOWSER_PATROL {
            self.heading = -1.0;
        } else if self.wall
            || self.touching_wall(level, self.heading as i32)
            || self.ledge_ahead(level, self.heading)
        {
            self.heading = -self.heading;
        }
        if self.on_ground && self.bowser_player_behind(st) {
            self.bowser_enter(bw::TURN, st);
            return;
        }
        self.bowser_counters(st);
    }

    /// The jump-counter tail of a decision: both counters count down; at 0 the big jump (if
    /// allowed: not SMB1, not while breathing) or the middle jump, otherwise a walk (SMB1) or
    /// little hop. No big jump with a Trampoline over him within its reach
    /// (`bowser_height_search`) [obs v3.0.3 0x7101153750]; in SMB1 / SMB3 no middle jump
    /// either with one within the middle jump's reach (the decision 0x7101152520's slot 0x368,
    /// 0x710115ab40) [obs v3.0.3]. SMW and NSMBU have their own branch slots (SMW 0x7101157dd0
    /// / 0x7101157c70, NSMBU 0x7101152ab0 / 0x7101152970) and states: actors/bosses/styles.rs.
    fn bowser_counters(&mut self, st: usize) {
        let j = self.byte(0).saturating_sub(1);
        let b = self.byte(1).saturating_sub(1);
        self.set_byte(0, j);
        self.set_byte(1, b);
        if st >= 2 {
            // SMW / NSMBU: the big jump is always allowed (slot 0x360 0x7101152aa0); their
            // own branches (actors/bosses/styles.rs)
            if b == 0 {
                self.bowser_reset_big(st);
                self.koopa_big_branch(st);
            } else if j == 0 {
                self.bowser_reset_jump(st);
                self.koopa_middle_branch(st);
            } else {
                self.bowser_walk(st);
            }
            return;
        }
        // (the big jump's allowed test 0x7101153750: no Trampoline over him within its reach)
        let big_ok = st != 0
            && !self.bowser_flag(bwf::BREATHING)
            && !self.spring_over.is_some_and(|h| h <= self.spring_reach_big);
        if big_ok && b == 0 {
            self.bowser_reset_big(st);
            self.bowser_rearm_fire();
            self.bowser_enter(bw::BIG_JUMP, st);
        } else if j == 0 {
            self.bowser_reset_jump(st);
            // SMB1 / SMB3: no middle jump with a Trampoline over him within its reach — he
            // walks (hops) instead (slot 0x368 0x710115ab40 → slot 0x308) [obs v3.0.3]
            if st <= 1 && self.spring_over.is_some_and(|h| h <= self.spring_reach_mid) {
                self.bowser_walk(st);
            } else {
                self.bowser_enter(bw::MIDDLE_JUMP, st);
            }
        } else {
            self.bowser_walk(st);
        }
    }

    /// The decision's walk (slot 0x308): SMB1 Walk, SMB3 LittleJump (0x710115a810), SMW and
    /// NSMBU Walk (0x71011524d0) [obs v3.0.3].
    fn bowser_walk(&mut self, st: usize) {
        if st == 1 {
            self.bowser_enter(bw::LITTLE_JUMP, st);
        } else {
            self.bowser_enter(bw::WALK, st);
        }
    }

    /// Off-screen mode 3 (0x71006a9970): in view → home here, mode 0, decide; otherwise stand
    /// (walk speed 0) and middle-jump when the jump counter runs out.
    fn bowser_idle_decide(&mut self, level: &Level, st: usize) {
        if self.in_view() {
            self.home_x = self.x;
            let b = self.byte(3) & !bwf::MODE;
            self.set_byte(3, b);
            return self.bowser_decide(level, st);
        }
        let j = self.byte(0);
        if j != 0 {
            self.set_byte(0, j - 1);
            if j - 1 != 0 {
                return self.bowser_walk(st);
            }
        }
        self.bowser_reset_jump(st);
        if st >= 2 {
            // SMW / NSMBU: their middle branch (the off-screen decision 0x7101152800 runs
            // slot 0x368)
            return self.koopa_middle_branch(st);
        }
        self.bowser_enter(bw::MIDDLE_JUMP, st);
    }

    /// Which chase mode (0x71006abe10): small Bowser outside SMB3 always chases fast (mode 2).
    /// Big or SMB3: fast within 640 px of the area's right end (not SMB3) or when he has
    /// fallen 32 px behind the view's left edge, otherwise at walking pace (mode 1).
    fn bowser_chase_dispatch(&mut self, level: &Level, st: usize) {
        let m3 = st == 1;
        if !m3 && !self.big {
            return self.bowser_chase(level, 2, st);
        }
        let cam_l = self.camera.0;
        let fast = if !m3 && self.x > level.right - 640.0 {
            true
        } else {
            match self.bowser_mode() {
                1 => self.x < cam_l - 32.0,
                2 => self.x <= cam_l + 32.0,
                _ => false,
            }
        };
        self.bowser_chase(level, if fast { 2 } else { 1 }, st);
    }

    /// Chase decision (0x71006ab370 [obs v1.0.0]; unchanged in v3.0.3 [obs v3.0.3
    /// 0x7101154260]): the first time, switch mode. While the
    /// player stays past his front edge he heads that way; otherwise home moves here and he
    /// goes back to patrolling. On the way, with a wall on his right (BG bit 12) he jumps it —
    /// the middle jump for a low one, the big jump for a high one — when the obstacle scan finds
    /// a way over; at a gap he middle-jumps across at the speed that lands him on its far
    /// ground (also when the scan finds no way over a wall he has no jump), else turns round
    /// (`bowser_chase_jumps`).
    fn bowser_chase(&mut self, level: &Level, mode: u8, st: usize) {
        if self.bowser_mode() != mode {
            return self.bowser_set_mode(mode, st);
        }
        if self.on_ground && self.x + self.bowser_bg_half(st) < self.target.0 {
            self.heading = self.dir as f32;
            if self.bowser_player_behind(st) {
                return self.bowser_enter(bw::TURN, st);
            }
            if self.bowser_chase_jumps(level, st) {
                return;
            }
            return self.bowser_counters(st);
        }
        self.home_x = self.x;
        let b = self.byte(3) & !bwf::MODE;
        self.set_byte(3, b);
        self.bowser_rearm_fire();
        self.bowser_decide(level, st);
    }

    /// The chase's obstacle and gap jumps (0x71006ab370 [obs v1.0.0], the same in v3.0.3
    /// [obs v3.0.3 0x7101154260: its vtable slots 0x18 further on]); true when a jump
    /// started. The obstacle kind (vtable slot 0x310 → 0x71006abfe0; SMB1 never takes the big
    /// jump); whether that jump is allowed (slot 0x428, 0x71006aa9d0 = v3.0.3 0x7101153860:
    /// not for kind 0, nor with solid terrain overhead (`bowser_blocked_overhead`), nor with a
    /// Trampoline over him within the kind's reach (`bowser_height_search`, slot 0x3f8)); a
    /// wall on his right (BG bit 12:
    /// v1.0.0's +0x649 bit 4, v3.0.3's +0x659 bit 5) with a way over → kind 2 the big jump (slot 0x308, state
    /// BigJump), kind 1 the middle jump (slot 0x300); then the gap scan (0x71006ab810): a gap
    /// with the jump allowed and no wall in the way → the middle jump across (slot 0x2f8, the
    /// counters reset by slots 0x358 / 0x360, the gap flag set for MiddleJump's enter
    /// 0x71006b0370), otherwise he turns round.
    fn bowser_chase_jumps(&mut self, level: &Level, st: usize) -> bool {
        let (mut kind, rows) = self.bowser_obstacle_kind(level);
        if st == 0 && kind > 1 {
            kind = 1;
        }
        let can = kind != 0
            && !self.bowser_blocked_overhead(level, rows)
            && !self
                .spring_over
                .is_some_and(|h| h <= self.bowser_reach(level, kind == 2));
        let wall = self.touching_wall(level, 1);
        if wall && can && st >= 2 {
            self.koopa_chase_jump(st, kind == 2, None);
            return true;
        }
        if wall && can {
            self.bowser_enter(
                if kind == 2 {
                    bw::BIG_JUMP
                } else {
                    bw::MIDDLE_JUMP
                },
                st,
            );
            return true;
        }
        let (gap, landing) = self.bowser_gap_ahead(level);
        if gap {
            if can && !wall && st >= 2 {
                // SMW / NSMBU: the big jump at the landing speed (slot 0x310)
                self.bowser_reset_jump(st);
                self.bowser_reset_big(st);
                self.koopa_chase_jump(st, true, Some(landing));
                return true;
            }
            if can && !wall {
                self.bowser_reset_jump(st);
                self.bowser_reset_big(st);
                self.bowser_enter(bw::MIDDLE_JUMP, st);
                self.vx = self.bowser_gap_speed(level, landing);
                self.gap_jump = true;
                return true;
            }
            self.heading = -self.heading;
        }
        false
    }

    /// The obstacle scan (0x71006abfe0 [obs v1.0.0] = [obs v3.0.3 0x7101154ed0], 36 at
    /// 0x71023190a0): at 24 px ahead (big 36) the points 8,
    /// 24, 40, … px over his feet are tested for terrain until a free run as tall as he is (3
    /// tiles, big 5) is found; the run starting 0–2 tiles up is kind 1, 3–5 tiles up kind 2,
    /// higher or none kind 0; with the run's start row over the first.
    fn bowser_obstacle_kind(&self, level: &Level) -> (u8, i32) {
        let px = self.x + if self.big { 36.0 } else { 24.0 };
        let need = if self.big { 5 } else { 3 };
        let solid = |k: i32| level.cell_at(px, self.y + 8.0 + 16.0 * k as f32).is_solid();
        let mut run = 0;
        for k in 0..10 {
            if solid(k) {
                run = 0;
                continue;
            }
            run += 1;
            if run == need {
                let up = k + 1 - need;
                let kind = match up {
                    0..=2 => 1,
                    3..=5 => 2,
                    _ => 0,
                };
                return (kind, up);
            }
        }
        (0, -1)
    }

    /// The jump's overhead check (0x7101153d40, run by the allowed test 0x7101153860 [obs
    /// v3.0.3]): upward lines in 3 columns (big 6) from 16 px left of him (big 40) and 16 px
    /// apart, each first held within his BG half width less 1 px, from 24 px over his feet up
    /// to 16 px a row of the obstacle's free run (`rows`) over his BG sensor's top, find terrain
    /// of a solid class (bits 20–22 of the unit not 0 or 2, the query filter 0x71028e3b88:
    /// semisolid tops don't count) → no jump. His BG sensor is his box id's record in
    /// 0x71023cb8b0 (0x34 + style, set by his build 0x7101147960; 3D World 0x38, 0x710115ebe8):
    /// top 26 / 31.8 / 34 / 36 / 46, half width 13 / 12 / 14 / 15.9 / 30, both ×2 big [obs].
    fn bowser_blocked_overhead(&self, level: &Level, rows: i32) -> bool {
        let (top, half) = if level.style == "3W" {
            (46.0, 30.0)
        } else {
            let st = style_index(level);
            ([26.0, 31.8, 34.0, 36.0][st], BOWSER_BG_HALF[st])
        };
        let k = if self.big { 2.0 } else { 1.0 };
        let (cols, half) = if self.big { (6, half * k) } else { (3, half) };
        let (lo, hi) = (self.x - half + 1.0, self.x + half - 1.0);
        let (y0, y1) = (self.y + 24.0, self.y + 16.0 * rows as f32 + top * k);
        let mut x = self.x - if self.big { 40.0 } else { 16.0 };
        for _ in 0..cols {
            x = x.clamp(lo, hi);
            let mut y = y0;
            loop {
                if level.cell_at(x, y.min(y1)).is_solid() {
                    return true;
                }
                if y >= y1 {
                    break;
                }
                y += TILE;
            }
            x += 16.0;
        }
        false
    }

    /// The jump-height search (0x7101153f00, run by the allowed tests 0x7101153860 /
    /// 0x7101153750, the decision 0x7101152520, LittleJump 0x7101158ff0 and HipDrop
    /// 0x7101159750) [obs v3.0.3]. His columns (3, big 6, from x − 16, big x − 40, 16 apart)
    /// probe upward from 24 px over his feet to the big jump's height (`bowser_reach`): first
    /// for solid terrain (each column held within his BG half less 1 px; counting the columns
    /// hit and keeping the lowest hit), then, unheld, for the BG units of type 0x25 with no
    /// collision class that an actor owns (filter 0x71028e3ba8). Those are a Trampoline's body
    /// (JumpStep's second collider, 0x71013b9260: the block attribute table _INIT_369's
    /// "Jumpdai" entry 0x25; its one-way top is type 0x24). The lowest Trampoline found is the
    /// answer when terrain doesn't cover every column, or when it does but the Trampoline is no
    /// higher than the lowest terrain. Height from his feet. [inf: the body box x ±8, 0–15 up
    /// (descriptor 0x71023cf754); found each frame, where the game caches it in +0xb4]
    fn bowser_height_search(&mut self, level: &Level) -> Option<f32> {
        let st = style_index(level);
        let top = if level.style == "3W" {
            46.0
        } else {
            [26.0, 31.8, 34.0, 36.0][st]
        } * self.scale();
        // the apex heights (0x71011530f0): Σ v while v += g stays > 0, + his height + 16
        let apex = |mut v: f32, g: f32| {
            let mut h = 0.0;
            loop {
                v += g;
                h += v;
                if v <= 0.0 {
                    break;
                }
            }
            h + top + 16.0
        };
        // in water / under the water-speed rule (0x71012a9180, or the liquid contact) the
        // second set (+0x98 / +0xa0 / +0xa8): the gravity scaled by the speed's band, ×0.1
        // under 0.3, ×0.15 under 0.7, ×0.2 under 1.2, else ×0.4, and half of each frame's v
        // (0x710115aed0 / 0x71011530f0)
        let apex_wet = |mut v: f32, g: f32| {
            let mut h = 0.0;
            loop {
                let a = v.abs();
                let k = if a < 0.3 {
                    0.1
                } else if a < 0.7 {
                    0.15
                } else if a < 1.2 {
                    0.2
                } else {
                    0.4
                };
                v += g * k;
                h += v * 0.5;
                if v <= 0.0 {
                    break;
                }
            }
            h + top + 16.0
        };
        let slow = self.slow_rule(level);
        let reach = |v: f32, g: f32| if slow { apex_wet(v, g) } else { apex(v, g) };
        let wings = self.wings;
        self.spring_slow = slow;
        // the little hop (vy 1.1 at −0.07, SMB1 / SMB3 slots 0x3a8 / 0x3c8: +0x94 / +0x98)
        self.spring_reach_little = reach(1.1, -0.07);
        self.spring_reach_mid = reach(if wings { 3.4 } else { 3.0 }, BOWSER_GRAVITY);
        self.spring_reach_big = reach(if wings { 6.4 } else { 5.9 }, -0.1875);
        // the third (+0xac / +0xb0), NSMBU's JumpFire: 3.2 (winged 3.4) at −0.1
        self.spring_reach_third = reach(if wings { 3.4 } else { 3.2 }, BOWSER_GRAVITY);
        self.spring_reach_body = top + 16.0;
        if self.overhead_units.is_empty() {
            return None;
        }
        let half = if level.style == "3W" {
            30.0
        } else {
            BOWSER_BG_HALF[st]
        } * self.scale();
        let cols = if self.big { 6 } else { 3 };
        let x0 = self.x - if self.big { 40.0 } else { 16.0 };
        let (y0, y1) = (self.y + 24.0, self.y + self.spring_reach_big);
        let (lo, hi) = (self.x - half + 1.0, self.x + half - 1.0);
        let (mut hits, mut low_solid) = (0, f32::MAX);
        let mut x = x0;
        for _ in 0..cols {
            x = x.clamp(lo, hi);
            let mut y = y0;
            while y <= y1 {
                if level.cell_at(x, y).is_solid() {
                    let at = (y / TILE).floor() * TILE;
                    hits += 1;
                    low_solid = low_solid.min(at.max(y0) - self.y);
                    break;
                }
                y = ((y / TILE).floor() + 1.0) * TILE;
            }
            x += 16.0;
        }
        let mut low = None::<f32>;
        for c in 0..cols {
            let cx = x0 + 16.0 * c as f32;
            for &(bx0, by0, bx1, by1) in &self.overhead_units {
                if cx >= bx0 && cx <= bx1 && by1 > y0 && by0 <= y1 {
                    let h = by0.max(y0) - self.y;
                    low = Some(low.map_or(h, |l: f32| l.min(h)));
                }
            }
        }
        let low = low?;
        (hits != cols || low <= low_solid).then_some(low)
    }

    /// His jump's reach for the obstacle kind (slot 0x3f8 0x7101153460: kind 2 the big jump's
    /// height +0xa4, else the middle jump's +0x9c; `bowser_height_search` sets them).
    fn bowser_reach(&self, _level: &Level, big: bool) -> f32 {
        if big {
            self.spring_reach_big
        } else {
            self.spring_reach_mid
        }
    }

    /// The gap scan (0x71006ab810 with no third argument [obs v1.0.0] = [obs v3.0.3
    /// 0x7101154700]): ground is looked for
    /// by downward rays at 2 + 24, 16, 8, 0 px ahead (big 2 + 48 … 0), farthest first, each from
    /// 8 px over his feet to 8 px under them, both stretched by its distance; none found, or no
    /// ground just past the one found (1, then 8 (16) and 16 (32) px further), is a gap. Returns
    /// the gap and the ground found (his own position when none), where the landing search
    /// starts.
    fn bowser_gap_ahead(&self, level: &Level) -> (bool, (f32, f32)) {
        let d = self.dir as f32;
        let step = if self.big { 16.0 } else { 8.0 };
        let ray = |x: f32| -> Option<f32> {
            let reach = (x - self.x).abs();
            let (top, bottom) = (self.y + 8.0 + reach, self.y - 8.001 - reach);
            ground_ray(level, x, top, bottom)
        };
        let Some((x, y)) = (0..4).rev().find_map(|k| {
            let x = self.x + 2.0 * d + step * k as f32 * d;
            ray(x).map(|y| (x, y))
        }) else {
            return (true, (self.x, self.y));
        };
        let x1 = x + d;
        let x2 = x1 + d + step * d;
        let x3 = x2 + d + 2.0 * step * d;
        let gap = ray(x1).is_none() || ray(x2).is_none() || ray(x3).is_none();
        (gap, (x, y))
    }

    /// The gap jump's speed (0x71006ac370 [obs v1.0.0] = [obs v3.0.3 0x7101155260, run by
    /// MiddleJump's enter 0x7101159260; 1.8 at 0x71023188b4]): the landing is looked for from the
    /// tile 40 px past the ground found (rounded to tiles), every 32 px up to 6 times, by a
    /// 4-px ray down from 2 px over that tile row; the speed covers the distance in the
    /// jump's time, |d| / 2 · |g| / vy, at most 1.8 (in water × 0.6); with no landing found
    /// 1.8.
    fn bowser_gap_speed(&self, level: &Level, from: (f32, f32)) -> f32 {
        self.bowser_gap_speed_with(level, from, 1.8, BOWSER_GRAVITY)
    }

    /// The landing speed for a jump of gravity `g` (vy already set), at most `max` (the SMW /
    /// NSMBU big jump's 2.6, 0x7102318bd4).
    fn bowser_gap_speed_with(&self, level: &Level, from: (f32, f32), max: f32, g: f32) -> f32 {
        let mut x = ((from.0 - 4.0) * (1.0 / 16.0)).floor() * 16.0 + 40.0;
        let top = ((from.1 + 4.0) * (1.0 / 16.0)).floor() * 16.0 + 2.0;
        let mut land = None;
        for _ in 0..6 {
            if ground_ray(level, x, top, top - 4.0).is_some() {
                land = Some(x);
                break;
            }
            x += 32.0;
        }
        let Some(lx) = land.filter(|&lx| lx.abs() > f32::EPSILON) else {
            return max;
        };
        let d = lx - self.x;
        let half = (d * 0.5).abs();
        let mut v = (half * g / self.vy).abs().min(max);
        if self.wet() {
            v *= 0.6;
        }
        if d * 0.5 >= 0.0 { v } else { -v }
    }

    /// The fire helper (0x71006a2710): counts down; at 1 the breath animation starts; the
    /// flame leaves at its fire frame (SMB1: when it ends), then the timer is re-armed.
    fn bowser_fire(&mut self, st: usize) -> Option<Actor> {
        if self.pause > 0 {
            self.pause -= 1;
            if self.pause == 1 {
                self.set_byte(2, 1);
            }
        }
        let frame = self.byte(2);
        let mut out = None;
        if frame > 0 {
            let (len, at) = BOWSER_BREATH[st];
            let f = frame - 1;
            if st != 0 && !self.bowser_flag(bwf::FIRED) && f == at {
                self.bowser_set_flag(bwf::FIRED, true);
                out = Some(self.bowser_flame(st));
            }
            if f >= len {
                if st == 0 {
                    out = Some(self.bowser_flame(st));
                }
                self.bowser_set_flag(bwf::FIRED, false);
                self.pause = self.bowser_fire_interval();
                self.set_byte(2, 0);
            }
        }
        self.bowser_set_flag(bwf::BREATHING, self.byte(2) > 0);
        out
    }

    /// A BulletKoopaFire (0x71006a2d40): at the mouth, 1.5 px/f plus Bowser's carried speed —
    /// the mean of his x steps over the last 16 frames (ring +0x560), 0 unless it points the
    /// way he faces — one of three lanes at random. (The game's ring leaves out the motion of a
    /// platform carrying him; the engine's position history includes it.)
    fn bowser_flame(&mut self, st: usize) -> Actor {
        let s = self.scale();
        let d = self.dir as f32;
        let (mx, my) = BOWSER_MOUTH[st];
        let mut f = Actor::new(Kind::BowserFire, self.x + d * mx * s, self.y + my * s);
        f.dir = self.dir;
        let mean = self.bowser_mean_speed();
        let carry = if mean * d >= 0.0 { mean } else { 0.0 };
        f.vx = BOWSER_FIRE_SPEED * d + carry;
        let lane = FIRE_LANE * s;
        let (ty, vy) = match self.rand(3) {
            0 => (f.y, 0.0),
            1 => (f.y - lane, -f.vx.abs() * 0.5),
            _ => (f.y + lane, f.vx.abs() * 0.5),
        };
        f.home_y = ty;
        f.vy = vy;
        f.timer = 600;
        f
    }

    // ------------------------------------------------------------ Boom Boom

    fn bb_sub(&self) -> u8 {
        self.byte(3) & 0x03
    }

    fn bb_frame(&self) -> u8 {
        self.byte(3) >> 2
    }

    fn bb_set_sub(&mut self, s: u8) {
        self.set_byte(3, s & 0x03);
    }

    fn bb_tick_frame(&mut self) {
        let f = (self.bb_frame() + 1).min(63);
        self.set_byte(3, (self.byte(3) & 0x03) | (f << 2));
    }

    /// Boom Boom's flags (byte 2): guard, carried-over damage immunity, repeat swoop.
    fn bb_flag(&self, f: u8) -> bool {
        self.byte(2) & f != 0
    }

    fn bb_set_flag(&mut self, f: u8, on: bool) {
        let b = self.byte(2);
        self.set_byte(2, if on { b | f } else { b & !f });
    }

    fn bb_guard(&self) -> bool {
        self.bb_flag(BBF_GUARD) || self.bb_flag(BBF_IMMUNE)
    }

    /// Wall bumps on the right (dir > 0) / left side (byte 1, a nibble each).
    fn bb_bumps(&self, dir: i32) -> u8 {
        let b = self.byte(1);
        if dir > 0 { b & 0x0f } else { b >> 4 }
    }

    fn bb_set_bumps(&mut self, dir: i32, n: u8) {
        let b = self.byte(1);
        let n = n.min(15);
        self.set_byte(
            1,
            if dir > 0 {
                (b & 0xf0) | n
            } else {
                (b & 0x0f) | (n << 4)
            },
        );
    }

    /// The action timer after a jump or Defend: 240 + random(0…59) (30 + random at 2 HP).
    fn bb_rearm(&mut self) {
        let base = if self.hops == 2 { 30 } else { 240 };
        self.pause = base + self.rand(60) as i32;
    }

    fn bb_slow(&self) -> i32 {
        self.speed as i32
    }

    fn bb_target_in_range(&self) -> bool {
        let (px, py) = self.target;
        ((px - self.x).powi(2) + (py - self.y).powi(2)).sqrt() <= BB_RANGE
    }

    /// Boom Boom (BunbunM3, also SMB1/SMW). Starts curled up (Defend), then walks at 0.75 /
    /// 0.75 / 1.5 px/f by hits taken, re-aiming every 40 frames; a wall costs a WallJump (the
    /// 3rd in a row high), the 4th bump turns it. When the action timer (240–299) runs out:
    /// 3 HP → a 2.0 dash that brakes into Defend; 2 HP → Jump (5.5); 1 HP → keep running. A
    /// hit → DamageDown (70 frames, floating) → Defend (brake, 30 frames, a 2.75 hop at the
    /// player) → 60 frames slow → Walk. Winged: hover near the player, swoop at it.
    pub(super) fn step_boom_boom(&mut self, level: &Level) -> Option<Actor> {
        if self.phase == 0 {
            self.boss_setup(level);
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if let Kind::BoomBoom { pom: true, .. } = self.kind {
            return self.step_pom_pom(level);
        }
        match self.phase {
            bb::WALK => self.bb_walk(level),
            bb::JUMP | bb::WALL_JUMP => self.bb_jump(level),
            bb::DAMAGE_DOWN | bb::W_DAMAGE => {
                // floats: no gravity, no motion
                self.vx = 0.0;
                self.vy = 0.0;
                self.collide(level);
                if self.timer == 0 {
                    let wings = self.phase == bb::W_DAMAGE;
                    self.bb_enter_defend(true);
                    if wings {
                        self.phase = bb::W_DEFEND;
                    }
                }
            }
            bb::DEFEND | bb::W_DEFEND => self.bb_defend(level),
            bb::W_WAIT => {
                self.vx = Self::approach(self.vx, 0.0, 0.5);
                self.vy = Self::approach(self.vy, 0.0, 0.5);
                self.collide(level);
                // a player is always targeted here
                self.bb_enter_fly();
            }
            bb::W_FLY => self.bb_fly(level),
            bb::W_ATTACK => self.bb_attack(level),
            _ => {
                // Clown Car: the Clown context only rides (no attack)
                self.step_clown(level);
            }
        }
        None
    }

    fn bb_enter_walk(&mut self, from_defend: bool) {
        self.phase = bb::WALK;
        if from_defend {
            self.speed = BB_SLOW_FRAMES as f32;
        } else {
            self.dir = self.toward_target();
        }
    }

    fn bb_walk(&mut self, level: &Level) {
        let toward = self.toward_target();
        if self.pause < 1 {
            match self.hops {
                1 => self.pause = 240 + self.rand(60) as i32,
                2 => return self.bb_enter_jump(),
                _ => {
                    self.vx = BB_CHARGE * self.dir as f32;
                    return self.bb_enter_defend(false);
                }
            }
        }
        let lost = 3u8.saturating_sub(self.hops).min(2) as usize;
        let mut slow = self.bb_slow();
        if slow != 0 {
            slow -= 1;
            self.speed = slow as f32;
        }
        if self.pause != 0 {
            self.pause -= 1;
        }
        if !self.bb_target_in_range() {
            return self.bb_enter_defend(false);
        }
        if slow < 1 {
            if self.pause % 40 == 0 {
                self.dir = toward;
            }
            self.vx = BB_SPEED[lost] * self.dir as f32;
        } else {
            self.bb_rearm();
            self.vx = BB_SLOW * self.dir as f32;
        }
        self.fall(level, G);
        if self.wall {
            let side = self.dir;
            let n = self.bb_bumps(side);
            self.bb_set_bumps(side, n + 1);
            if n < BB_WALL_BUMPS {
                return self.bb_enter_wall_jump();
            }
            self.bb_set_bumps(side, 0);
            self.dir = -self.dir;
            self.vx = -self.vx;
        }
    }

    fn bb_enter_jump(&mut self) {
        self.phase = bb::JUMP;
        self.timer = BB_WINDUP;
        self.vx = 0.0;
        self.bb_set_sub(0);
    }

    fn bb_enter_wall_jump(&mut self) {
        self.phase = bb::WALL_JUMP;
        self.bb_set_sub(1);
        self.vy = if self.bb_bumps(self.dir) == 3 {
            BB_WALL_JUMP_HIGH
        } else {
            BB_WALL_JUMP
        };
        self.vx = BB_JUMP_VX * self.dir as f32;
        self.on_ground = false;
    }

    /// Jump / WallJump (0x71006361a0): a wall met in the air turns it while that side has no
    /// bumps; a landing pauses 30 frames after a Jump, 1 after a WallJump.
    fn bb_jump(&mut self, level: &Level) {
        if self.phase == bb::WALL_JUMP && self.pause != 0 {
            self.pause -= 1;
        }
        let vx = self.vx;
        self.fall(level, G);
        if self.wall && self.bb_bumps(self.dir) == 0 {
            self.dir = -self.dir;
            self.vx = vx.abs() * self.dir as f32;
        }
        match self.bb_sub() {
            0 => {
                self.bb_rearm();
                self.set_byte(1, 0);
                self.vx = 0.0;
                if self.timer == 0 {
                    self.dir = self.toward_target();
                    self.vx = BB_JUMP_VX * self.dir as f32;
                    self.vy = BB_JUMP;
                    self.on_ground = false;
                    self.bb_set_sub(1);
                }
            }
            1 => {
                if self.vy <= 0.0 && self.on_ground {
                    self.timer = if self.phase == bb::JUMP { 30 } else { 1 };
                    self.vx = 0.0;
                    self.vy = 0.0;
                    self.bb_set_sub(2);
                }
            }
            _ => {
                self.vx = 0.0;
                if self.timer == 0 {
                    if self.phase == bb::WALL_JUMP {
                        // WallJump exit: a cleared wall takes a bump back
                        let side = self.dir;
                        let n = self.bb_bumps(side);
                        if n > BB_WALL_BUMPS {
                            self.set_byte(1, 0);
                        } else if !self.touching_wall(level, side) {
                            self.bb_set_bumps(side, n.saturating_sub(1));
                        }
                    }
                    self.bb_enter_walk(false);
                }
            }
        }
    }

    fn touching_wall(&self, level: &Level, dir: i32) -> bool {
        let wx = self.x + dir as f32 * (self.half_w() + 0.5);
        [self.y + 2.0, self.y + self.height() - 2.0]
            .iter()
            .any(|&wy| level.cell_at(wx, wy).is_solid())
    }

    fn bb_enter_defend(&mut self, from_damage: bool) {
        self.phase = bb::DEFEND;
        self.bb_set_sub(0);
        self.bb_set_flag(BBF_GUARD, false);
        self.bb_set_flag(BBF_IMMUNE, from_damage);
        self.set_byte(1, 0);
    }

    /// Defend (0x7100635b20): squat until on the ground, then 30 frames while braking by 0.05;
    /// a hop at the player (vy 2.75, 1.0 px/f) with the stand-up animation; on landing the
    /// action timer is re-armed → Walk (slow). Winged (0x7100632cc0): the hop hovers at its
    /// apex and 12 frames after the animation it flies again.
    fn bb_defend(&mut self, level: &Level) {
        let winged = self.phase == bb::W_DEFEND;
        self.bb_tick_frame();
        let frame = self.bb_frame();
        // the squat: spiky (guarded) from its 7th frame, which also ends the damage immunity
        if self.bb_sub() < 2 && frame > 6 {
            self.bb_set_flag(BBF_IMMUNE, false);
            self.bb_set_flag(BBF_GUARD, true);
        }
        match self.bb_sub() {
            0 => {
                self.fall(level, G);
                if self.on_ground {
                    self.vy = 0.0;
                    self.timer = BB_DEFEND_WAIT;
                    self.set_byte(3, (self.byte(3) & 0xfc) | 1);
                }
            }
            1 => {
                if self.vx != 0.0 {
                    let v = self.vx - BB_DEFEND_BRAKE * self.vx.signum();
                    if v * self.vx > 0.0 {
                        self.vx = v;
                        self.fall(level, G);
                        return;
                    }
                    self.vx = 0.0;
                }
                self.fall(level, G);
                if self.timer != 0 {
                    return;
                }
                self.dir = self.toward_target();
                self.vx = self.dir as f32;
                self.vy = BB_DEFEND_HOP;
                self.on_ground = false;
                self.set_byte(3, 2);
            }
            2 => {
                if frame > 9 {
                    self.bb_set_flag(BBF_GUARD, false);
                }
                if winged && self.vy <= 0.0 {
                    self.vy = 0.0;
                    self.collide(level);
                } else {
                    self.fall(level, G);
                }
                if frame >= BB_ANIM {
                    if winged {
                        self.timer = 12;
                    }
                    self.bb_set_sub(3);
                }
            }
            _ => {
                if winged {
                    if self.vy <= 0.0 {
                        self.vy = 0.0;
                    }
                    self.collide(level);
                    if self.timer == 0 {
                        self.phase = bb::W_FLY;
                        self.bb_enter_fly();
                    }
                } else {
                    self.fall(level, G);
                    if self.on_ground {
                        self.bb_rearm();
                        self.bb_enter_walk(true);
                    }
                }
            }
        }
    }

    fn bb_enter_fly(&mut self) {
        self.phase = bb::W_FLY;
        self.timer = 240 + self.rand(60) as i32;
        self.bb_set_flag(BBF_REPEAT, self.hops == 1);
    }

    /// The point a winged Boom Boom hovers around (Wing slot 6, 0x7100631a80).
    fn bb_fly_home(&self) -> (f32, f32) {
        let (cx, cy) = self.view_centre();
        let hx = (self.target.0 + 1.2 * TILE * self.dir as f32).clamp(cx - 128.0, cx + 128.0);
        (hx, cy + 3.5 * TILE)
    }

    /// Fly (0x71006321e0): ±2.0 px/f toward the hover point (0.05 px/f²), ±1.0 vertically in
    /// 0.075 steps; after 240–299 frames, with the player in view, Attack.
    fn bb_fly(&mut self, level: &Level) {
        let (hx, hy) = self.bb_fly_home();
        self.dir = if hx - self.x <= 0.0 { -1 } else { 1 };
        self.vx = Self::approach(self.vx, BB_FLY_SPEED * self.dir as f32, 0.05);
        let ty = if hy - self.y > 0.0 { 1.0 } else { -1.0 };
        self.vy = Self::approach(self.vy, ty, 0.075);
        self.collide(level);
        // the swoop waits until his box is in the player's view (0x710035f7f0)
        if self.timer == 0 && self.box_in_view() {
            self.bb_enter_attack();
        }
    }

    fn bb_enter_attack(&mut self) {
        self.phase = bb::W_ATTACK;
        self.vx = 0.0;
        self.vy = 0.0;
        self.timer = 60;
        self.bb_set_sub(0);
    }

    /// Attack (0x71006324a0): 60 frames still, then a straight dive at where the player was
    /// (over 60 frames at 3 HP, else 40) while the horizontal speed builds to 2.0 (0.025 px/f²);
    /// below the target it pulls up at the dive's speed (steps 0.5, 0.15 at 3 HP, 0.04 once
    /// rising) and climbs back to its start height, then flies (at 1 HP it swoops twice).
    fn bb_attack(&mut self, level: &Level) {
        match self.bb_sub() {
            0 => {
                self.vx = 0.0;
                self.vy = 0.0;
                if self.timer == 0 {
                    let (px, py) = self.target;
                    let n = if self.hops == 3 { 60.0 } else { 40.0 };
                    self.trail = vec![(px, py), (self.x, self.y)];
                    self.dir = if px - self.x < 0.0 { -1 } else { 1 };
                    self.vx = (px - self.x) / n;
                    self.vy = (py - self.y) / n;
                    self.bb_set_sub(1);
                }
            }
            1 => {
                let (tx, ty) = self.trail.first().copied().unwrap_or(self.target);
                self.dir = if tx - self.x <= 0.0 { -1 } else { 1 };
                self.vx = Self::approach(self.vx, BB_FLY_SPEED * self.dir as f32, BB_SWOOP_ACCEL);
                if self.y < ty + 4.0 {
                    self.speed = -self.vy;
                    self.bb_set_sub(2);
                }
            }
            _ => {
                let step = if self.vy >= 0.0 {
                    0.04
                } else if self.hops == 3 {
                    0.15
                } else {
                    0.5
                };
                let target = self.speed.max(-1.2);
                self.vy = Self::approach(self.vy, target, step);
                let (hx, _) = self.bb_fly_home();
                self.dir = if hx - self.x <= 0.0 { -1 } else { 1 };
                self.vx = Self::approach(self.vx, BB_FLY_SPEED * self.dir as f32, 0.05);
                let start_y = self.trail.get(1).map_or(self.y, |p| p.1);
                if self.y >= start_y {
                    self.collide(level);
                    if self.bb_flag(BBF_REPEAT) {
                        self.bb_set_flag(BBF_REPEAT, false);
                        self.bb_enter_attack();
                    } else {
                        self.phase = bb::W_FLY;
                        self.timer = 240 + self.rand(60) as i32;
                    }
                    return;
                }
            }
        }
        self.collide(level);
    }

    /// Pom Pom (Punpun, 3D World; Basic states 0x7102181380). Wait 60 frames facing the
    /// player; on foot she then throws, winged she first jumps (vy 6.0 for 15 frames, then
    /// hovers bobbing 0.4·sin at 8°/frame) and throws from the air. A throw: ThrowSign (on foot
    /// `ThrowSignLeft/Right` 42 frames, winged `JumpThrowSign` 18; the player's position + 11.87
    /// px is taken as the target, 0x7100757c50), Throw (14 frames; the shuriken leaves at its
    /// start, 0x7100756400), ThrowEnd (on foot `ThrowEnd` 37, winged `JumpThrowEnd` 19, then 60
    /// frames, 0x7100757c40), Hide (`Hide` 56: a 2.5 hop at frame 10, level again at 23), then
    /// Lapse: gone for at least 120 frames while a new spot is searched for (0x71007587d0, one
    /// try per frame), LapseMove 60 frames to it, Appear (`Appear` 58) → Wait. A hit
    /// (DamageDown 0x7100757780): a stomp plays `PressDown` (59), anything else hops her at 2.5
    /// through `FireBallDown` (40) and, landed, `FireBallDownEnd` (76) → Lapse. Animation
    /// lengths are the 3W model's at play rate 1.0. Not modelled: the PunpunFake decoys she
    /// leaves at further spots after a hit.
    fn step_pom_pom(&mut self, level: &Level) -> Option<Actor> {
        let toward = self.toward_target();
        let mut shot = None;
        match self.phase {
            bb::POM_WAIT => {
                self.vx = 0.0;
                self.dir = toward;
                self.fall(level, G);
                if self.timer == 0 {
                    if self.wings {
                        self.phase = bb::POM_JUMP;
                        self.vy = 6.0;
                        self.on_ground = false;
                        self.timer = 15;
                        self.heading = 0.0;
                    } else {
                        self.pom_enter_throw_sign();
                    }
                }
            }
            bb::POM_JUMP => {
                if self.timer > 0 {
                    let before = self.vy;
                    self.collide(level);
                    if before > 0.0 && self.vy == 0.0 {
                        self.timer = 0;
                    }
                } else {
                    self.heading += 8.0;
                    self.vy = 0.4 * self.heading.to_radians().dsin();
                    self.collide(level);
                    if self.heading >= 45.0 * 8.0 {
                        self.pom_enter_throw_sign();
                    }
                }
            }
            bb::POM_THROW_SIGN | bb::POM_THROW | bb::POM_THROW_END => {
                self.vx = 0.0;
                if self.wings && self.phase == bb::POM_THROW_SIGN {
                    self.heading += 8.0;
                    self.vy = 0.4 * self.heading.to_radians().dsin();
                    self.collide(level);
                } else if self.wings && self.phase == bb::POM_THROW {
                    self.vy = 0.0;
                    self.collide(level);
                } else {
                    // ThrowEnd and Throw's exit restore the gravity (0x7100756630 / 6640)
                    self.fall(level, G);
                }
                if self.timer == 0 {
                    match self.phase {
                        bb::POM_THROW_SIGN => {
                            // Throw: the shuriken leaves for the target taken at ThrowSign
                            shot = Some(self.pom_shuriken());
                            self.phase = bb::POM_THROW;
                            self.timer = POM_THROW;
                        }
                        bb::POM_THROW => {
                            self.phase = bb::POM_THROW_END;
                            self.timer = POM_THROW_END[self.wings as usize] + POM_AFTER_THROW;
                        }
                        _ => {
                            self.phase = bb::POM_HIDE;
                            self.timer = POM_HIDE;
                        }
                    }
                }
            }
            bb::POM_HIDE => {
                self.vx = 0.0;
                let frame = POM_HIDE - self.timer;
                if frame == 10 {
                    self.vy = 2.5;
                    self.on_ground = false;
                } else if frame == 23 {
                    self.vy = 0.0;
                }
                self.fall(level, G);
                if self.timer == 0 {
                    self.pom_enter_lapse();
                }
            }
            bb::POM_DAMAGE => {
                self.vx = 0.0;
                self.fall(level, G);
                if self.timer == 0 {
                    // FireBallDown, landed: FireBallDownEnd (the hop's chain byte 1 = 1)
                    if self.byte(1) == 1 && self.on_ground {
                        self.set_byte(1, 0);
                        self.timer = POM_DAMAGE_END;
                    } else {
                        self.pom_enter_lapse();
                    }
                }
            }
            bb::POM_LAPSE => {
                self.vx = 0.0;
                self.vy = 0.0;
                self.pause += 1;
                if !self.pom_spot_found() {
                    self.pom_try_spot(level);
                }
                if self.timer == 0 && self.pom_spot_found() {
                    self.phase = bb::POM_LAPSE_MOVE;
                    self.timer = POM_LAPSE_MOVE;
                }
            }
            bb::POM_LAPSE_MOVE => {
                if self.timer == 0 {
                    let (x, y) = self.trail[0];
                    self.x = x;
                    self.y = y;
                    self.on_ground = true;
                    self.phase = bb::POM_APPEAR;
                    self.timer = POM_APPEAR;
                }
            }
            bb::POM_APPEAR => {
                self.vx = 0.0;
                self.dir = toward;
                self.fall(level, G);
                if self.timer == 0 {
                    self.phase = bb::POM_WAIT;
                    self.timer = 60;
                }
            }
            _ => {
                self.phase = bb::POM_WAIT;
                self.timer = 60;
            }
        }
        shot
    }

    fn pom_enter_throw_sign(&mut self) {
        self.phase = bb::POM_THROW_SIGN;
        self.timer = POM_THROW_SIGN[self.wings as usize];
        // the throw target: the player's position, 11.87 px up (0x7100757c50)
        self.home_x = self.target.0;
        self.home_y = self.target.1 + POM_AIM_UP;
    }

    /// The shuriken (BulletPunpunShuriken, 0x71003b8db0) leaves her hand flying level the way
    /// she faces; it homes on the target (see `step_boss_shot`).
    fn pom_shuriken(&self) -> Actor {
        let mut s = Actor::new(Kind::Shuriken, self.x, self.y + 12.0 * self.scale());
        s.dir = self.dir;
        s.big = self.big;
        s.speed = if self.big { 1.5 } else { 1.0 };
        s.heading = 0.0;
        s.home_x = self.home_x;
        s.home_y = self.home_y;
        s.vx = s.speed * self.dir as f32;
        s.timer = 600;
        s
    }

    /// Lapse (0x71007569f0): gone for at least 120 frames while a new spot is searched for.
    fn pom_enter_lapse(&mut self) {
        self.phase = bb::POM_LAPSE;
        self.timer = POM_LAPSE;
        self.pause = 0;
        self.vx = 0.0;
        self.vy = 0.0;
        self.trail.clear();
    }

    fn pom_spot_found(&self) -> bool {
        !self.trail.is_empty()
    }

    /// One try at a new spot (0x71007587d0): a random point in a 384 × 300 box from 192 px left
    /// of and below the view centre; looking down from it (to 108 px below the centre) for
    /// ground with room for her above; at least 28 px (big 56) from the player, her box inside
    /// the view at that spot, and for the first 90 frames 12 px (big 24) from where she was.
    fn pom_try_spot(&mut self, level: &Level) {
        let (cx, cy) = self.view_centre();
        let x = cx - 192.0 + 384.0 * self.rand(1 << 16) as f32 / 65536.0;
        let top = cy - 192.0 + 300.0 * self.rand(1 << 16) as f32 / 65536.0;
        let s = self.scale();
        let mut y = top;
        let ground = loop {
            if y < cy - 108.0 {
                return;
            }
            if level.cell_at(x, y).is_solid() {
                break ((y / TILE).floor() + 1.0) * TILE;
            }
            y -= 1.0;
        };
        if top < ground {
            return;
        }
        let h = self.height();
        if (1..=h as i32).any(|k| level.cell_at(x, ground + k as f32).is_solid()) {
            return;
        }
        let (px, py) = self.target;
        if (px - x).dhypot(py - ground) < 28.0 * s {
            return;
        }
        let mut probe = self.clone();
        probe.x = x;
        probe.y = ground;
        if !probe.box_in_view() {
            return;
        }
        if self.pause <= 90 && (self.x - x).dhypot(self.y - ground) < 12.0 * s {
            return;
        }
        self.trail = vec![(x, ground)];
    }

    // ------------------------------------------------------------ Bowser Jr.

    fn jr_rearm_cooldown(&mut self) {
        let c = 90 + self.rand(45) as u8;
        self.set_byte(2, c);
    }

    fn jr_enter_wait_or_fall(&mut self) {
        if self.on_ground {
            self.jr_enter_wait();
        } else {
            self.phase = jr::FALL;
        }
    }

    fn jr_enter_wait(&mut self) {
        self.phase = jr::WAIT;
        self.timer = JR_WAIT;
    }

    fn jr_sub(&self) -> u8 {
        self.byte(3)
    }

    /// The Hop / ShiftJump wind-up has just finished (the stored jump starts now).
    fn jr_wound_up(&mut self) -> bool {
        if self.jr_sub() == JR_WINDING && self.timer == 0 {
            self.jr_set_sub(0);
            return true;
        }
        false
    }

    fn jr_set_sub(&mut self, s: u8) {
        self.set_byte(3, s);
    }

    /// Bowser Jr. (KoopaJr Basic action). Wait 20 frames facing the player, then: after a
    /// 90–134-frame cooldown (counted in Wait) a shift jump; otherwise (unless he has just
    /// shot) 70 % fire — winged 75 % hammers — when the player is in view; else a computed hop
    /// to the ground 30 px ahead, or a walk (1.25 px/f, winged 0.8) that stops 24 px from home.
    /// Shift jump: straight up 5.5 → at vy ≤ 0.4 PrevHipDrop (winged: Hover 50–74 frames
    /// first) 20 frames → HipDrop (g −0.75, cap −8) → 50 frames → Wait. A hit: DamageDown
    /// (bounce 3.0, 15 frames) → the shell → Wait with home where he stands.
    pub(super) fn step_bowser_jr(&mut self, level: &Level) -> Option<Actor> {
        if self.phase == 0 {
            self.boss_setup(level);
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        let st = style_index(level);
        let mut out = None;
        match self.phase {
            jr::WAIT => self.jr_wait(level),
            jr::FALL => {
                self.fall_capped(level, G, CAP);
                if self.on_ground {
                    self.jr_enter_wait();
                }
            }
            jr::HOP => {
                if self.jr_wound_up() {
                    self.vx = self.speed;
                    self.vy = self.heading;
                    self.on_ground = false;
                }
                if self.jr_sub() == JR_WINDING {
                    self.vx = 0.0;
                    self.fall_capped(level, G, CAP);
                } else {
                    self.fall_capped(level, G, CAP);
                    if self.on_ground && self.vy <= 0.0 {
                        self.vx = 0.0;
                        self.jr_enter_wait();
                    }
                }
            }
            jr::WALK => self.jr_walk(level),
            jr::SHOT => {
                if self.on_ground {
                    self.vx = Self::approach(self.vx, 0.0, 0.5);
                }
                self.fall_capped(level, G, CAP);
                self.pause += 1;
                out = if self.wings {
                    self.jr_hammers()
                } else {
                    self.jr_fire_cycle()
                };
            }
            jr::SHIFT_JUMP => {
                if self.jr_wound_up() {
                    self.vy = self.heading;
                    self.on_ground = false;
                }
                if self.jr_sub() == JR_WINDING {
                    // vx = vy = 0 and no gravity while the wind-up plays
                    self.vx = 0.0;
                    self.vy = 0.0;
                    return None;
                }
                self.fall_capped(level, G, CAP);
                if self.vy <= JR_HOVER_AT {
                    self.jr_rearm_cooldown();
                    if self.wings {
                        self.phase = jr::HOVER;
                        self.timer = 50 + self.rand(25) as i32;
                        self.heading = 0.0;
                    } else {
                        self.jr_enter_prev_hip_drop(st);
                    }
                }
            }
            jr::HOVER => {
                self.heading += 8.0;
                self.vy = 0.4 * self.heading.to_radians().dsin();
                self.vx = 0.0;
                self.collide(level);
                self.dir = self.toward_target();
                if self.timer == 0 {
                    self.jr_enter_prev_hip_drop(st);
                }
            }
            jr::PREV_HIP_DROP => {
                self.vx = 0.0;
                self.vy = 0.0;
                if self.timer == 0 {
                    self.phase = jr::HIP_DROP;
                }
            }
            jr::HIP_DROP => {
                self.vx = 0.0;
                self.fall_capped(level, JR_HIP_GRAVITY, JR_HIP_CAP);
                if self.on_ground {
                    // quake: players on the ground in view are stunned [no engine hook yet]
                    self.phase = jr::HIP_LAND;
                    self.timer = JR_HIP_LAND;
                    self.vy = self.vy.max(-1.5);
                }
            }
            jr::HIP_LAND => {
                self.fall_capped(level, G, CAP);
                if self.timer == 0 {
                    self.jr_rearm_cooldown();
                    self.jr_enter_wait();
                }
            }
            jr::DAMAGE_DOWN => {
                self.vx = 0.0;
                self.fall_capped(level, G, CAP);
                if self.timer == 0 {
                    // ShellIn: outside NSMBU the shell context starts at once
                    self.phase = jr::SHELL_WAIT;
                    self.timer = JR_SHELL_WAIT;
                }
            }
            jr::SHELL_WAIT => {
                self.vx = 0.0;
                self.fall_capped(level, G, CAP);
                if self.timer == 0 {
                    self.phase = jr::SHELL;
                    self.pause = JR_SHELL_FRAMES;
                    self.speed = 0.0;
                    self.heading = 0.0;
                    self.set_byte(1, 0);
                    self.jr_set_sub(SL_AIM);
                }
            }
            jr::SHELL => self.jr_shell_slide(level),
            jr::SHELL_OUT => self.jr_shell_wakeup(level),
            _ => out = self.jr_clown(level),
        }
        out
    }

    /// Within 16 px of the area edge he is sliding toward (his background half-width 6, big 12).
    fn jr_at_area_edge(&self, level: &Level) -> bool {
        let w = 6.0 * self.scale();
        (self.vx < 0.0 && self.x - w <= level.left + 16.0)
            || (self.vx > 0.0 && level.right - 16.0 <= self.x + w)
    }

    /// ShellSlide (0x7100614b10; v3.0.3 exe 0x710108aa50). `pause` is the slide counter,
    /// `speed` the slide speed, `heading` its acceleration, chain byte 1 the turn cooldown,
    /// `jr_sub` the sub-phase, `home_y` the x a hop left from. On the floor he looks under his
    /// front edge each frame (`jr_shell_ledge_wakes`); a wall met on the floor is analysed the
    /// next frame (`jr_shell_obstacle`): he hops it or comes out of the shell. In the air a
    /// wall bounces him back at ×−0.2 (0x71023191a0). [?: with the wall-contact bit 16 set the
    /// game only reverses the speed; what sets it is not known]
    fn jr_shell_slide(&mut self, level: &Level) {
        if self.pause > 0 {
            self.pause -= 1;
        }
        let toward = self.toward_target();
        match self.jr_sub() {
            SL_AIM => {
                self.fall_capped(level, G, CAP);
                if self.on_ground {
                    self.dir = toward;
                    self.speed = 0.0;
                    self.heading = 0.0;
                    self.jr_set_sub(SL_SLIDE);
                }
                return;
            }
            SL_HOP => {
                // sub 3: lands, then slides on at 1.5; a hop that did not move him ends the
                // slide counter and the turn cooldown. The speed (+0x274) outlasts a wall
                // touch: the BG only stops the move.
                self.vx = self.speed * self.dir as f32;
                self.fall_capped(level, G, CAP);
                if self.on_ground {
                    if (self.x - self.home_y).abs() <= 1e-5 {
                        self.pause = 0;
                        self.set_byte(1, 0);
                    }
                    self.jr_set_sub(SL_SLIDE);
                }
                return;
            }
            SL_BRAKE => {
                let step = if self.vx.abs() > JR_SHELL_TOP {
                    0.3
                } else {
                    0.1
                };
                self.vx = Self::approach(self.vx, 0.0, step);
                self.fall_capped(level, G, CAP);
                if self.vx == 0.0 || self.jr_at_area_edge(level) {
                    self.vx = 0.0;
                    self.speed = 0.0;
                    self.heading = 0.0;
                    self.dir = -self.dir;
                    self.set_byte(1, JR_SHELL_TURN);
                    self.jr_set_sub(SL_AIM);
                }
                return;
            }
            _ => {}
        }
        // chain byte 0: he met a wall on the floor last frame (+0x658 bit 14)
        let wall = self.byte(0) != 0;
        self.set_byte(0, 0);
        if self.on_ground && self.jr_shell_ledge_wakes(level) {
            self.vx = 0.0;
            self.speed = 0.0;
            self.jr_enter_wakeup();
            return;
        }
        if self.dir != toward && self.speed.abs() > 1e-5 {
            self.jr_set_sub(SL_BRAKE);
        }
        let cooldown = self.byte(1);
        if cooldown > 0 {
            self.set_byte(1, cooldown - 1);
        }
        if cooldown <= 1 && self.pause == 0 && self.on_ground && self.jr_feet_on_floor(level) {
            self.jr_enter_wakeup();
            return;
        }
        if wall && self.on_ground && self.dir == toward && self.speed.abs() > 1e-5 {
            if self.jr_shell_obstacle(level) {
                self.vy = JR_SHELL_HOP_VY;
                self.speed = JR_SHELL_HOP_VX;
                self.vx = self.speed * self.dir as f32;
                self.home_y = self.x;
                self.on_ground = false;
                self.jr_set_sub(SL_HOP);
                self.fall_capped(level, G, CAP);
            } else {
                self.vx = 0.0;
                self.speed = 0.0;
                self.jr_enter_wakeup();
            }
            return;
        }
        if self.jr_at_area_edge(level) {
            self.jr_set_sub(SL_BRAKE);
        }
        self.heading = Self::approach(self.heading, JR_SHELL_ACCEL, JR_SHELL_JERK);
        self.speed = Self::approach(self.speed, JR_SHELL_TOP, self.heading);
        self.vx = self.speed * self.dir as f32;
        let ground = self.on_ground;
        self.fall_capped(level, G, CAP);
        if self.wall && ground && self.on_ground {
            self.set_byte(0, 1);
        } else if self.wall && !ground {
            // "koura_hit": back off the wall
            self.speed *= -0.2;
            self.vx = self.speed * self.dir as f32;
        }
    }

    /// ShellSlide's floor look under his front edge (x + 6·facing, big 12), each frame on the
    /// floor [obs v3.0.3 0x710108aa50 sub 2 head, LAB_710108b184]: from 6 px under his feet
    /// it scans down in 8-px steps to 142 px for a floor (one under the surface of lava, the
    /// area's liquid type 2, does not count). Floor that deep: he slides off. Floor within 6
    /// px, or none: if the floor is within 4 px of his feet at the front edge he slides on,
    /// else he looks 16 px further: solid at his feet + 4 there wakes him, floor within 4 px
    /// there lets him slide on, anything else (a drop, a pit, lava) wakes him (ShellWakeup).
    /// [inf: a slope's surface counts as floor for every ray (`jr_floor`); the game tilts the
    /// ±4 ray by the floor's angle]
    fn jr_shell_ledge_wakes(&self, level: &Level) -> bool {
        let f = self.dir as f32;
        let front = self.x + 6.0 * self.scale() * f;
        let lava = level.liquid.is_some_and(|l| l.kind == 2);
        let surface = self.w3.liquid.map_or(f32::NEG_INFINITY, |(y, _)| y);
        let mut k = -0.5_f32;
        while k >= -8.0 {
            let p = self.y + 16.0 * k + 2.0;
            if jr_floor(level, front, self.y, p, floor_in).is_some() {
                break;
            }
            if jr_floor(level, front, p, p - 16.0, ground_ray)
                .is_some_and(|hit| !(lava && hit < surface))
            {
                return false;
            }
            k -= 0.5;
        }
        if jr_floor(level, front, self.y + 4.0, self.y - 4.0, ground_ray).is_some() {
            return false;
        }
        let ahead = front + 16.0 * f;
        if level.cell_at(ahead, self.y + 4.0).is_solid() {
            return true;
        }
        !jr_floor(level, ahead, self.y + 4.0, self.y - 4.0, ground_ray)
            .is_some_and(|hit| (hit - self.y).abs() < 16.0)
    }

    /// ShellSlide meeting a wall on the floor, the frame after (the wall bit +0x658 bit 14
    /// with ground contact): true = hop it, false = come out of the shell (ShellWakeup, speed
    /// 0) [obs v3.0.3 0x710108aa50 sub 2]. From his front edge, h = 16 (32 under the
    /// water-speed rule): anything solid up the face from his feet + 8 to + 8 + h, or across
    /// the 16 px ahead at that height, wakes him. A further wall face 16..16 + 2h ahead at
    /// that height with floor within h under it 8 px short of it: the hop. Otherwise the test
    /// arc (vy 3.0, his gravity and fall cap; the water set, halved, under the rule): for a
    /// landing height of 16 px (32 under the rule) down to −128 in 8-px steps, the frames
    /// until the arc comes back down to it, at 1.5 px/f (halved under the rule; under it the
    /// frames to rise 16 px don't count when solid stands at his feet + 24 within 8 px of the
    /// front) — solid at that spot 2 px over the height wakes him, floor within 16 px under
    /// that spot is the hop; no height found wakes him.
    fn jr_shell_obstacle(&self, level: &Level) -> bool {
        let f = self.dir as f32;
        // just off the face he stands against [inf: the engine's body is 8 px a side (big 16)
        // where the game's background half-width is 6 (12); the rays start at the face, as
        // the game's do]
        let front = self.x + (self.half_w() - 0.01) * f;
        let slow = self.slow_rule(level);
        let h = if slow { 32.0 } else { 16.0 };
        let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
        let top = self.y + 8.0 + h;
        if floor_in(level, front, top, self.y + 8.0).is_some()
            || wall_in(level, top, front, front + 16.0 * f).is_some()
        {
            return false;
        }
        if wall_in(level, top, front + 16.0 * f, front + (16.0 + 2.0 * h) * f)
            .is_some_and(|face| jr_floor(level, face - 8.0 * f, top, top - h, ground_ray).is_some())
        {
            return true;
        }
        let skip = if slow && wall_in(level, self.y + 24.0, front, front + 8.0 * f).is_some() {
            jr_arc_frames(slow, |sum, _| sum >= 16.0) - 1
        } else {
            0
        };
        let mut k: f32 = if slow { 2.0 } else { 1.0 };
        while k >= -8.0 {
            let n = jr_arc_frames(slow, |sum, v| v < 0.0 && sum <= k * 16.0);
            let mut dx = f * 1.5 * (n - skip) as f32;
            if slow {
                dx *= 0.5;
            }
            let (x, y) = (front + dx, self.y + 16.0 * k + 2.0);
            if solid(x, y) {
                return false;
            }
            if jr_floor(level, x, y, y - 16.0, ground_ray).is_some() {
                return true;
            }
            k -= 0.5;
        }
        false
    }

    /// Both foot probes (x ∓ 4, big 8) find the floor within 10 px below.
    fn jr_feet_on_floor(&self, level: &Level) -> bool {
        let f = 4.0 * self.scale();
        [self.x - f, self.x + f]
            .iter()
            .all(|&fx| (0..=14).any(|k| level.cell_at(fx, self.y + 4.0 - k as f32).is_solid()))
    }

    /// ShellWakeup (0x7100616f80): the brake is the larger of the slide acceleration and a
    /// tenth of his speed (0x7100617170 sub 5).
    fn jr_enter_wakeup(&mut self) {
        self.phase = jr::SHELL_OUT;
        self.heading = self.heading.max(self.vx.abs() * 0.1);
        self.speed = self.vx.abs();
        self.jr_set_sub(5);
    }

    /// ShellWakeup: brakes to a stop; once his vertical speed is within the brake he hops out
    /// (vy 3.0, vx 0); at vy ≤ 0.4 the Basic action restarts with home here (ReturnFromShell
    /// 0x71006eea30 is not modelled beyond that [?]).
    fn jr_shell_wakeup(&mut self, level: &Level) {
        if self.jr_sub() == 5 {
            self.speed = (self.speed - self.heading).max(0.0);
            if self.jr_at_area_edge(level) {
                self.speed = 0.0;
            }
            self.vx = self.speed * self.dir as f32;
            if self.vy.abs() <= self.heading {
                self.vy = JR_WAKE_HOP;
                self.on_ground = false;
                self.vx = 0.0;
                self.speed = 0.0;
                self.jr_set_sub(6);
            }
            self.fall_capped(level, G, CAP);
            return;
        }
        self.vx = 0.0;
        self.fall_capped(level, G, CAP);
        if self.vy <= JR_WAKE_END {
            self.home_x = self.x;
            self.jr_set_sub(0);
            self.jr_rearm_cooldown();
            self.jr_enter_wait_or_fall();
        }
    }

    fn jr_enter_prev_hip_drop(&mut self, st: usize) {
        self.phase = jr::PREV_HIP_DROP;
        self.vx = 0.0;
        self.vy = 0.0;
        self.timer = if st == 3 { 10 } else { 20 };
    }

    /// Wait (0x71006ea960).
    fn jr_wait(&mut self, level: &Level) {
        if self.on_ground {
            self.vx = 0.0;
        }
        self.dir = self.toward_target();
        let cooldown = self.byte(2);
        let cooldown = if cooldown > 0 {
            self.set_byte(2, cooldown - 1);
            cooldown - 1
        } else {
            0
        };
        self.fall_capped(level, G, CAP);
        if self.timer != 0 {
            return;
        }
        let just_shot = self.jr_sub() == 1;
        let windup = JR_WINDUP[style_index(level)];
        if cooldown == 0 && self.on_ground {
            self.phase = jr::SHIFT_JUMP;
            self.vx = 0.0;
            self.vy = 0.0;
            self.speed = 0.0;
            self.heading = if self.big {
                JR_SHIFT_JUMP_BIG
            } else {
                JR_SHIFT_JUMP
            };
            self.jr_set_sub(JR_WINDING);
            self.timer = windup;
            return;
        }
        if !(self.on_ground && just_shot) && self.box_in_view() {
            let r = self.rand(100);
            if r < if self.wings { 75 } else { 70 } {
                self.phase = jr::SHOT;
                self.pause = 0;
                self.jr_set_sub(1);
                self.set_byte(1, 0);
                return;
            }
        }
        self.jr_set_sub(0);
        if !self.on_ground {
            return;
        }
        // which way: toward home, or at random within 24 px of it
        let heading = if (self.x - self.home_x).abs() <= JR_TURN_RANGE {
            if self.rand(2) == 0 { 1 } else { -1 }
        } else if self.home_x < self.x {
            -1
        } else {
            1
        };
        let ceiling = level
            .cell_at(self.x, self.y + self.height() + 16.0)
            .is_solid();
        if !ceiling {
            for d in [heading, -heading] {
                // the solved jump waits for the wind-up animation (SMB1: none)
                if let Some(v) = self.jr_hop_probe(level, d) {
                    self.phase = jr::HOP;
                    self.vx = 0.0;
                    self.speed = v.0 * d as f32;
                    self.heading = v.1;
                    self.jr_set_sub(JR_WINDING);
                    self.timer = windup;
                    return;
                }
            }
        }
        self.dir = if self.rand(2) == 0 { 1 } else { -1 };
        self.phase = jr::WALK;
        self.timer = JR_WALK_FRAMES;
    }

    /// The boss's view box touches the targeted player's camera view (0x710035f7f0: the box
    /// from slot 0x298 against the view rectangle, edges inclusive). The box (0x710078cbc0) is
    /// centred on the hitbox centre with half-extents (size / 2 + 16) / 2 from slot 0x2a0's
    /// size: Bowser Jr. 32 (big 64) square, Boom Boom 30 × 24 (big ×3).
    fn box_in_view(&self) -> bool {
        let (w, h) = match self.kind {
            Kind::BoomBoom { .. } => {
                let k = if self.big { 3.0 } else { 1.0 };
                (30.0 * k, 24.0 * k)
            }
            _ => {
                let k = if self.big { 64.0 } else { 32.0 };
                (k, k)
            }
        };
        let (hx, hy) = ((w / 2.0 + 16.0) / 2.0, (h / 2.0 + 16.0) / 2.0);
        let (x, y) = (self.x, self.y + self.height() / 2.0);
        let (cx, cy) = self.camera;
        x - hx <= cx + VIEW_W && x + hx >= cx && y - hy <= cy + VIEW_H && y + hy >= cy
    }

    /// The hop probe: 30 px ahead at feet + 4 and feet + 20; unless both are walls, look down
    /// from there (128 px when heading toward the player, else 16) for ground, and compute the
    /// jump that lands on it.
    fn jr_hop_probe(&self, level: &Level, d: i32) -> Option<(f32, f32)> {
        let df = d as f32;
        let ex = self.x + df * JR_PROBE;
        let wall_at = |y: f32| {
            let mut s = 1.0;
            while s <= JR_PROBE {
                if level.cell_at(self.x + df * s, y).is_solid() {
                    return true;
                }
                s += 4.0;
            }
            false
        };
        let low = wall_at(self.y + 4.0);
        let high = wall_at(self.y + 20.0);
        if low && high {
            return None;
        }
        let start = if low { self.y + 20.0 } else { self.y + 4.0 };
        let depth = if d == self.dir { 128.0 } else { 16.0 };
        let mut y = start;
        let mut land = None;
        while y >= self.y - depth {
            if level.cell_at(ex, y).is_solid() {
                land = Some(((y / TILE).floor() + 1.0) * TILE);
                break;
            }
            y -= 1.0;
        }
        let ly = land?;
        let v = jr_jump_velocity(JR_PROBE, ly - self.y, self.wings)?;
        self.jr_arc_clear(level, v.0 * df, v.1).then_some(v)
    }

    /// The arc test (0x7100619f50): the jump is flown frame by frame (gravity −0.1875, cap
    /// −4) from 1 px above his feet; every 15 frames the path since the last check is swept at
    /// the feet and at 0.33, 0.66 and 0.99 of his background height (15.9, big 31.8). The feet
    /// sweep landing on a floor ends the test: clear if the three higher sweeps to the landing
    /// point are clear. Any other hit, dropping 96 px below the area bottom, or 300 frames
    /// without landing fail. (Not modelled: the game also fails an arc that crosses the
    /// liquid surface.)
    fn jr_arc_clear(&self, level: &Level, vx: f32, mut vy: f32) -> bool {
        let h = 15.9 * self.scale();
        let rows = [0.33 * h, 0.66 * h, 0.99 * h];
        let (mut x, mut y) = (self.x, self.y + 1.0);
        let mut last = (x, y);
        for i in 0..300 {
            vy = (vy + G).max(CAP);
            x += vx;
            y += vy;
            if i % 15 != 0 {
                continue;
            }
            if last.1 + h + 16.0 <= level.bottom - 96.0 {
                return false;
            }
            if let Some((hx, hy, floor)) = sweep(level, last, (x, y)) {
                if !floor {
                    return false;
                }
                return rows
                    .iter()
                    .all(|&r| sweep(level, (last.0, last.1 + r), (hx, hy + r)).is_none());
            }
            if rows
                .iter()
                .any(|&r| sweep(level, (last.0, last.1 + r), (x, y + r)).is_some())
            {
                return false;
            }
            last = (x, y);
        }
        false
    }

    /// Walk (0x71006eb460): toward 1.25 (winged 0.8) in 0.1 steps; stops (→ Wait) 24 px from
    /// home, at a ledge or a wall, or when the 120 frames are up.
    fn jr_walk(&mut self, level: &Level) {
        let speed = if self.wings { JR_WALK_WINGS } else { JR_WALK };
        self.vx = Self::approach(self.vx, speed * self.dir as f32, JR_WALK_STEP);
        if (self.vx < 0.0 && self.x < self.home_x - JR_TURN_RANGE)
            || (self.vx > 0.0 && self.x > self.home_x + JR_TURN_RANGE)
        {
            self.vx = 0.0;
            self.fall_capped(level, G, CAP);
            return self.jr_enter_wait();
        }
        self.fall_capped(level, G, CAP);
        let ahead = self.x + self.dir as f32 * 4.0;
        let depth = if self.dir == self.toward_target() {
            128
        } else {
            16
        };
        let floor = (0..=depth)
            .step_by(4)
            .any(|dy| level.cell_at(ahead, self.y - 1.0 - dy as f32).is_solid());
        if !floor || self.wall {
            self.vx = 0.0;
            return self.jr_enter_wait();
        }
        if self.timer == 0 {
            self.jr_enter_wait();
        }
    }

    /// Fire (0x71006e5e20): three cycles of the 60-frame fire animation, a fireball 26
    /// frames into each.
    fn jr_fire_cycle(&mut self) -> Option<Actor> {
        let t = self.pause;
        let out = if t % JR_FIRE_ANIM == JR_FIRE_AT && t / JR_FIRE_ANIM < 3 {
            Some(self.jr_fireball())
        } else {
            None
        };
        if t >= 3 * JR_FIRE_ANIM {
            self.jr_enter_wait();
        }
        out
    }

    /// Hammers (winged, 0x71006e5790): four thrown 36, 52, 68 and 84 frames into the
    /// 110-frame throw animation.
    fn jr_hammers(&mut self) -> Option<Actor> {
        let t = self.pause;
        let out = JR_THROW_AT.contains(&t).then(|| self.make_hammer());
        if t >= JR_THROW_ANIM {
            self.jr_enter_wait();
        }
        out
    }

    /// A Bowser Jr. fireball: from 12 px ahead / 12 up (big 24 / 18), flying straight at the
    /// player's chest but within 25° of level, at 1.5 px/f plus his forward speed × cos²θ.
    fn jr_fireball(&mut self) -> Actor {
        let d = self.dir as f32;
        let (ox, oy) = if self.big { (24.0, 18.0) } else { (12.0, 12.0) };
        let (sx, sy) = (self.x + d * ox, self.y + oy);
        let (px, py) = self.target;
        let mut a = (py + 12.0 - sy).datan2((px - sx) * d).to_degrees();
        a = a.clamp(-JR_AIM, JR_AIM);
        let (sn, cs) = a.to_radians().dsin_cos();
        let carry = if self.vx * d > 0.0 {
            self.vx.abs()
        } else {
            0.0
        };
        let speed = BOWSER_FIRE_SPEED + carry * cs * cs;
        let mut f = Actor::new(Kind::BowserFire, sx, sy);
        f.dir = self.dir;
        f.vx = cs * speed * d;
        f.vy = sn * speed;
        f.home_y = f32::NAN;
        f.timer = 600;
        f
    }

    /// The Clown Car machine (init 0x71006e81c0). Wait (0x71006e84b0): the attack timer runs
    /// while the player is within 80 px or he is inside the view band (left + 32 … right − 32),
    /// and also before the player has ever been near; after that, 150 frames away clear it.
    /// At 0 he ducks (10 frames), throws two Green Koopa Troopas one each way after 30
    /// (ThrowShell 0x71006e88e0 → 0x71006e64d0), comes back up (10 frames) and at 1 HP also
    /// fires (or, winged, throws hammers); each attack ends with the timer at 150.
    fn jr_clown(&mut self, level: &Level) -> Option<Actor> {
        self.step_clown(level);
        let mut out = None;
        match self.phase {
            jr::CLOWN => {
                let near = (self.target.0 - self.x).abs() < 80.0
                    || (self.x >= self.camera.0 + 32.0 && self.x <= self.camera.0 + VIEW_W - 32.0);
                if near {
                    // chain byte 1: the player has been near; byte 2: the frames away
                    self.set_byte(1, 1);
                } else if self.byte(1) != 0 {
                    // +0x6c: counts down 150 frames away, clearing the attack timer whenever
                    // it runs out (and on the first frame away while it is 0)
                    let c = self.byte(2);
                    if c != 0 {
                        self.set_byte(2, c - 1);
                        if c != 1 {
                            return None;
                        }
                    }
                    self.pause = 0;
                    self.set_byte(2, JR_CLOWN_FORGET);
                    return None;
                }
                if self.pause > 0 {
                    self.pause -= 1;
                }
                if self.pause == 0 {
                    self.phase = jr::C_PRE_THROW;
                    self.timer = 10;
                }
            }
            jr::C_DAMAGE => {
                if self.timer == 0 {
                    self.phase = jr::CLOWN;
                }
            }
            jr::C_PRE_THROW => {
                if self.timer == 0 {
                    self.phase = jr::C_THROW;
                    self.timer = 30;
                    self.jr_set_sub(0);
                }
            }
            jr::C_THROW => {
                if self.timer == 0 {
                    // two Green Koopa Troopas (course actor 97, big 98, default spawn flags),
                    // left from x − 8 then right from x + 8 (big ±16), at y + 16 (big 24):
                    // vy 4.0 and ±0.5 px/f (0x71006e64d0; it also hands over his speed
                    // history, which is ~0 for a rider carried by the car; the game throws both
                    // in one frame, the engine one per step)
                    let k = self.jr_sub();
                    let d = if k == 0 { -1 } else { 1 };
                    let (ox, oy) = if self.big { (16.0, 24.0) } else { (8.0, 16.0) };
                    let mut s = Actor::new(
                        Kind::Koopa { red: false },
                        self.x + d as f32 * ox,
                        self.y + oy,
                    );
                    s.big = self.big;
                    s.dir = d;
                    s.vx = 0.5 * d as f32;
                    s.vy = 4.0;
                    out = Some(s);
                    if k == 0 {
                        self.jr_set_sub(1);
                    } else {
                        self.phase = jr::C_POST_THROW;
                        self.timer = 10;
                    }
                }
            }
            jr::C_POST_THROW => {
                if self.timer == 0 {
                    if self.hops == 1 {
                        self.phase = jr::C_SHOT;
                        self.pause = 0;
                    } else {
                        self.phase = jr::CLOWN;
                        self.pause = JR_CLOWN_ATTACK;
                    }
                }
            }
            _ => {
                self.pause += 1;
                out = if self.wings {
                    self.jr_hammers()
                } else {
                    self.jr_fire_cycle()
                };
                if self.phase == jr::WAIT {
                    self.phase = jr::CLOWN;
                    self.pause = JR_CLOWN_ATTACK;
                }
            }
        }
        out
    }
}

/// Boom Boom flag bits (chain byte 2).
const BBF_GUARD: u8 = 0x01;
const BBF_IMMUNE: u8 = 0x02;
const BBF_REPEAT: u8 = 0x04;

/// Sweeps the segment `a` → `b` through the terrain one pixel at a time: the first point
/// inside a solid cell, and whether it was a floor (entered from above while moving down).
fn sweep(level: &Level, a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32, bool)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let n = dx.abs().max(dy.abs()).ceil().max(1.0) as usize;
    let mut prev = a;
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let p = (a.0 + dx * t, a.1 + dy * t);
        if level.cell_at(p.0, p.1).is_solid() {
            let top = ((p.1 / TILE).floor() + 1.0) * TILE;
            let floor = dy < 0.0 && prev.1 >= top && !level.cell_at(p.0, prev.1).is_solid();
            return Some((p.0, if floor { top } else { p.1 }, floor));
        }
        prev = p;
    }
    None
}

/// The frames of ShellSlide's test arc (vy 3.0, gravity −0.1875 to the fall cap; under the
/// water-speed rule the water set by |vy| — −0.01875 under 0.3, −0.028125 under 0.7, −0.0375
/// under 1.2, else −0.075 — to half the cap, each step halved) before the one where `done`
/// (height so far, vy) holds [obs v3.0.3 0x710108aa50].
fn jr_arc_frames(slow: bool, done: impl Fn(f32, f32) -> bool) -> i32 {
    let (mut v, mut sum, mut n) = (JR_SHELL_HOP_VY, 0.0_f32, 0);
    loop {
        let step = if slow {
            let a = v.abs();
            let g = if a < 0.3 {
                -0.01875
            } else if a < 0.7 {
                -0.028125
            } else if a < 1.2 {
                -0.0375
            } else {
                -0.075
            };
            v = (v + g).max(CAP * 0.5);
            v * 0.5
        } else {
            v = (v + G).max(CAP);
            v
        };
        sum += step;
        if done(sum, v) {
            return n;
        }
        n += 1;
        if n > 10_000 {
            return n;
        }
    }
}

/// A downward ray for ShellSlide's looks: `ray` over the cells, or a slope's surface in the span
/// when that is higher.
fn jr_floor(
    level: &Level,
    x: f32,
    y_hi: f32,
    y_lo: f32,
    ray: fn(&Level, f32, f32, f32) -> Option<f32>,
) -> Option<f32> {
    let slope = level
        .slope_at(x)
        .map(|(y, _)| y)
        .filter(|&y| (y_lo..=y_hi).contains(&y));
    match (ray(level, x, y_hi, y_lo), slope) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// A ray at column `x` between `y_hi` and `y_lo` meeting solid terrain from any side: the top
/// of the first solid going down (`y_hi` itself when it starts inside). Rays that look for
/// floors only (the game's flag 8) are `ground_ray`, which also stops on one-way tops.
fn floor_in(level: &Level, x: f32, y_hi: f32, y_lo: f32) -> Option<f32> {
    let mut y = y_hi;
    loop {
        if level.cell_at(x, y).is_solid() {
            return Some(y_hi.min(((y / TILE).floor() + 1.0) * TILE));
        }
        let next = (y / TILE).floor() * TILE - 0.001;
        if next < y_lo {
            return None;
        }
        y = next;
    }
}

/// A sideways ray at height `y` from `x0` toward `x1`: the face of the first solid it meets.
fn wall_in(level: &Level, y: f32, x0: f32, x1: f32) -> Option<f32> {
    let d = if x1 >= x0 { 1.0 } else { -1.0 };
    let mut x = x0;
    loop {
        if level.cell_at(x, y).is_solid() {
            let c = (x / TILE).floor() * TILE;
            return Some(if d > 0.0 {
                c.max(x0)
            } else {
                (c + TILE).min(x0)
            });
        }
        let next = if d > 0.0 {
            ((x / TILE).floor() + 1.0) * TILE
        } else {
            (x / TILE).floor() * TILE - 0.001
        };
        if (next - x1) * d > 0.0 {
            return None;
        }
        x = next;
    }
}

/// The boss jump solver (0x7100619690): the launch (vx ≥ 0, vy) that lands `dy` above (or
/// below) a point `dx` away under gravity −0.1875 / fall cap −4, clearing 16 px (32 winged)
/// above the higher end; a steeper arc is tried while the speed would exceed 3 px/f.
fn jr_jump_velocity(dx: f32, dy: f32, winged: bool) -> Option<(f32, f32)> {
    let mut k = if winged { 2.0 } else { 1.0 };
    for _ in 0..8 {
        let h = if dy <= 0.0 { 16.0 * k } else { dy + 16.0 * k };
        let mut vy = (2.0 * -G * h).sqrt();
        for _ in 0..5 {
            let (mut v, mut rise) = (vy, 0.0);
            for _ in 0..180 {
                let n = v + G;
                if n <= 0.0 {
                    break;
                }
                v = n.max(CAP);
                rise += v;
            }
            if (h - rise).abs() <= 0.3 || rise <= 0.0 {
                break;
            }
            vy *= (h / rise).sqrt();
        }
        vy = vy.min(10.0);
        let (mut v, mut y, mut n) = (vy, 0.0, 0);
        loop {
            v = (v + G).max(CAP);
            y += v;
            if (v < 0.0 && y <= dy) || n > 400 {
                break;
            }
            n += 1;
        }
        if n > 0 {
            let vx = dx.abs() / n as f32;
            if vx <= 3.0 {
                return Some((vx, vy));
            }
        }
        k += 1.0;
    }
    None
}

/// A sprite choice: model, texture, width, height (texture size, ×2 when big), mirror.
pub type BossLook = (&'static str, &'static str, f32, f32, bool);

impl Actor {
    /// Model and texture for a boss or boss projectile; None while invisible (vanished, or the
    /// off-beat of the hurt flash). Every M1 boss texture faces left.
    pub fn boss_look(&self) -> Option<BossLook> {
        let flash = self.kick_lock > 0 && (self.kick_lock / 2) % 2 == 0;
        if flash || self.boss_hidden() {
            return None;
        }
        let cycle =
            |frames: &[&'static str], rate: u32| frames[(self.age / rate) as usize % frames.len()];
        let face = self.dir > 0;
        Some(match self.kind {
            Kind::Bowser { .. } => {
                let firing = matches!(
                    self.phase,
                    bw::FIRE | bw::JUMP_FIRE | bw::SPREAD_FIRE | bw::FIRE_FALLING
                );
                let tex = if self.boss_breathing() || firing {
                    "fire.0"
                } else if !self.on_ground && self.phase != bw::CLOWN {
                    cycle(&["fall.0", "fall.1"], 5)
                } else {
                    cycle(&["wait.0", "wait.1"], 20)
                };
                ("M1_Enemy_koopa", tex, 48.0, 48.0, face)
            }
            Kind::BowserFire => (
                "M1_Enemy_koopa",
                cycle(&["effect.0", "effect.1"], 2),
                24.0,
                16.0,
                self.vx > 0.0,
            ),
            Kind::BoomBoom { .. } => {
                const WALK: [&str; 6] =
                    ["walk.0", "walk.1", "walk.2", "walk.3", "walk.4", "walk.5"];
                const FLY: [&str; 6] = ["fly.0", "fly.1", "fly.2", "fly.3", "fly.4", "fly.5"];
                let f = (self.bb_frame() as usize / 4).min(2);
                let tex = match self.phase {
                    bb::W_DAMAGE => cycle(&["damage_fly.0", "damage_fly.1"], 5),
                    bb::DAMAGE_DOWN | bb::POM_DAMAGE => cycle(&["damage.0", "damage.1"], 5),
                    bb::DEFEND | bb::W_DEFEND if self.bb_sub() < 2 => {
                        ["squat.0", "squat.1", "squat.2"][f]
                    }
                    bb::DEFEND | bb::W_DEFEND => ["standup.0", "standup.1", "standup.2"][f],
                    bb::JUMP if self.bb_sub() == 0 => "jump_st.0",
                    bb::POM_THROW_SIGN => "jump_st.0",
                    bb::JUMP | bb::WALL_JUMP | bb::POM_JUMP => {
                        if self.vy > 0.0 {
                            "jump_ed.0"
                        } else {
                            "jump_ed.1"
                        }
                    }
                    bb::W_WAIT | bb::W_FLY | bb::W_ATTACK | bb::CLOWN => cycle(&FLY, 5),
                    // walk_ftp (24 frames, uneven keys); at 1 HP walk2_ftp (18, every 3)
                    _ if self.hops == 1 => cycle(&WALK, 3),
                    _ => {
                        const KEYS: [u32; 6] = [0, 6, 9, 12, 18, 21];
                        let t = self.age % 24;
                        WALK[KEYS.iter().rposition(|&k| k <= t).unwrap_or(0)]
                    }
                };
                ("M1_Enemy_bunbun", tex, 32.0, 32.0, face)
            }
            // Pom Pom (3D World only) throws BulletPunpunShuriken, drawn with the 3D World model
            // `3W_Enemy_punpunshuriken` (its one animation, `Spin`, 35 frames) through the
            // `M1_` name like the other 3D World enemies; it has no 2D sprite in any style
            Kind::Shuriken => ("M1_Enemy_punpunshuriken", "spin.0", 16.0, 16.0, false),
            Kind::BowserJr { .. } => {
                if self.boss_in_shell() {
                    let tex = if self.phase == jr::SHELL {
                        cycle(&["shell.0", "shell.1", "shell.2", "shell.3"], 4)
                    } else {
                        "shell.0"
                    };
                    return Some(("M1_Enemy_koopaJr", tex, 16.0, 16.0, face));
                }
                let tex = match self.phase {
                    jr::SHOT | jr::C_SHOT if self.wings => {
                        let t = self.pause % JR_THROW_ANIM;
                        if t < 30 || (t / 10) % 2 == 0 {
                            "throw.0"
                        } else {
                            "throw.1"
                        }
                    }
                    jr::SHOT | jr::C_SHOT if self.pause % JR_FIRE_ANIM < 25 => "fire.0",
                    jr::SHOT | jr::C_SHOT => "fire.1",
                    jr::PREV_HIP_DROP | jr::HIP_DROP => "before.0",
                    jr::CLOWN | jr::C_PRE_THROW | jr::C_THROW | jr::C_POST_THROW => "wait.0",
                    // ducked into the car between the Clown DamageDown's 22nd and 68th frame
                    jr::C_DAMAGE if (9..=55).contains(&self.timer) => return None,
                    jr::C_DAMAGE => "wait.0",
                    _ if !self.on_ground => cycle(&["fall.0", "fall.1"], 5),
                    _ => cycle(&["wait.0", "wait.1"], 10),
                };
                ("M1_Enemy_koopaJr", tex, 32.0, 32.0, face)
            }
            _ => return None,
        })
    }
}

/// A downward BG ray at x from `top` to `bottom`: the first terrain surface it meets (the top
/// of a solid cell, or of a one-way top it crosses), if any [inf: the game's BG ray 0x7100606de0
/// taken as a cell scan].
fn ground_ray(level: &Level, x: f32, top: f32, bottom: f32) -> Option<f32> {
    if level.cell_at(x, top).is_solid() {
        return Some(top);
    }
    let mut row = (top / TILE).floor() - 1.0;
    while (row + 1.0) * TILE >= bottom {
        let surface = (row + 1.0) * TILE;
        let cell = level.cell_at(x, row * TILE + TILE * 0.5);
        if (cell.is_solid() || cell == Cell::SemiTop) && surface <= top {
            return Some(surface);
        }
        row -= 1.0;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boss(kind: Kind, x: f32) -> (Level, Actor) {
        let level = Level::flat(80);
        let mut a = Actor::new(kind, x, 32.0);
        a.on_ground = true;
        a.dir = -1;
        a.target = (x - 100.0, 32.0);
        // the camera shows the boss
        a.camera = (x - VIEW_W / 2.0, 0.0);
        (level, a)
    }

    const BOWSER: Kind = Kind::Bowser {
        key: false,
        clown: false,
    };
    const BOOM_BOOM: Kind = Kind::BoomBoom {
        key: false,
        clown: false,
        pom: false,
    };
    const BOWSER_JR: Kind = Kind::BowserJr {
        key: false,
        clown: false,
    };

    #[test]
    fn the_chase_scans_jump_over_walls_and_gaps() {
        let (mut level, mut a) = boss(BOWSER, 30.0 * TILE);
        a.dir = 1;
        // flat ground ahead: nothing in the way, no gap
        assert_eq!(a.bowser_obstacle_kind(&level).0, 1);
        assert!(!a.bowser_gap_ahead(&level).0);
        // a 2-tile wall right ahead: free from 2 tiles up, the middle jump
        let tx = ((a.x + 24.0) / TILE).floor() as i32;
        level.set_solid(tx, 2);
        level.set_solid(tx, 3);
        assert_eq!(a.bowser_obstacle_kind(&level).0, 1);
        // a 4-tile one: the big jump; a 7-tile one: no way over
        level.set_solid(tx, 4);
        level.set_solid(tx, 5);
        assert_eq!(a.bowser_obstacle_kind(&level), (2, 4));
        for y in 6..9 {
            level.set_solid(tx, y);
        }
        assert_eq!(a.bowser_obstacle_kind(&level).0, 0);
        // a gap: no ground under 26 px ahead nor nearer, ground again 4 tiles on
        let mut level = Level::flat(80);
        let gx = (a.x / TILE).floor() as i32 + 1;
        for x in gx..gx + 4 {
            level.set(x, 0, Cell::Empty);
            level.set(x, 1, Cell::Empty);
        }
        let (gap, from) = a.bowser_gap_ahead(&level);
        assert!(gap);
        // the landing is searched from 40 px past where it looked: speed |d| / 2 · 0.1 / 3
        a.vy = 3.0;
        let v = a.bowser_gap_speed(&level, from);
        assert!(v > 0.0 && v <= 1.8, "{v}");
        // in the chase it middle-jumps across (SMB1 / SMB3)
        let mut b = a.clone();
        assert!(a.bowser_chase_jumps(&level, 1));
        assert_eq!(a.phase, bw::MIDDLE_JUMP);
        assert!(a.gap_jump && a.vx > 0.0);
        // SMW big-jumps it at the landing speed (slot 0x310), NSMBU after BeforeBigJump
        let mut c = b.clone();
        assert!(b.bowser_chase_jumps(&level, 2));
        assert_eq!(b.phase, bw::BIG_JUMP);
        assert!(c.bowser_chase_jumps(&level, 3));
        assert_eq!(c.phase, bw::BEFORE_BIG_JUMP);
    }

    #[test]
    fn solid_terrain_overhead_stops_the_chase_jump() {
        let (mut level, mut a) = boss(BOWSER, 30.0 * TILE);
        level.style = "WU".to_string();
        a.dir = 1;
        let tx = ((a.x + 24.0) / TILE).floor() as i32;
        level.set_solid(tx, 2);
        level.set_solid(tx, 3);
        let (kind, rows) = a.bowser_obstacle_kind(&level);
        assert_eq!((kind, rows), (1, 2));
        // NSMBU: the lines reach 16 · 2 + 36 = 68 px over his feet: a ceiling 4 tiles up blocks
        // it, a semisolid there doesn't, one 6 tiles up is out of reach
        assert!(!a.bowser_blocked_overhead(&level, rows));
        let (mx, row) = ((a.x / TILE).floor() as i32, ((a.y + 64.0) / TILE) as i32);
        level.set(mx, row, Cell::SemiTop);
        assert!(!a.bowser_blocked_overhead(&level, rows));
        level.set_solid(mx, row);
        assert!(a.bowser_blocked_overhead(&level, rows));
        assert!(!a.bowser_chase_jumps(&level, 2), "no jump over the wall");
        level.set(mx, row, Cell::Empty);
        level.set_solid(mx, row + 2);
        assert!(!a.bowser_blocked_overhead(&level, rows));
        assert!(a.bowser_chase_jumps(&level, 2));
    }

    #[test]
    fn a_trampoline_over_bowser_stops_jumps_that_reach_it() {
        let (mut level, mut a) = boss(BOWSER, 30.0 * TILE);
        level.style = "WU".to_string();
        a.dir = 1;
        // NSMBU: reach 3.0 at −0.1 → 43.4 + 36 + 16; big 5.9 at −0.1875 → 89.8 + 36 + 16
        assert_eq!(a.bowser_height_search(&level), None);
        assert!(
            (a.spring_reach_mid - 95.4).abs() < 0.01,
            "{}",
            a.spring_reach_mid
        );
        assert!(
            (a.spring_reach_big - 141.8).abs() < 0.01,
            "{}",
            a.spring_reach_big
        );
        // a Trampoline 80 px over his feet, over his middle column
        a.overhead_units = vec![(a.x - 8.0, a.y + 80.0, a.x + 8.0, a.y + 95.0)];
        assert_eq!(a.bowser_height_search(&level), Some(80.0));
        // out of his columns' way: none
        a.overhead_units = vec![(a.x + 40.0, a.y + 80.0, a.x + 56.0, a.y + 95.0)];
        assert_eq!(a.bowser_height_search(&level), None);
        // terrain over every column, lower than the Trampoline: hidden
        a.overhead_units = vec![(a.x - 8.0, a.y + 80.0, a.x + 8.0, a.y + 95.0)];
        let row = ((a.y + 48.0) / TILE) as i32;
        for tx in -3..=3 {
            level.set_solid((a.x / TILE).floor() as i32 + tx, row);
        }
        assert_eq!(a.bowser_height_search(&level), None);
        for tx in -3..=3 {
            level.set((a.x / TILE).floor() as i32 + tx, row, Cell::Empty);
        }
        // the chase: a low wall would be middle-jumped (reach 95.4 ≥ 80): refused
        let tx = ((a.x + 24.0) / TILE).floor() as i32;
        level.set_solid(tx, 2);
        a.spring_over = a.bowser_height_search(&level);
        assert!(!a.bowser_chase_jumps(&level, 2));
        // higher up (100 px) the middle jump goes ahead
        a.overhead_units = vec![(a.x - 8.0, a.y + 100.0, a.x + 8.0, a.y + 115.0)];
        a.spring_over = a.bowser_height_search(&level);
        assert_eq!(a.spring_over, Some(100.0));
        assert!(a.bowser_chase_jumps(&level, 2));
    }

    #[test]
    fn bowser_paces_within_64_px_of_home_at_0_3() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        let home = a.home_x;
        let (mut lo, mut hi) = (home, home);
        let mut walk_speeds = Vec::new();
        for _ in 0..4000 {
            // the player stays on his left, so he never chases
            a.target.0 = home - 150.0;
            let _ = a.step_spawning(&level);
            lo = lo.min(a.x);
            hi = hi.max(a.x);
            if a.phase == bw::WALK && a.on_ground && a.vx != 0.0 {
                walk_speeds.push(a.vx.abs());
            }
        }
        // a decision comes at most 39 frames (≤ 12 px) after crossing a bound
        assert!(
            lo >= home - 64.0 - 12.0 && hi <= home + 64.0 + 12.0,
            "{lo}..{hi}"
        );
        assert!(hi - lo > 64.0, "actually paces: {lo}..{hi}");
        assert!(walk_speeds.iter().all(|&v| (v - 0.3).abs() < 1e-6));
    }

    #[test]
    fn smb1_bowser_breathes_35_frames_then_fires_every_40_to_79_more() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let mut shots = Vec::new();
        let mut breath_len = 0;
        let mut breathing = 0;
        for f in 0..3000 {
            if let Some(fire) = a.step_spawning(&level) {
                assert_eq!(fire.kind, Kind::BowserFire);
                // 1.5 px/f plus his own forward speed (0 or the 0.3 walk)
                let carry = fire.vx.abs() - BOWSER_FIRE_SPEED;
                assert!((-1e-6..=0.3 + 1e-6).contains(&carry), "{}", fire.vx);
                assert_eq!(fire.vx.signum() as i32, a.dir);
                // three lanes: level, or 17 px up / down
                let lane = fire.home_y - fire.y;
                assert!([-17.0, 0.0, 17.0].iter().any(|l| (lane - l).abs() < 1e-3));
                shots.push(f);
                breath_len = breath_len.max(breathing);
            }
            if a.boss_breathing() {
                breathing += 1;
            } else {
                breathing = 0;
            }
        }
        assert!(shots.len() > 20);
        assert_eq!(breath_len, 35, "the SMB1 fire animation is 35 frames");
        for w in shots.windows(2) {
            let gap = w[1] - w[0];
            assert!((39 + 35..=79 + 35).contains(&gap), "gap {gap}");
        }
    }

    #[test]
    fn bowser_stands_still_while_breathing_in_smb1() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let mut was = (0, false, 0);
        let mut checked = 0;
        for _ in 0..2000 {
            let _ = a.step_spawning(&level);
            // the walk reads the breath flag the fire helper left on the previous frame (its
            // entry, also on a decision that walks again, still sets the walking speed)
            let same_walk = was.0 == bw::WALK && a.timer < was.2;
            if a.phase == bw::WALK && same_walk && was.1 && a.boss_breathing() {
                assert_eq!(a.vx, 0.0);
                checked += 1;
            }
            was = (a.phase, a.boss_breathing(), a.timer);
        }
        assert!(checked > 0);
    }

    #[test]
    fn bowser_flame_climbs_to_its_lane_then_flies_level() {
        let mut f = Actor::new(Kind::BowserFire, 100.0, 52.0);
        f.vx = -1.5;
        f.vy = 0.75;
        f.home_y = 69.0;
        f.timer = 600;
        let level = Level::flat(20);
        for _ in 0..40 {
            f.step_boss_shot(&level);
        }
        assert!((f.y - 69.25).abs() < 1e-3, "{}", f.y);
        assert_eq!(f.vy, 0.0);
        assert!((f.x - 40.0).abs() < 1e-3);
    }

    #[test]
    fn smb1_bowser_middle_jumps_3_0_and_never_big_jumps() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let mut jumps = 0;
        for _ in 0..3000 {
            let was = a.phase;
            let _ = a.step_spawning(&level);
            if a.phase == bw::MIDDLE_JUMP && was != a.phase {
                jumps += 1;
                assert!((a.vy - 3.0).abs() < 1e-6);
            }
            assert!(!matches!(a.phase, bw::BIG_JUMP | bw::LITTLE_JUMP));
        }
        assert!(jumps >= 5, "{jumps} middle jumps");
    }

    #[test]
    fn smb3_bowser_hops_low_and_skips_middle_jumps_under_a_low_trampoline() {
        let (mut level, mut a) = boss(BOWSER, 30.0 * TILE);
        level.style = "M3".to_string();
        // little hop reach: 1.1 at −0.07 → 8.08 + 31.8 + 16
        let _ = a.step_spawning(&level);
        assert!(
            (a.spring_reach_little - 55.88).abs() < 0.01,
            "{}",
            a.spring_reach_little
        );
        // a Trampoline 50 px over his feet: within the hop's, middle and big jumps' reaches
        let (mut hops, mut low) = (0, 0);
        for _ in 0..3000 {
            a.overhead_units = vec![(a.x - 8.0, a.y + 50.0, a.x + 8.0, a.y + 65.0)];
            let was = a.phase;
            let _ = a.step_spawning(&level);
            if a.phase == bw::LITTLE_JUMP && was != a.phase {
                hops += 1;
                low += i32::from((a.vy - 0.7).abs() < 1e-6);
            }
            assert!(!matches!(a.phase, bw::BIG_JUMP | bw::MIDDLE_JUMP));
        }
        assert!(hops > 20 && low == hops, "{low} of {hops} hops at 0.7");
        // without it: 1.1 hops, and middle jumps again
        a.overhead_units.clear();
        let mut middle = 0;
        for _ in 0..3000 {
            let was = a.phase;
            let _ = a.step_spawning(&level);
            if a.phase == bw::LITTLE_JUMP && was != a.phase {
                assert!((a.vy - 1.1).abs() < 1e-6, "vy {}", a.vy);
            }
            middle += i32::from(a.phase == bw::MIDDLE_JUMP && was != a.phase);
        }
        assert!(middle > 0);
    }

    #[test]
    fn bowser_jump_counters_follow_the_styles_tables() {
        let (_, mut a) = boss(BOWSER, 30.0 * TILE);
        let seen = |a: &mut Actor, st: usize, mode: u8| {
            let b = (a.byte(3) & !bwf::MODE) | mode;
            a.set_byte(3, b);
            let (mut j, mut g) = (
                std::collections::BTreeSet::new(),
                std::collections::BTreeSet::new(),
            );
            for _ in 0..400 {
                a.bowser_reset_jump(st);
                a.bowser_reset_big(st);
                j.insert(a.byte(0));
                g.insert(a.byte(1));
            }
            (
                j.into_iter().collect::<Vec<_>>(),
                g.into_iter().collect::<Vec<_>>(),
            )
        };
        assert_eq!(seen(&mut a, 0, 0), (vec![3, 4], vec![5, 6]));
        assert_eq!(seen(&mut a, 0, 2), (vec![4, 5], vec![9, 10]));
        assert_eq!(seen(&mut a, 1, 1), (vec![4, 5, 6], vec![9, 10]));
        // SMW (0x7101157fa0 / 0x7101158030)
        assert_eq!(seen(&mut a, 2, 0), (vec![1, 2, 3], vec![5, 6, 7, 8]));
        assert_eq!(seen(&mut a, 2, 1), (vec![1, 2], vec![8, 9, 10]));
        assert_eq!(seen(&mut a, 2, 2), (vec![3, 4], vec![9, 10, 11]));
        assert_eq!(seen(&mut a, 2, 3), (vec![1, 2, 3], vec![7, 8, 9]));
        // NSMBU (0x7101152cb0 / 0x7101152d40)
        assert_eq!(seen(&mut a, 3, 0), (vec![1, 2, 3], vec![5, 6, 7, 8]));
        assert_eq!(seen(&mut a, 3, 1), (vec![1, 2], vec![8, 9, 10]));
        assert_eq!(seen(&mut a, 3, 2), (vec![4, 5], vec![7, 8, 9]));
        assert_eq!(seen(&mut a, 3, 3), (vec![1, 2, 3], vec![7, 8, 9]));
    }

    #[test]
    fn smb1_bowser_turns_on_the_spot_when_the_player_gets_behind() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        assert_eq!(a.dir, -1);
        // the player goes past his back edge (13 px)
        a.target.0 = a.x + 14.0;
        let mut turned = None;
        for f in 0..200 {
            let _ = a.step_spawning(&level);
            if a.dir > 0 {
                turned = Some(f);
                break;
            }
        }
        assert!(turned.is_some(), "turned round");
    }

    #[test]
    fn small_bowser_chases_the_player_who_got_past_him() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        let mut top: f32 = 0.0;
        for _ in 0..600 {
            a.target.0 = a.x + 100.0;
            let _ = a.step_spawning(&level);
            if a.phase == bw::WALK && !a.boss_breathing() {
                top = top.max(a.vx);
            }
        }
        assert_eq!(a.bowser_mode(), 2, "chase mode");
        assert!((top - 0.7).abs() < 1e-6, "SMB1 chase speed {top}");
    }

    #[test]
    fn off_screen_bowser_waits_until_in_view() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        a.camera = (0.0, 0.0); // the view is far to his left
        let x0 = a.x;
        for _ in 0..600 {
            let _ = a.step_spawning(&level);
        }
        assert_eq!(a.bowser_mode(), 3);
        assert!((a.x - x0).abs() < 1e-3, "stands in place");
        a.camera = (a.x - VIEW_W / 2.0, 0.0);
        for _ in 0..200 {
            let _ = a.step_spawning(&level);
        }
        assert_eq!(a.bowser_mode(), 0, "patrols once on screen");
    }

    #[test]
    fn smb3_bowser_big_jumps_then_hip_drops_and_looks_around() {
        let (mut level, mut a) = boss(BOWSER, 30.0 * TILE);
        level.style = "M3".into();
        let mut top_vy: f32 = 0.0;
        let mut hip = false;
        let mut looked = false;
        for _ in 0..3000 {
            let _ = a.step_spawning(&level);
            if a.phase == bw::BIG_JUMP {
                top_vy = top_vy.max(a.vy);
            }
            hip |= a.phase == bw::HIP_DROP;
            looked |= hip && a.phase == bw::LOOK_AROUND;
        }
        assert!((top_vy - 6.2).abs() < 1e-4, "{top_vy}");
        assert!(hip && looked);
    }

    #[test]
    fn a_block_bump_hops_bowser_without_damage() {
        let (level, mut a) = boss(BOWSER, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        assert_eq!(a.boss_hit(BossAttack::BlockHit, a.x), BossHit::Absorbed);
        assert_eq!(a.phase, bw::HIT_BLOCK);
        assert!((a.vy - 2.2).abs() < 1e-6);
        assert_eq!(a.boss_hp(), 20);
    }

    #[test]
    fn boom_boom_starts_curled_up_then_walks_slowly() {
        let (level, mut a) = boss(BOOM_BOOM, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        assert_eq!(a.phase, bb::DEFEND);
        let mut hop = None;
        for _ in 0..200 {
            let _ = a.step_spawning(&level);
            if a.vy > 2.0 && hop.is_none() {
                hop = Some(a.vy);
            }
            if a.phase == bb::WALK {
                break;
            }
        }
        assert!((hop.expect("hopped") - BB_DEFEND_HOP).abs() < 0.2);
        assert_eq!(a.phase, bb::WALK);
        let _ = a.step_spawning(&level);
        assert!((a.vx.abs() - BB_SLOW).abs() < 1e-6, "slow walk {}", a.vx);
    }

    #[test]
    fn boom_boom_wall_jumps_three_times_the_third_high_then_turns() {
        let (mut level, mut a) = boss(BOOM_BOOM, 30.0 * TILE);
        // a wall 6 tiles high just right of it, the player beyond it
        for y in 2..8 {
            level.set_solid(33, y);
        }
        a.target = (34.0 * TILE, 32.0);
        a.dir = 1;
        let mut wall_jumps = Vec::new();
        for _ in 0..2000 {
            let was = a.phase;
            let _ = a.step_spawning(&level);
            a.target = (a.x + 40.0, 32.0);
            if a.phase == bb::WALL_JUMP && was != bb::WALL_JUMP {
                wall_jumps.push(a.vy);
            }
            if a.dir < 0 && a.phase == bb::WALK {
                break;
            }
        }
        assert_eq!(wall_jumps.len(), 3, "{wall_jumps:?}");
        assert!((wall_jumps[0] - BB_WALL_JUMP).abs() < 1e-6);
        assert!((wall_jumps[2] - BB_WALL_JUMP_HIGH).abs() < 1e-6);
        assert_eq!(a.dir, -1, "turned after the 4th bump");
    }

    #[test]
    fn boom_boom_floats_while_stunned() {
        let (level, mut a) = boss(BOOM_BOOM, 30.0 * TILE);
        let _ = a.step_spawning(&level);
        a.y += 20.0;
        a.on_ground = false;
        assert_eq!(a.boss_hit(BossAttack::Shell, a.x), BossHit::Hurt);
        let y = a.y;
        for _ in 0..60 {
            let _ = a.step_spawning(&level);
        }
        assert_eq!(a.phase, bb::DAMAGE_DOWN);
        assert_eq!(a.y, y, "no gravity in DamageDown");
        assert!(a.boss_guarding());
    }

    #[test]
    fn winged_boom_boom_hovers_then_swoops_and_keeps_its_wings() {
        let (level, mut a) = boss(BOOM_BOOM, 30.0 * TILE);
        a.wings = true;
        a.y = 120.0;
        a.on_ground = false;
        a.target = (a.x - 60.0, 32.0);
        let mut attack = None;
        for f in 0..600 {
            let _ = a.step_spawning(&level);
            if a.phase == bb::W_ATTACK && attack.is_none() {
                attack = Some(f);
            }
            assert!(a.vx.abs() <= BB_FLY_SPEED + 1e-6);
        }
        let f = attack.expect("swooped");
        assert!((239..=302).contains(&f), "attack at {f}");
        // a stomp costs 1 HP but not the wings
        let mut b = a.clone();
        b.phase = bb::W_FLY;
        assert_eq!(b.boss_hit(BossAttack::Stomp, b.x), BossHit::Hurt);
        assert!(b.wings);
        assert_eq!(b.phase, bb::W_DAMAGE);
        assert_eq!(b.boss_hp(), 2);
    }

    #[test]
    fn bowser_jr_shift_jumps_straight_up_and_hip_drops_to_minus_8() {
        let (level, mut a) = boss(BOWSER_JR, 30.0 * TILE);
        let mut jumped = false;
        let mut min_vy: f32 = 0.0;
        for _ in 0..1500 {
            let (was, vy) = (a.phase, a.vy);
            let _ = a.step_spawning(&level);
            if a.phase == jr::SHIFT_JUMP && was == jr::SHIFT_JUMP && vy == 0.0 && a.vy > 0.0 {
                // SMB1: no wind-up, the jump starts on the state's first frame
                jumped = true;
                assert_eq!(a.vx, 0.0);
                assert!((a.vy - (JR_SHIFT_JUMP + G)).abs() < 1e-6, "{}", a.vy);
            }
            if a.phase == jr::HIP_DROP {
                min_vy = min_vy.min(a.vy);
            }
            assert_ne!(a.phase, jr::HOVER, "only winged Bowser Jr. hovers");
        }
        assert!(jumped);
        assert!(min_vy <= JR_HIP_CAP + 1e-6, "{min_vy}");
    }

    #[test]
    fn winged_bowser_jr_hovers_50_to_74_frames() {
        let (level, mut a) = boss(BOWSER_JR, 30.0 * TILE);
        a.wings = true;
        let mut hovers = Vec::new();
        let mut len = 0;
        for _ in 0..4000 {
            let _ = a.step_spawning(&level);
            if a.phase == jr::HOVER {
                len += 1;
            } else if len > 0 {
                hovers.push(len);
                len = 0;
            }
        }
        assert!(!hovers.is_empty());
        assert!(hovers.iter().all(|h| (49..=74).contains(h)), "{hovers:?}");
    }

    #[test]
    fn bowser_jr_fires_three_aimed_shots_per_volley() {
        let (level, mut a) = boss(BOWSER_JR, 30.0 * TILE);
        a.target = (a.x - 120.0, 32.0);
        let mut volleys: Vec<Vec<f32>> = Vec::new();
        let mut was = 0;
        for _ in 0..3000 {
            let out = a.step_spawning(&level);
            if a.phase == jr::SHOT && was != jr::SHOT {
                volleys.push(Vec::new());
            }
            if let Some(f) = out {
                assert_eq!(f.kind, Kind::BowserFire);
                let speed = (f.vx * f.vx + f.vy * f.vy).sqrt();
                assert!((speed - 1.5).abs() < 1e-4);
                assert!(f.vy.datan2(-f.vx).to_degrees().abs() <= JR_AIM + 1e-3);
                volleys.last_mut().expect("in a volley").push(f.x);
            }
            was = a.phase;
        }
        volleys.pop(); // the last one may be cut off
        assert!(!volleys.is_empty());
        assert!(volleys.iter().all(|v| v.len() == 3), "{volleys:?}");
    }

    #[test]
    fn bowser_jr_hops_onto_a_step_ahead() {
        let (mut level, mut a) = boss(BOWSER_JR, 30.0 * TILE);
        // a one-tile step 1–2 tiles ahead in every direction he might pick
        for x in [27, 28, 32, 33] {
            level.set_solid(x, 2);
        }
        let mut hop = false;
        for _ in 0..600 {
            let _ = a.step_spawning(&level);
            hop |= a.phase == jr::HOP && a.vy > 0.0;
        }
        assert!(hop, "hopped");
    }

    #[test]
    fn pom_pom_hops_vanishes_after_a_hit_and_reappears() {
        let (level, mut a) = boss(
            Kind::BoomBoom {
                key: false,
                clown: false,
                pom: true,
            },
            30.0 * TILE,
        );
        let _ = a.step_spawning(&level);
        let mut b = a.clone();
        // a shell: the 2.5 hop, FireBallDown 40 + FireBallDownEnd 76, then Lapse
        assert_eq!(a.boss_hit(BossAttack::Shell, a.x), BossHit::Hurt);
        assert!((a.vy - 2.5).abs() < 1e-6, "hop 2.5");
        let (mut seen, mut hidden, mut moved) = (0, 0, false);
        let start = a.x;
        for _ in 0..600 {
            let _ = a.step_spawning(&level);
            if a.boss_hidden() {
                hidden += 1;
            } else if hidden > 0 {
                moved = (a.x - start).abs() > 1.0;
                break;
            } else {
                seen += 1;
            }
        }
        assert_eq!(seen, POM_DAMAGE_HOP + POM_DAMAGE_END - 1, "damage frames");
        assert!(hidden >= POM_LAPSE + POM_LAPSE_MOVE, "{hidden}");
        assert!(!a.boss_hidden() && moved, "reappears elsewhere");
        assert_eq!(a.phase, bb::POM_APPEAR);
        // a stomp: PressDown in place, no hop
        assert_eq!(b.boss_hit(BossAttack::Stomp, b.x), BossHit::Hurt);
        assert_eq!((b.vy, b.timer), (0.0, POM_PRESS));
    }

    #[test]
    fn pom_pom_throws_one_homing_shuriken_then_hides() {
        let (level, mut a) = boss(
            Kind::BoomBoom {
                key: false,
                clown: false,
                pom: true,
            },
            30.0 * TILE,
        );
        a.target = (a.x - 100.0, 64.0);
        let mut shots = Vec::new();
        let mut thrown_at = None;
        for f in 0..400 {
            if let Some(s) = a.step_spawning(&level) {
                shots.push(s);
                thrown_at.get_or_insert(f);
            }
            if a.phase == bb::POM_HIDE {
                break;
            }
        }
        // Wait 60 (frames 0–59), ThrowSign 42 → the shuriken as Throw starts
        assert_eq!(thrown_at, Some(59 + POM_THROW_SIGN[0]));
        assert_eq!(shots.len(), 1);
        let mut s = shots.remove(0);
        assert_eq!(s.dir, -1);
        let mut climb: f32 = 0.0;
        let flat = Level::flat(20);
        for _ in 0..60 {
            s.step_boss_shot(&flat);
            climb = climb.max(s.heading);
            assert!((s.vx.dhypot(s.vy) - 1.0).abs() < 1e-4);
        }
        // the target is 32 px up: it turns up by 2° a frame, never past 45°
        assert!(climb > 10.0 && climb <= 45.0, "{climb}");
    }

    fn styled(mut level: Level, style: &str) -> Level {
        level.style = style.into();
        level
    }

    #[test]
    fn bowser_turn_and_breath_lengths_follow_the_models() {
        let (_, mut a) = boss(BOWSER, 30.0 * TILE);
        assert_eq!(a.bowser_turn_frames(1), 15.0); // SMB3 turn 30 at rate 2
        assert_eq!(a.bowser_turn_frames(2), 23.0); // SMW 45 at 2, rounded up
        assert_eq!(a.bowser_turn_frames(3), 54.0); // NSMBU lkp_walk 160 at 3
        a.big = true;
        assert_eq!(a.bowser_turn_frames(3), 160.0);
        assert_eq!(BOWSER_BREATH[3], (104, 48));
    }

    #[test]
    fn bowser_flame_carries_the_mean_of_his_last_16_steps() {
        let (_, mut a) = boss(BOWSER, 30.0 * TILE);
        a.dir = 1;
        for k in 0..20 {
            a.x = 100.0 + 0.5 * k as f32;
            a.bowser_record_step();
        }
        assert_eq!(a.trail.len(), SPEED_HISTORY + 1);
        let f = a.bowser_flame(0);
        assert!((f.vx - (BOWSER_FIRE_SPEED + 0.5)).abs() < 1e-5, "{}", f.vx);
        // moving backward: nothing is carried
        a.dir = -1;
        let f = a.bowser_flame(0);
        assert!((f.vx + BOWSER_FIRE_SPEED).abs() < 1e-5);
    }

    #[test]
    fn bowser_jr_hop_and_shift_jump_wait_for_the_wind_up() {
        for (style, wind) in [("M3", 4), ("WU", 25)] {
            let (level, mut a) = boss(BOWSER_JR, 30.0 * TILE);
            let level = styled(level, style);
            let mut winding = 0;
            let mut launched = false;
            for _ in 0..600 {
                let _ = a.step_spawning(&level);
                if matches!(a.phase, jr::HOP | jr::SHIFT_JUMP) && a.jr_sub() == JR_WINDING {
                    winding += 1;
                    assert_eq!(a.vx, 0.0);
                } else if winding > 0 {
                    launched = a.vy > 0.0;
                    break;
                }
            }
            assert!(launched, "{style}");
            assert_eq!(winding, wind, "{style}");
        }
    }

    #[test]
    fn the_view_test_uses_the_boss_box_with_inclusive_edges() {
        let (_, mut a) = boss(BOWSER_JR, 30.0 * TILE);
        // Bowser Jr.'s view box: ±16 around his hitbox centre
        a.camera = (a.x + 16.0, 0.0);
        assert!(a.box_in_view());
        a.camera = (a.x + 16.5, 0.0);
        assert!(!a.box_in_view());
    }

    #[test]
    fn the_hop_arc_fails_under_a_low_ceiling_and_clears_open_ground() {
        let (mut level, a) = boss(BOWSER_JR, 30.0 * TILE);
        assert!(a.jr_arc_clear(&level, 1.0, 2.0));
        // a block 1 tile above his head along the arc
        let (tx, ty) = (((a.x + 16.0) / TILE) as i32, 4);
        level.set(tx, ty, Cell::Solid);
        level.set(tx + 1, ty, Cell::Solid);
        assert!(!a.jr_arc_clear(&level, 1.0, 3.5));
    }

    #[test]
    fn bowser_jr_slides_for_200_frames_at_up_to_1_6_then_hops_out() {
        let (level, mut a) = boss(BOWSER_JR, 60.0 * TILE);
        a.phase = jr::SHELL_WAIT;
        a.hops = 2;
        a.timer = 1;
        // far enough that he never passes the player (which would brake and turn him)
        a.target = (a.x - 900.0, 32.0);
        let mut slide = 0;
        let mut top: f32 = 0.0;
        for _ in 0..400 {
            let _ = a.step_spawning(&level);
            if a.phase == jr::SHELL {
                slide += 1;
                top = top.max(a.vx.abs());
            }
            if a.phase == jr::SHELL_OUT && a.jr_sub() == 6 {
                break;
            }
        }
        assert!(
            (JR_SHELL_FRAMES..=JR_SHELL_FRAMES + 2).contains(&slide),
            "{slide}"
        );
        assert!((top - JR_SHELL_TOP).abs() < 1e-5, "{top}");
        assert_eq!(a.vx, 0.0);
        assert!((a.vy - JR_WAKE_HOP - G).abs() < 1e-5 || a.vy < JR_WAKE_HOP);
        // the hit rules in the shell: a stomp bounces off, a block bump knocks it up
        a.phase = jr::SHELL;
        assert_eq!(a.boss_hit(BossAttack::Stomp, a.x), BossHit::Guarded);
        assert_eq!(a.boss_hit(BossAttack::Fire, a.x), BossHit::Ignored);
        assert_eq!(a.boss_hit(BossAttack::BlockHit, a.x), BossHit::Absorbed);
        assert_eq!((a.vy, a.boss_hp()), (2.0, 2));
    }

    /// Bowser Jr. sliding left in his shell at full speed, on the floor (y 32).
    fn jr_sliding(level: Level, x: f32) -> (Level, Actor) {
        let (_, mut a) = boss(BOWSER_JR, x);
        a.phase = jr::SHELL;
        a.pause = JR_SHELL_FRAMES;
        a.jr_set_sub(SL_SLIDE);
        a.dir = -1;
        a.speed = JR_SHELL_TOP;
        a.heading = JR_SHELL_ACCEL;
        a.vx = -JR_SHELL_TOP;
        a.target = (a.x - 2000.0, 32.0);
        (level, a)
    }

    /// Steps until he leaves the shell or `stop` holds (at most 300 frames).
    fn jr_run(level: &Level, a: &mut Actor, stop: impl Fn(&Actor) -> bool) {
        for _ in 0..300 {
            let _ = a.step_spawning(level);
            if a.phase != jr::SHELL || stop(a) {
                return;
            }
        }
    }

    #[test]
    fn bowser_jr_in_his_shell_hops_a_step_and_a_block() {
        // a step one tile high, four wide (tiles 26..=29): the arc lands on it
        let mut level = Level::flat(80);
        for x in 26..=29 {
            level.set_solid(x, 2);
        }
        let (level, mut a) = jr_sliding(level, 40.0 * TILE);
        jr_run(&level, &mut a, |a| a.jr_sub() == SL_HOP);
        assert_eq!((a.phase, a.jr_sub()), (jr::SHELL, SL_HOP));
        assert!((a.vx + JR_SHELL_HOP_VX).abs() < 1e-5, "{}", a.vx);
        jr_run(&level, &mut a, |a| a.jr_sub() == SL_SLIDE);
        assert_eq!(a.jr_sub(), SL_SLIDE);
        assert_eq!(a.y, 48.0);
        // a single block: the test arc lands beyond it, so he hops; the face holds him back
        // while he rises, so he comes down on it and slides off the far side
        let mut level = Level::flat(80);
        level.set_solid(29, 2);
        let (level, mut a) = jr_sliding(level, 40.0 * TILE);
        jr_run(&level, &mut a, |a| a.jr_sub() == SL_HOP);
        assert_eq!(a.jr_sub(), SL_HOP);
        jr_run(&level, &mut a, |a| a.jr_sub() == SL_SLIDE);
        assert_eq!(a.y, 48.0);
        jr_run(&level, &mut a, |a| a.y == 32.0 && a.on_ground);
        assert_eq!((a.phase, a.y), (jr::SHELL, 32.0));
        assert!(a.x < 29.0 * TILE);
    }

    #[test]
    fn bowser_jr_comes_out_of_his_shell_at_a_tall_wall_or_a_pit() {
        // a wall two tiles high
        let mut level = Level::flat(80);
        level.set_solid(29, 2);
        level.set_solid(29, 3);
        let (level, mut a) = jr_sliding(level, 40.0 * TILE);
        jr_run(&level, &mut a, |_| false);
        assert_eq!(a.phase, jr::SHELL_OUT);
        assert!(a.x > 30.0 * TILE);
        // a pit: no floor under his front edge (tiles 20..=29 open)
        let mut level = Level::flat(80);
        for x in 20..=29 {
            level.set(x, 0, Cell::Empty);
            level.set(x, 1, Cell::Empty);
        }
        let (level, mut a) = jr_sliding(level, 40.0 * TILE);
        jr_run(&level, &mut a, |_| false);
        assert_eq!(a.phase, jr::SHELL_OUT);
        assert!((a.x - (30.0 * TILE + 6.0)).abs() < 2.0, "{}", a.x);
        // a drop to a floor two tiles down: he slides off it
        let mut level = Level::flat(80);
        for x in 0..=29 {
            level.set(x, 1, Cell::Empty);
        }
        for x in 30..80 {
            level.set_solid(x, 2);
            level.set_solid(x, 3);
        }
        let (level, mut a) = jr_sliding(level, 40.0 * TILE);
        a.y = 64.0;
        jr_run(&level, &mut a, |a| a.y < 20.0);
        assert_eq!(a.phase, jr::SHELL);
        assert!(a.y < 20.0);
    }

    #[test]
    fn bowser_jr_in_his_shell_slides_down_a_slope() {
        // a plateau 4 tiles high from tile 16 with a "/" slope down to the floor at tile 10
        let mut level = Level::flat(80);
        level.add_test_slope(
            crate::level::Slope {
                left: 10,
                bottom: 1,
                width: 6,
                height: 3,
                rising: true,
                steep: false,
            },
            16,
            4,
        );
        let (level, mut a) = jr_sliding(level, 24.0 * TILE);
        a.y = 64.0;
        jr_run(&level, &mut a, |a| a.x < 8.0 * TILE);
        assert_eq!(a.phase, jr::SHELL, "{} {}", a.x, a.y);
        assert!(a.x < 8.0 * TILE && a.y < 40.0, "{} {}", a.x, a.y);
    }

    #[test]
    fn a_clown_car_rider_steers_at_0_02_up_to_1_and_drifts_out_of_range() {
        let clown = Kind::BowserJr {
            key: false,
            clown: true,
        };
        let (level, mut a) = boss(clown, 30.0 * TILE);
        a.y = 100.0;
        a.on_ground = false;
        a.target = (a.x + 200.0, 100.0);
        let _ = a.step_spawning(&level);
        assert!(
            (a.vx.dhypot(a.vy) - CLOWN_ACCEL).abs() < 1e-3,
            "{} {}",
            a.vx,
            a.vy
        );
        for _ in 0..200 {
            a.target = (a.x + 200.0, a.y);
            let _ = a.step_spawning(&level);
        }
        assert!((a.vx - CLOWN_SPEED).abs() < 0.05, "{}", a.vx);
        // beyond 320 px it lets go (Free drift)
        a.target = (a.x + 400.0, a.y);
        let v = a.vx;
        let _ = a.step_spawning(&level);
        assert!((a.vx - v * 0.95).abs() < 1e-4);
        // the car under him: 21 px below Bowser Jr., 38 below an SMB3 Bowser
        assert_eq!(a.clown_ride_height("M1"), 21.0);
        let (_, b) = boss(BOWSER, 0.0);
        assert_eq!(b.clown_ride_height("M3"), 38.0);
    }

    #[test]
    fn bowser_jr_in_a_clown_car_ducks_when_hurt_and_throws_koopa_troopas() {
        let clown = Kind::BowserJr {
            key: false,
            clown: true,
        };
        let (level, mut a) = boss(clown, 30.0 * TILE);
        a.y = 100.0;
        a.on_ground = false;
        a.target = (a.x - 40.0, 100.0);
        let mut thrown = Vec::new();
        for _ in 0..80 {
            if let Some(s) = a.step_spawning(&level) {
                thrown.push(s);
            }
        }
        assert_eq!(thrown.len(), 2);
        for (s, d) in thrown.iter().zip([-1, 1]) {
            assert_eq!(s.kind, Kind::Koopa { red: false });
            assert_eq!((s.vx, s.vy, s.dir), (0.5 * d as f32, 4.0, d));
        }
        a.phase = jr::CLOWN;
        assert_eq!(a.boss_hit(BossAttack::Stomp, a.x), BossHit::Hurt);
        assert_eq!((a.phase, a.timer), (jr::C_DAMAGE, JR_CLOWN_DAMAGE));
        assert_eq!(a.boss_hit(BossAttack::Stomp, a.x), BossHit::Guarded);
    }

    #[test]
    fn jump_solver_lands_on_target() {
        let (vx, vy) = jr_jump_velocity(30.0, 16.0, false).expect("solution");
        // simulate
        let (mut x, mut y, mut v) = (0.0f32, 0.0f32, vy);
        loop {
            v = (v + G).max(CAP);
            x += vx;
            y += v;
            if v < 0.0 && y <= 16.0 {
                break;
            }
        }
        assert!((x - 30.0).abs() <= vx + 1e-3, "{x}");
        assert!(vx <= 3.0);
    }
}
