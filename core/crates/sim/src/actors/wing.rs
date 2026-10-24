//! Enemies' own Wing actions (the EnemySys Wing context, research decomp/objects actionmap).
//!
//! - **Bob-omb** (Bombhei Wing: Fly / Turn) [obs v3.0.3: the flight helper 0x71010833a0 run by
//!   Fly's exe 0x71010831e0 and Turn's exe 0x7101083300; v1.0.0 Fly 0x710060de20 / 0x710060df20,
//!   Turn 0x710060dff0 / 0x710060e030, helper 0x710060e0d0]: no gravity; it flies at 0.5 px/f
//!   the way it faces and bobs between 8 px above and 8 px below where it started, its vertical
//!   speed easing toward ±0.5 by 0.03 a frame (starting upward); a floor met going down sends it
//!   up at 0.5 at once, a ceiling met going up down at 0.5. A wall ahead turns it (Turn: the
//!   facing flips and the speed is kept until the turn ends, then Fly takes 0.5 the new way).
//!   Not ported [inf]: the helper's drop (gravity −0.1875 until it lands, then on from there)
//!   on a BG flag the engine doesn't model (+0x659 bit 2 / +0xcfe bit 5), and the downward
//!   start of one coming out of a pipe downward.
//! - **Lava Bubble** (Bubble Wing: the Default Wing class with Bubble's own init and Jump
//!   [obs v3.0.3: vtable 0x71028bf830, init 0x710109f400, Jump exe 0x710109f9b0, the bounce
//!   0x710109da60; v1.0.0 0x7100627d00 / 0x7100628170 / 0x7100626480]): no gravity; it flies
//!   in a straight line at 1 px/f and bounces off terrain like a ball. A placed one sets off
//!   up and to the left (its heading its rotation + 45°, the velocity that heading + 90°). It
//!   is drawn turned toward its heading, turning at most 8° a frame (the shorter way within
//!   90°, else on the way it was turning); only once the turn is complete and at least 4
//!   frames after the last bounce is a contact checked: each surface touched (a wall on the
//!   right at 90°, on the left at −90°, the floor 0°, the ceiling 180°; two of them, a
//!   corner: their mean) that it is moving into reflects its direction, d' = 2·s − d, and the
//!   new heading is that direction's. Between the checks a wall just holds it.
//!   [inf: terrain keeps its velocity; slopes' own surface angles, the actor-contact bounce
//!   (+0x21) and the latch on BG bit 34 are not modelled; a winged one out of a pipe or a
//!   block flies as a placed one]

use super::{Actor, TURN_LOCK};
use crate::fmath::{DetMath, table_atan2, table_sin_cos};
use crate::level::Level;

/// The Bob-omb's flight speed, bob half-height and vertical ease [obs v3.0.3: Fly exe
/// 0x71010831e0 (0.5 × facing), helper 0x71010833a0 (±8, ±0.5, DAT 0x7102318844 / 0x71023190a8
/// = ∓0.03)].
const BOMB_SPEED: f32 = 0.5;
const BOMB_BOB: f32 = 8.0;
const BOMB_VY: f32 = 0.5;
const BOMB_EASE: f32 = 0.03;
/// The helper's vertical modes (+0x68): 2 up, 3 down (a floor or ceiling flips them through
/// table 0x7102320460 = {1, 0, 3, 2}).
const UP: u8 = 2;
const DOWN: u8 = 3;

/// The winged Lava Bubble's speed (unit direction vectors from the sine table), its turn rate
/// (0x5b05b00 ≈ 8°), the frames between bounce checks (+0x78 = 4) and its start: heading =
/// rotation + 0x20000000 for a placed one (+0x408 = −1) [obs v3.0.3 0x710109f400].
const BUBBLE_TURN: i32 = 0x05b0_5b00;
const BUBBLE_LOCK: u8 = 4;
const BUBBLE_START: u32 = 0x2000_0000;
/// The bounce's dead zone for a velocity component, ±1e-5 (0x7102318cf4 / 0x7102318820).
const BUBBLE_EPS: f32 = 1e-5;
/// `phase` of a flying winged Lava Bubble (0 is the hidden phase, harmless).
pub(super) const BUBBLE_WING: u8 = 3;

/// The terrain a winged Lava Bubble touched in its last move.
#[derive(Clone, Copy, Default)]
struct Touch {
    right: bool,
    left: bool,
    floor: bool,
    ceiling: bool,
}

