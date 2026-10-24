//! Boo (41-boo.md; the Teresa action, v3.0.3 vtable 0x71029108a0, states 0x710126c680).
//!
//! - **Aim** [obs v3.0.3: Follow exe 0x710126d970 and PataCharge exe 0x710126df50, the same
//!   block; v1.0.0 0x7100770da0 / 0x7100771310]: the Boo faces the player's side, then turns its
//!   heading at most 1° a frame toward the player's position 8 px up and clamps it to 45° about
//!   the horizontal on the side it faces (mirrored for a Boo facing left), so a player crossing
//!   under it swings the heading over to the other side at once. It starts pointing straight
//!   down (init 0x710126c9e0: +0x268 = 0xc0000000), i.e. 45° down on its first frame.
//! - **The winged Boo's charge** (PataCharge, enter 0x710126deb0 / exe 0x710126df50) [obs
//!   v3.0.3]: a winged Boo (+0x523 bit 1) starts in it, and comes back to it each time it stops
//!   being looked at (Stop exe 0x710126d0d0 → 4; an unwinged one goes on to Follow). It stops
//!   dead (velocity 0) and keeps aiming while its wings speed up: the wing helper
//!   (0x7101270eb0) eases its flap rate toward the rate of the body's animation — 2.0 by 0.04 a
//!   frame for the charge's (0x20), 0.25 by 0.1 for Stop's (0x35), 1.0 by 0.1 otherwise — and
//!   reports when it gets there. The charge then sets the enemy timer +0x53c to 20 (counted
//!   down after the action each frame, 0x710128acc0) and goes to Follow when it reads 1; Follow's
//!   enter (0x710126d8f0) gives a Boo coming from the charge a forward speed of 0.8, which its
//!   first frame clamps to the top speed 0.4: it sets off at full speed. Looked at during the
//!   charge it goes to Stop.
//!   [inf: the flap rate starts at 1.0; the helper runs after the action in a frame. 3D World
//!   plays its own charge animations (0x1e first time, else 0x20 then 0x1e, rate 1.0): the
//!   first charge here waits only the 20 frames, a later one is ramped as elsewhere, as the
//!   length of its 0x20 is not read]

use super::Actor;
use crate::fmath::DetMath;

/// Boo turn rate and heading clamp [obs v3.0.3 0x710126d970: 0xb60b60, ±0x20000000].
const TURN: f32 = 1.0;
const CLAMP: f32 = 45.0;
/// Forward speed: +0.005 a frame up to 0.4 (0x7102319510 / 0x7102319274); from the charge
/// Follow starts at 0.8 (0x710126d8f0).
const ACCEL: f32 = 0.005;
const TOP: f32 = 0.4;
const FROM_CHARGE: f32 = 0.8;
/// Wobble: amplitude +0.0015 a frame to 0.2 (0x7102319bf0 / 0x7102319f28), phase 2° a frame.
const WOBBLE_GROW: f32 = 0.0015;
const WOBBLE_MAX: f32 = 0.2;

/// Winged Boo states (`phase`): chasing (Follow), stopped (Stop) and charging (PataCharge).
pub(super) const BOO_FOLLOW: u8 = 0;
pub(super) const BOO_STOP: u8 = 1;
pub(super) const BOO_CHARGE: u8 = 2;
/// The charge's wait once its wings are up to speed (+0x53c = 0x14).
const CHARGE_WAIT: i32 = 20;

/// The wing helper's target flap rate and step for the body's animation [obs v3.0.3
/// 0x7101270eb0: anim 0x20 → 2.0 by 0.04 (0x7102319088), 0x35 → 0.25, else 1.0, by 0.1
/// (0x7102318b04)].
const RATE_CHARGE: (f32, f32) = (2.0, 0.04);
const RATE_STOP: (f32, f32) = (0.25, 0.1);
const RATE_FLY: (f32, f32) = (1.0, 0.1);

fn wrap(d: f32) -> f32 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

