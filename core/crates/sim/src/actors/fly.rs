//! The shared winged flight (the fly helper behind the Wing context of items, coins and map
//! objects, and behind a winged block's Wing move state) [obs v3.0.3: helper init 0x71013d0e50,
//! states Idle / FlyMove (exe 0x71013d10a0) / FlyShuttleMove (exe 0x71013d1270) /
//! FlyJumpdaiMove (enter 0x71013d1460), update 0x71013d1670; v1.0.0 0x71008a7b30 …
//! 0x71008a8350]:
//! - **FlyMove**: the base point moves 0.625 px/f the way the part faces, and the part sits
//!   16·sin θ above it, θ stepping 0x2000000 (2.8125°, one bob every 128 frames) from 0 [obs:
//!   the parameters item Move enter 0x7101342b20 (speed 0x7101342fc0, angle 0x7101342f10) and the
//!   block's Wing enter 0x710138bd30 write].
//! - **FlyShuttleMove** (winged Hard, Donut and Ice blocks: block kinds 6, 0x40, 0x78 and the
//!   unmapped 0x5a / 0x110): no drift; the part swings ±48 px sideways round its base too, that
//!   angle stepping −0x800000 (−0.703°, one swing every 512 frames), and faces the way it moves.
//! - A base point knocked off its height eases back 0.1 px a frame, snapping within 0.2 (the
//!   shuttle on both axes).
//! - In water (the part's BG flag bit 31) the drift, both angle steps and the tilt step halve.
//! - Terrain: only in 3D World does the helper read the part's BG checker (the update's style
//!   test): a wall ahead turns it (the shuttle's sideways angle restarts the other way), a floor
//!   met moving down restarts the bob from its bottom, a ceiling met moving up from its top, the
//!   base moved to where the terrain put it. The items and coins have no BG check while winged
//!   in any style (EnemyDB bgcheck `InvalidPataAllSkin`), so they fly through everything; the map
//!   objects (`InvalidPata`) and the blocks meet terrain in 3D World only [inf: a block's BG
//!   checker (+0x420 v1) taken as present there]. In the 2D styles everything winged passes
//!   through terrain.
//!
//! The drawn tilt (±11.25°, stepping 0.703° a frame) is not modelled.

use super::{Actor, Kind};
use crate::blocks::BlockKind;
use crate::fmath::table_sin_cos;
use crate::level::Level;

/// Drift speed of FlyMove [obs v3.0.3 0x7101342fc0 (items, map objects), 0x710138bd30 (blocks)].
pub const FLY_SPEED: f32 = 0.625;
/// Vertical bob amplitude and angle step (2.8125°/frame).
pub const FLY_AMP: f32 = 16.0;
const FLY_STEP: i32 = 0x200_0000;
/// FlyShuttleMove's sideways swing: amplitude and angle step (−0.703°/frame).
pub const SHUTTLE_AMP: f32 = 48.0;
const SHUTTLE_STEP: i32 = -0x80_0000;
/// A displaced base eases back this much a frame and snaps within `SNAP` [obs v3.0.3
/// 0x7102319eb0 ±0.1, 0x7102319384 0.2, deadband 0x7102318cf4 1e-5].
const RETURN: f32 = 0.1;
const SNAP: f32 = 0.2;
const DEADBAND: f32 = 1e-5;

/// The fly helper's state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fly {
    /// Set up (the Wing context's Move enter has run).
    pub on: bool,
    pub shuttle: bool,
    /// The base point the part bobs round, and the point it eases back to.
    pub base: (f32, f32),
    pub home: (f32, f32),
    /// Sideways (shuttle) and vertical angles (32-bit binary angles).
    pub angle: (u32, u32),
}

impl Fly {
    /// The start: both angles 0, the part at its base.
    fn start(x: f32, y: f32, shuttle: bool) -> Self {
        Fly {
            on: true,
            shuttle,
            base: (x, y),
            home: (x, y),
            angle: (0, 0),
        }
    }
}

