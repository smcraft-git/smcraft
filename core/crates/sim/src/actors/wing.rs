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

use super::{Actor, TURN_LOCK};
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

impl Actor {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Kind;
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
}