impl Actor {
    /// Boo: stops while the player faces it (speed decays by max(0.12·|v|, 0.002));
    /// otherwise aims at the player (`boo_aim`) and speeds up by 0.005 to 0.4, with a wobble
    /// growing 0.0015/frame to 0.2 (2°/frame phase). A winged one charges first (module
    /// docs). `w3`: the style is 3D World.
    pub(super) fn step_boo(&mut self, w3: bool) {
        let px = self.target.0;
        if self.age == 1 {
            // init: pointing straight down; a winged Boo starts in its charge
            self.heading = -90.0;
            if self.wings {
                self.boo_enter_charge();
                if w3 {
                    // 3D World's first charge animation asks for the normal rate [inf]
                    self.wing_rate = RATE_FLY.0;
                    self.phase = BOO_CHARGE + 1;
                }
            }
        }
        let looking = (px < self.x) == (self.target_facing > 0);
        if self.wings && self.phase >= BOO_CHARGE {
            self.step_boo_charge(looking);
            return;
        }
        if looking {
            self.speed = (self.speed - (0.12 * self.speed).max(0.002)).max(0.0);
            if self.wings {
                self.phase = BOO_STOP;
            }
        } else if self.wings && self.phase == BOO_STOP {
            // Stop → the charge (not Follow) for a winged one
            self.boo_enter_charge();
            self.boo_wings(RATE_CHARGE);
            return;
        } else {
            self.boo_aim();
            self.speed = (self.speed + ACCEL).min(TOP);
        }
        if self.wings {
            self.boo_wings(if looking { RATE_STOP } else { RATE_FLY });
        }
        // wobble only while chasing (its amplitude grows from the start of the chase)
        if looking {
            self.timer = 0;
        } else {
            self.timer += 1;
        }
        let amp = (self.timer as f32 * WOBBLE_GROW).min(WOBBLE_MAX);
        let w = amp * (self.timer as f32 * 2.0).to_radians().dsin();
        let (s, c) = self.heading.to_radians().dsin_cos();
        self.vx = self.speed * c - w * s;
        self.vy = self.speed * s + w * c;
        self.x += self.vx;
        self.y += self.vy;
    }

    /// The aim shared by Follow and the charge: face the player's side, turn ≤ 1° toward the
    /// player's position + 8 px up, then clamp to ±45° about the horizontal on the facing side.
    fn boo_aim(&mut self) {
        let (px, py) = self.target;
        self.dir = if px < self.x { -1 } else { 1 };
        let want = (py + 8.0 - self.y).datan2(px - self.x).to_degrees();
        let h = self.heading + wrap(want - self.heading).clamp(-TURN, TURN);
        self.heading = if self.dir < 0 {
            wrap(-180.0 - wrap(-180.0 - h).clamp(-CLAMP, CLAMP))
        } else {
            wrap(h).clamp(-CLAMP, CLAMP)
        };
    }

    /// PataCharge's enter: velocity 0, the timer cleared.
    fn boo_enter_charge(&mut self) {
        self.phase = BOO_CHARGE;
        self.vx = 0.0;
        self.vy = 0.0;
        self.pause = 0;
    }

    /// One frame of the charge (`phase` BOO_CHARGE, or BOO_CHARGE + 1 for 3D World's first,
    /// whose animation asks for rate 1.0): still, aiming, until 20 frames after the wings
    /// are up to speed.
    fn step_boo_charge(&mut self, looking: bool) {
        let rate = if self.phase > BOO_CHARGE {
            RATE_FLY
        } else {
            RATE_CHARGE
        };
        // the action (the wings' report of the frame before)
        let ready = self.wing_rate == rate.0;
        if looking {
            self.phase = BOO_STOP;
            self.timer = 0;
        } else {
            self.boo_aim();
            if self.pause == 0 {
                if ready {
                    self.pause = CHARGE_WAIT;
                }
            } else if self.pause == 1 {
                // Follow, from the charge: 0.8, clamped to the top speed on its first frame
                self.phase = BOO_FOLLOW;
                self.speed = FROM_CHARGE;
            }
        }
        self.vx = 0.0;
        self.vy = 0.0;
        self.boo_wings(if looking {
            RATE_STOP
        } else if self.phase == BOO_FOLLOW {
            RATE_FLY
        } else {
            rate
        });
        if self.pause > 0 {
            self.pause -= 1;
        }
    }