impl Actor {
    /// One frame of a winged Lava Bubble (module docs). `fly.angle` holds its drawn rotation
    /// (+0x264) and heading (+0x74), `timer` the rotation of the frame before (+0x7c), `hops`
    /// the frames left before a bounce check (+0x78), `fly.home` the contacts of the last move.
    pub(super) fn step_winged_bubble(&mut self, level: &Level) {
        if !self.fly.on {
            self.fly.on = true;
            self.phase = BUBBLE_WING;
            self.fly.angle = (0, BUBBLE_START);
            self.timer = 0;
            self.hops = 0;
            let (s, c) = table_sin_cos(BUBBLE_START.wrapping_add(0x4000_0000));
            self.vx = c;
            self.vy = s;
            self.fly.home = (0.0, 0.0);
        }
        let touch = Touch {
            right: self.fly.home.0 > 0.0,
            left: self.fly.home.0 < 0.0,
            floor: self.fly.home.1 < 0.0,
            ceiling: self.fly.home.1 > 0.0,
        };
        // Jump's exe: the bounce check once the turn is complete, every 4 frames at most
        let (rot, head) = self.fly.angle;
        let c = self.hops;
        if c > 0 {
            self.hops = c - 1;
        }
        if (c == 0 || c == 1) && bubble_aligned(rot, head) && self.bubble_bounce(touch) {
            self.hops = BUBBLE_LOCK;
            let n = self.vx.dhypot(self.vy);
            let (nx, ny) = if n > 0.0 {
                (self.vx * (1.0 / n), self.vy * (1.0 / n))
            } else {
                (self.vx, self.vy)
            };
            self.fly.angle.1 = table_atan2(nx, -ny) ^ 0x8000_0000;
        }
        self.bubble_turn();
        // the move: no gravity, the velocity kept against terrain
        self.fly.home = self.bubble_move(level);
    }

    /// The drawn rotation's turn toward the heading (the end of Jump's exe).
    fn bubble_turn(&mut self) {
        let (rot, head) = (self.fly.angle.0 as i32, self.fly.angle.1 as i32);
        let prev = self.timer;
        let turning = rot.wrapping_sub(prev);
        self.timer = rot;
        let near = (rot.wrapping_add(0x3fff_ffbf).wrapping_sub(head) as u32) < 0x7fff_ff7f;
        let new = if rot == prev || near {
            let d = head.wrapping_sub(rot);
            if d > BUBBLE_TURN {
                rot.wrapping_add(BUBBLE_TURN)
            } else if d < -BUBBLE_TURN {
                rot.wrapping_sub(BUBBLE_TURN)
            } else {
                head
            }
        } else {
            // keep turning the way it was going until within a step of the heading
            let step = if turning < 1 {
                -BUBBLE_TURN
            } else {
                BUBBLE_TURN
            };
            let d = head.wrapping_sub(rot);
            if d == 0 {
                rot
            } else if (turning < 1 && (d >= 0 || d.wrapping_sub(step) < 1))
                || (turning >= 1 && (d < 1 || d.wrapping_sub(step) >= 0))
            {
                rot.wrapping_add(step)
            } else {
                head
            }
        };
        self.fly.angle.0 = new as u32;
        if new == head {
            self.timer = new;
        }
    }

    /// The bounce (0x710109da60): the surfaces touched that it moves into reflect its
    /// direction; true when it bounced.
    fn bubble_bounce(&mut self, t: Touch) -> bool {
        let mut s: Vec<u32> = Vec::new();
        for (on, a) in [
            (t.right, 0x4000_0000u32),
            (t.left, 0xc000_0000),
            (t.floor, 0),
            (t.ceiling, 0x8000_0000),
        ] {
            if on && s.len() < 2 && self.bubble_reflect(a).is_some() {
                s.push(a);
            }
        }
        let surface = match s[..] {
            [] => return false,
            [a] => a,
            [a, b, ..] => {
                // a corner: the mean of the two (the far side of it when they wrap)
                let mean = ((b as i32 as f32 * 0.5) as i32)
                    .wrapping_add((a as i32 as f32 * 0.5) as i32) as u32;
                let (a, b) = (a as i32, b as i32);
                let d = if a.wrapping_sub(b) == 0 || a < b {
                    b.wrapping_sub(a)
                } else {
                    a.wrapping_sub(b)
                };
                if d < 0 { mean ^ 0x8000_0000 } else { mean }
            }
        };
        match self.bubble_reflect(surface) {
            Some((vx, vy)) => {
                self.vx = vx;
                self.vy = vy;
                true
            }
            None => false,
        }
    }

