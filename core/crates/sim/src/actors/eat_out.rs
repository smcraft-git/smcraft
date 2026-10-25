//! A power-up Yoshi spat out: the item's EatOut state [obs v3.0.3].
//!
//! A power-up is never swallowed: its eat handler (the item's vfunc 0x5a0 0x7101351110, vtable
//! 0x7102927b98) takes its type from the EnemyDB `eat_react` column through the table
//! 0x71023cb894 {Invalid 0, GulpDown 2, SpitOut 1, Fire 4, Bone 5, SnowBall 6} (0 for a big
//! one, vfunc 0x5b0 0x7101078190; set up by the actor start 0x7101286ee0), and the mouth's
//! catch (0x71015fdc10) swallows only types 2 / 3 (Drink); every power-up row is SpitOut, so it
//! is held (Success 0x71015fc5e0, the item in its Ride context 0x7101053e40 with its speeds
//! zeroed) until Yoshi spits it. The spit (handler slot 0x28 0x710134f780 → 0x710134f870, the
//! release event 0x710129c7c0 → the Basic context's enter 0x710133d5f0) puts it in EatOut
//! (state 4): x speed 2.5 the way Yoshi faces (half with the release's drop flag), plus the
//! rider's speed when that makes 2.5 or more; y speed 1.0 (0 for the item type +0xd20 = 0x10 [?:
//! which part; not modelled]); gravity −0.1875 down to −4; one px
//! up; three landings (one when dropped or put down). EatOut (0x710133e2c0 / 0x710133e350 /
//! 0x710133e4f0): untouchable for 6 frames (12 under the water-speed rule; actor +0x528 bit
//! 20); each landing but the last bounces it up at 1.5 with its x speed × 0.8 (0x7102318a54); a
//! wall turns it round with its speed reversed; the last landing ends it in the item's own
//! motion.

use super::{Actor, ENEMY_GRAVITY, Kind, MAX_FALL};
use crate::level::Level;

/// The spit's x speed (0x7102319ce8 by facing).
const SPIT_SPEED: f32 = 2.5;
/// The y speed it leaves at, and a bounce's.
const SPIT_VY: f32 = 1.0;
const BOUNCE_VY: f32 = 1.5;
/// A bounce keeps this much of the x speed.
const BOUNCE_KEEP: f32 = 0.8;
/// Frames it can't be touched (doubled under the water-speed rule).
const LOCK: u8 = 6;

/// How Yoshi lets a held power-up go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    /// Spat out (kind 5).
    Spit,
    /// Put down where he is (kind 7: no speed, one landing) [inf: the mouth event that calls
    /// the handler's slot 0x18 (0x710134fc90) — taken as Yoshi losing his rider].
    PutDown,
}

impl Actor {
    /// Power-up `kind` leaving Yoshi's mouth at (`x`, `y`): facing `facing`, the rider moving at
    /// `rider_vx`.
    pub fn spat_out(
        level: &Level,
        kind: Kind,
        (x, y): (f32, f32),
        facing: i32,
        rider_vx: f32,
        how: Release,
    ) -> Actor {
        let mut a = Actor::new(kind, x, y + 1.0);
        a.dir = facing;
        a.on_ground = false;
        match how {
            Release::Spit => {
                let base = SPIT_SPEED * facing as f32;
                let with = base + rider_vx;
                a.vx = if with.abs() >= SPIT_SPEED { with } else { base };
                a.vy = SPIT_VY;
                a.eat_out = 3;
            }
            Release::PutDown => {
                a.vx = 0.0;
                a.vy = 0.0;
                a.eat_out = 1;
            }
        }
        a.eat_lock = if a.slow_rule(level) { LOCK * 2 } else { LOCK };
        a
    }

    /// One frame of EatOut; the last landing ends it, and the item moves on its own from the
    /// next frame.
    pub(super) fn step_eat_out(&mut self, level: &Level) {
        self.eat_lock = self.eat_lock.saturating_sub(1);
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        self.collide(level);
        if self.on_ground {
            self.eat_out -= 1;
            if self.eat_out == 0 {
                self.vx = 0.0;
                self.vy = 0.0;
                return;
            }
            self.vx *= BOUNCE_KEEP;
            self.vy = BOUNCE_VY;
            self.on_ground = false;
        }
        if self.wall {
            self.dir = -self.dir;
            self.vx = -self.vx;
        }
    }
}