    /// The wing helper's frame: the flap rate eased toward `target` by `step` (snapping on
    /// it when the step would reach or pass it).
    fn boo_wings(&mut self, (target, step): (f32, f32)) {
        let r = self.wing_rate;
        if r < target {
            let n = r + step;
            self.wing_rate = if target <= n { target } else { n };
        } else if r > target {
            let n = r - step;
            self.wing_rate = if n <= target { target } else { n };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::actors::{Actor, Kind};

    fn boo(wings: bool) -> Actor {
        let mut a = Actor::new(Kind::Boo, 100.0, 100.0);
        a.wings = wings;
        a.dir = -1;
        // the player to the left, facing away (left)
        a.target = (20.0, 100.0);
        a.target_facing = -1;
        a
    }

    fn run(a: &mut Actor, n: usize) {
        for _ in 0..n {
            a.age += 1;
            a.step_boo(false);
        }
    }

    #[test]
    fn a_boo_starts_45_degrees_down_on_its_side_and_turns_1_degree_a_frame() {
        let mut a = boo(false);
        run(&mut a, 1);
        assert!((a.heading - -135.0).abs() < 1e-4, "{}", a.heading);
        run(&mut a, 10);
        assert!((a.heading - -145.0).abs() < 1e-3, "{}", a.heading);
    }

    #[test]
    fn a_player_crossing_under_it_swings_the_heading_over_at_once() {
        let mut a = boo(false);
        run(&mut a, 60);
        assert!(a.heading.abs() > 135.0, "{}", a.heading);
        a.target = (180.0, 100.0);
        a.target_facing = 1; // still facing away
        run(&mut a, 1);
        assert!(
            a.dir == 1 && a.heading.abs() <= 45.0 + 1e-4,
            "{}",
            a.heading
        );
    }

    #[test]
    fn a_winged_boo_waits_for_its_wings_then_sets_off_at_full_speed() {
        let mut a = boo(true);
        // 26 frames of the flap rate easing 1.0 → 2.0 by 0.04, the report seen a frame
        // later, then the 20-frame timer read at 1 after 19 counts
        let mut still = 0;
        for _ in 0..200 {
            a.age += 1;
            let x = a.x;
            a.step_boo(false);
            if a.x != x {
                break;
            }
            still += 1;
        }
        assert_eq!(still, 26 + 1 + 19);
        assert!((a.speed - 0.4).abs() < 1e-6, "{}", a.speed);
    }

    #[test]
    fn looked_at_it_stops_and_charges_again_when_the_player_turns_away() {
        let mut a = boo(true);
        run(&mut a, 120);
        assert_eq!(a.phase, super::BOO_FOLLOW);
        a.target_facing = 1; // the player turns to face it
        run(&mut a, 60);
        assert_eq!(a.phase, super::BOO_STOP);
        assert!(a.wing_rate < 0.26, "{}", a.wing_rate);
        a.target_facing = -1;
        run(&mut a, 1);
        assert_eq!(a.phase, super::BOO_CHARGE);
        let x = a.x;
        run(&mut a, 30);
        assert_eq!(a.x, x, "still while charging");
        // from 0.25 the wings need 44 frames to reach 2.0
        run(&mut a, 40);
        assert_eq!(a.phase, super::BOO_FOLLOW);
    }

    #[test]
    fn an_unwinged_boo_never_charges() {
        let mut a = boo(false);
        let x = a.x;
        run(&mut a, 2);
        assert!(a.x < x);
        assert_eq!(a.phase, 0);
    }
}