    /// The direction reflected off a surface at binary angle `s` when moving into it.
    fn bubble_reflect(&self, s: u32) -> Option<(f32, f32)> {
        let still = (-BUBBLE_EPS..=BUBBLE_EPS).contains(&self.vx)
            && (-BUBBLE_EPS..=BUBBLE_EPS).contains(&self.vy);
        if still {
            return None;
        }
        let d = table_atan2(self.vy, self.vx);
        let rel = d.wrapping_sub(s) as i32;
        if rel == i32::MIN || rel >= 0 {
            return None;
        }
        let (sin, cos) = table_sin_cos(s.wrapping_mul(2).wrapping_sub(d));
        let flat = |v: f32| (-BUBBLE_EPS..=BUBBLE_EPS).contains(&v);
        if flat(cos) && flat(sin) {
            return None;
        }
        Some((cos, sin))
    }

    /// The move against terrain: each axis stops at a solid cell, the velocity kept. Returns
    /// the contacts as (x side, y side): +1 right / ceiling, −1 left / floor.
    fn bubble_move(&mut self, level: &Level) -> (f32, f32) {
        let (hw, h) = (self.half_w(), self.height());
        let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
        let mut touch = (0.0, 0.0);
        let nx = self.x + self.vx;
        let side = if self.vx >= 0.0 { hw } else { -hw };
        let edge = nx + side;
        if (0..=2).any(|k| solid(edge, self.y + 1.0 + (h - 2.0) * k as f32 / 2.0)) {
            let cell = (edge / 16.0).floor() * 16.0;
            self.x = if side > 0.0 {
                cell - hw
            } else {
                cell + 16.0 + hw
            };
            touch.0 = side.signum();
        } else {
            self.x = nx;
        }
        let ny = self.y + self.vy;
        let (foot, up) = (ny, ny + h);
        let xs = [self.x - hw + 1.0, self.x, self.x + hw - 1.0];
        if self.vy > 0.0 && xs.iter().any(|&x| solid(x, up)) {
            self.y = (up / 16.0).floor() * 16.0 - h;
            touch.1 = 1.0;
        } else if self.vy <= 0.0 && xs.iter().any(|&x| solid(x, foot)) {
            self.y = (foot / 16.0).floor() * 16.0 + 16.0;
            touch.1 = -1.0;
        } else {
            self.y = ny;
        }
        touch
    }

    /// One frame of a winged Bob-omb's Fly / Turn. `hops` holds the helper's mode (0 before
    /// Fly's enter), `home_y` its base height.
    pub(super) fn step_winged_bobomb(&mut self, level: &Level) {
        if self.hops == 0 {
            // Fly enter: no gravity, at rest vertically, going up from here
            self.hops = UP;
            self.home_y = self.y;
            self.vy = 0.0;
        }
        // Turn keeps the speed it had; Fly runs at 0.5 the way it faces
        if self.turn_lock == 0 {
            self.vx = BOMB_SPEED * self.dir as f32;
        }
        if self.hops == UP && self.y > self.home_y + BOMB_BOB {
            self.hops = DOWN;
        } else if self.hops == DOWN && self.y < self.home_y - BOMB_BOB {
            self.hops = UP;
        }
        let target = if self.hops == DOWN { -BOMB_VY } else { BOMB_VY };
        self.vy = if self.vy < target {
            (self.vy + BOMB_EASE).min(target)
        } else {
            (self.vy - BOMB_EASE).max(target)
        };
        let rising = self.vy > 0.0;
        self.collide(level);
        // a floor met going down, or a ceiling going up, turns the bob round at once
        if self.hops == DOWN && self.on_ground {
            self.vy = BOMB_VY;
            self.hops = UP;
        } else if self.hops == UP && rising && self.vy == 0.0 {
            self.vy = -BOMB_VY;
            self.hops = DOWN;
        }
        if self.wall && self.turn_lock == 0 {
            // Turn: the facing flips, the speed is kept until it ends
            self.dir = -self.dir;
            self.turn_lock = TURN_LOCK;
        }
    }
}