/// A step halved in water, truncated as the game's int conversion does.
fn half(step: i32, slow: bool) -> i32 {
    if slow {
        (step as f32 * 0.5) as i32
    } else {
        step
    }
}

/// Eases `v` back toward `home` (the helper's return).
fn ease(v: f32, home: f32) -> f32 {
    if (v - home).abs() <= DEADBAND {
        return v;
    }
    let n = v + if v < home { RETURN } else { -RETURN };
    if (home - n).abs() <= SNAP { home } else { n }
}

impl Kind {
    /// Parts whose wings fly them with the shared fly helper: the items (ItemDefault Wing), the
    /// coins (Coin Wing), the map objects with wings (MapObjDefault Wing: spring, P switch, POW)
    /// and winged blocks.
    pub fn flies_winged(self) -> bool {
        self.is_item()
            || matches!(
                self,
                Kind::FloatCoin
                    | Kind::BigCoin { .. }
                    | Kind::Spring
                    | Kind::SideSpring
                    | Kind::PSwitch
                    | Kind::Pow
                    | Kind::MovingBlock { .. }
            )
    }

    /// Meets terrain while flying, in 3D World only (bgcheck `InvalidPata`; blocks [inf]).
    fn fly_meets_terrain(self) -> bool {
        matches!(
            self,
            Kind::Spring | Kind::SideSpring | Kind::PSwitch | Kind::Pow | Kind::MovingBlock { .. }
        )
    }

    /// FlyShuttleMove's blocks [obs v3.0.3 0x710138bd30: kinds 6, 0x40, 0x78 (and 0x5a, 0x110)].
    fn fly_shuttles(self) -> bool {
        matches!(
            self,
            Kind::MovingBlock {
                block: BlockKind::Hard | BlockKind::Donut | BlockKind::Ice,
                ..
            }
        )
    }
}

impl Actor {
    /// One frame of the winged flight. `slow`: in water.
    pub(super) fn step_fly(&mut self, level: &Level, slow: bool) {
        if !self.fly.on {
            // (an item out of a Lakitu's throw, origin 10, starts the bob from its top instead
            // [obs v3.0.3 0x7101342f10]; the engine's thrown parts land first)
            self.fly = Fly::start(self.x, self.y, self.kind.fly_shuttles());
        }
        let (x0, y0) = (self.x, self.y);
        let f = &mut self.fly;
        if f.shuttle {
            f.angle.0 = f.angle.0.wrapping_add(half(SHUTTLE_STEP, slow) as u32);
            f.angle.1 = f.angle.1.wrapping_add(half(FLY_STEP, slow) as u32);
            self.x = SHUTTLE_AMP * table_sin_cos(f.angle.0).0 + f.base.0;
            self.y = FLY_AMP * table_sin_cos(f.angle.1).0 + f.base.1;
            f.base = (ease(f.base.0, f.home.0), ease(f.base.1, f.home.1));
            self.dir = if self.x <= x0 { -1 } else { 1 };
        } else {
            let v = FLY_SPEED * self.dir as f32;
            f.base.0 += if slow { v * 0.5 } else { v };
            f.angle.1 = f.angle.1.wrapping_add(half(FLY_STEP, slow) as u32);
            self.x = f.base.0;
            self.y = FLY_AMP * table_sin_cos(f.angle.1).0 + f.base.1;
            f.base.1 = ease(f.base.1, f.home.1);
        }
        self.vx = self.x - x0;
        self.vy = self.y - y0;
        if level.style == "3W" && self.kind.fly_meets_terrain() {
            self.fly_terrain(level, x0, y0);
        }
    }