/// Jump exe's test that the drawn rotation has reached the heading: their difference in
/// radians, wrapped to (−π, π], within 1.2e-7 (0x7102319590), worked in f32 as the game does.
fn bubble_aligned(rot: u32, head: u32) -> bool {
    use std::f32::consts::{PI, TAU};
    let k = f32::from_bits(0x3000_0000); // 2^-31
    let mut f = rot as i32 as f32 * PI * k - head as i32 as f32 * PI * k;
    f -= (f * (1.0 / TAU)).floor() * TAU;
    if !(f > 0.0 && f < TAU) && f < TAU {
        f = 0.0;
    }
    if PI <= f {
        f -= TAU;
    }
    f.abs() <= f32::from_bits(0x3400_0000)
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Kind;
    use crate::fmath::DetMath;
    use crate::level::TILE;

    #[test]
    fn a_winged_bobomb_flies_level_bobbing_8_px() {
        let lvl = Level::flat(80);
        let mut a = Actor::new(Kind::BobOmb, 40.0 * TILE, 80.0);
        a.wings = true;
        a.dir = -1;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..400 {
            a.step_winged_bobomb(&lvl);
            lo = lo.min(a.y);
            hi = hi.max(a.y);
            assert!(a.vy.abs() <= BOMB_VY + 1e-6);
        }
        assert!((a.x - (40.0 * TILE - 200.0)).abs() < 1e-2, "{}", a.x);
        // it turns round past ±8 and eases over the turn: about 8 + 0.5² / (2 · 0.03) beyond
        assert!(
            hi > 88.0 && hi < 93.0 && lo < 72.0 && lo > 67.0,
            "{lo} {hi}"
        );
    }

    #[test]
    fn a_floor_sends_it_up_and_a_wall_turns_it() {
        let mut lvl = Level::flat(80);
        lvl.set_solid(37, 2);
        let mut a = Actor::new(Kind::BobOmb, 40.0 * TILE, 36.0);
        a.wings = true;
        a.dir = -1;
        let mut bounced = false;
        let mut turned = false;
        for _ in 0..200 {
            let down = a.hops == DOWN;
            a.step_winged_bobomb(&lvl);
            bounced |= down && a.hops == UP && a.vy == BOMB_VY;
            turned |= a.dir == 1;
        }
        assert!(bounced, "the floor turned the bob round");
        assert!(turned, "the block turned it");
    }

    /// A walled room: the flat ground (top y 32), a ceiling row 12 (bottom y 192), walls at
    /// columns 30 and 50.
    fn room() -> Level {
        let mut lvl = Level::flat(80);
        for x in 0..80 {
            lvl.set_solid(x, 12);
        }
        for y in 0..13 {
            lvl.set_solid(30, y);
            lvl.set_solid(50, y);
        }
        lvl
    }

    #[test]
    fn a_winged_lava_bubble_sets_off_up_left_at_1_px_a_frame() {
        let lvl = room();
        let mut a = Actor::new(Kind::LavaBubble, 40.0 * TILE, 100.0);
        a.wings = true;
        a.spawn = Some(0);
        let (x, y) = (a.x, a.y);
        a.step_winged_bubble(&lvl);
        assert!((a.x - x + 0.70710677).abs() < 1e-4 && (a.y - y - 0.70710677).abs() < 1e-4);
        assert!((a.vx.dhypot(a.vy) - 1.0).abs() < 1e-5);
        assert_eq!(a.phase, BUBBLE_WING);
    }

    #[test]
    fn it_bounces_off_walls_floor_and_ceiling_and_stays_in_the_room() {
        let lvl = room();
        let mut a = Actor::new(Kind::LavaBubble, 40.0 * TILE, 100.0);
        a.wings = true;
        let (mut left, mut right, mut up, mut down) = (false, false, false, false);
        for _ in 0..3000 {
            a.step_winged_bubble(&lvl);
            assert!(a.x - 8.0 >= 31.0 * TILE - 1e-3 && a.x + 8.0 <= 50.0 * TILE + 1e-3);
            assert!(a.y >= 32.0 - 1e-3 && a.y + 16.0 <= 192.0 + 1e-3, "{}", a.y);
            assert!((a.vx.dhypot(a.vy) - 1.0).abs() < 1e-4);
            left |= a.vx < 0.0;
            right |= a.vx > 0.0;
            up |= a.vy > 0.0;
            down |= a.vy < 0.0;
        }
        assert!(left && right && up && down);
    }

    #[test]
    fn its_rotation_turns_8_degrees_a_frame_before_a_bounce_is_checked() {
        let lvl = room();
        // against the left wall, heading still turning: the wall holds it
        let mut a = Actor::new(Kind::LavaBubble, 31.0 * TILE + 8.5, 100.0);
        a.wings = true;
        a.step_winged_bubble(&lvl);
        // 45° to turn at 8° a frame: 6 frames (5 whole steps and the rest)
        let mut frames = 1;
        while a.vx < 0.0 {
            a.step_winged_bubble(&lvl);
            frames += 1;
            assert!(frames < 20);
        }
        // held flush against the wall until the bounce, then off at once
        assert!(
            (a.x - (31.0 * TILE + 8.0 + 0.70710677)).abs() < 1e-3,
            "{}",
            a.x
        );
        assert_eq!(frames, 7);
        assert!(super::bubble_aligned(BUBBLE_START, BUBBLE_START));
    }
}