    /// 3D World: the move redone through the terrain, and the helper's reactions to what it met.
    fn fly_terrain(&mut self, level: &Level, x0: f32, y0: f32) {
        let (tx, ty) = (self.x, self.y);
        let (dx, dy) = (tx - x0, ty - y0);
        (self.x, self.y, self.vx, self.vy) = (x0, y0, dx, dy);
        self.wall = false;
        self.collide(level);
        if self.wall {
            if self.fly.shuttle {
                self.fly.angle.0 = if self.dir < 0 { 0x8000_0000 } else { 0 };
            }
            self.dir = -self.dir;
            self.fly.base.0 = self.x;
        }
        if dy < 0.0 && self.y > ty + 1e-4 {
            // a floor: the bob restarts from its bottom
            self.fly.angle.1 = 0;
            self.fly.base.1 = self.y;
        } else if dy > 0.0 && self.y < ty - 1e-4 {
            // a ceiling: from its top
            self.fly.angle.1 = 0x8000_0000;
            self.fly.base.1 = self.y;
        }
        self.vy = self.y - y0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;

    fn level(style: &str) -> Level {
        let mut l = Level::flat(40);
        l.style = style.to_string();
        l
    }

    #[test]
    fn a_winged_mushroom_drifts_its_way_bobbing_16_px_every_128_frames() {
        let lvl = level("M1");
        let mut a = Actor::new(Kind::Mushroom, 100.0, 64.0);
        a.wings = true;
        let mut top: f32 = 0.0;
        for t in 1..=128 {
            a.step_fly(&lvl, false);
            top = top.max(a.y);
            assert!((a.x - (100.0 + 0.625 * t as f32)).abs() < 1e-3);
            if t == 32 {
                // a quarter of the bob: 16 px up
                assert!((a.y - 80.0).abs() < 1e-4, "{}", a.y);
            }
        }
        assert!((top - 80.0).abs() < 1e-4);
        assert!(
            (a.y - 64.0).abs() < 1e-4,
            "back at the base after a whole bob"
        );
    }

    #[test]
    fn in_water_the_flight_runs_at_half_speed() {
        let lvl = level("M1");
        let mut a = Actor::new(Kind::Star, 100.0, 64.0);
        a.dir = -1;
        for _ in 0..64 {
            a.step_fly(&lvl, true);
        }
        assert!((a.x - (100.0 - 0.3125 * 64.0)).abs() < 1e-3);
        // 64 frames at 1.40625°: a quarter bob
        assert!((a.y - 80.0).abs() < 1e-4, "{}", a.y);
    }

    #[test]
    fn a_winged_hard_block_shuttles_48_px_sideways_and_faces_its_way() {
        let lvl = level("M1");
        let mut a = Actor::new(
            Kind::MovingBlock {
                block: BlockKind::Hard,
                contents: None,
            },
            200.0,
            64.0,
        );
        let mut lo = f32::MAX;
        for t in 1..=512 {
            a.step_fly(&lvl, false);
            lo = lo.min(a.x);
            if t == 10 {
                assert_eq!(
                    a.dir, -1,
                    "the sideways angle steps down: it heads left first"
                );
            }
        }
        assert!((lo - 152.0).abs() < 1e-3, "{lo}");
        assert!((a.x - 200.0).abs() < 1e-3, "one swing in 512 frames");
    }

    #[test]
    fn the_flight_passes_through_terrain_except_a_3d_world_map_object() {
        // a wall 3 tiles right of the start
        for (style, kind, through) in [
            ("M1", Kind::PSwitch, true),
            ("3W", Kind::Mushroom, true),
            ("3W", Kind::PSwitch, false),
        ] {
            let mut lvl = level(style);
            for ty in 0..12 {
                lvl.set(9, ty, crate::level::Cell::Solid);
            }
            let mut a = Actor::new(kind, 120.0, 64.0);
            for _ in 0..120 {
                a.step_fly(&lvl, false);
            }
            assert_eq!(a.x > 144.0, through, "{style} {kind:?}: x {}", a.x);
            if !through {
                assert_eq!(a.dir, -1, "turned at the wall");
            }
        }
    }

    #[test]
    fn a_displaced_base_eases_back_a_tenth_a_frame() {
        assert_eq!(ease(10.0, 10.0), 10.0);
        assert!((ease(9.0, 10.0) - 9.1).abs() < 1e-6);
        assert_eq!(ease(9.85, 10.0), 10.0);
        assert!((ease(11.0, 10.0) - 10.9).abs() < 1e-6);
    }
}
