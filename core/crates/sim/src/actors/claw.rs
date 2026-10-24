//! The Swinging Claw's rope (105-swinging-claw.md "Pendulum model"). The claw actor's own
//! position is the free end of the rope, a point mass under gravity held on a circle round the
//! pivot; the pivot sits 48 px above the placement [obs v3.0.3: constructor 0x71013e7280, init
//! 0x71013e7780 (one step at creation), exe 0x71013e7b10 (one step a frame), the step
//! 0x7101ca03f0; v1.0.0 0x71008bc2c0 / 0x71008bc7c0 / 0x71008bcc60 / 0x7100f0add0]:
//! - The rope is 6 links × 7 px: length L = 7 · (6 − 1) = 35 (the rope's link count and spacing
//!   vfuncs 0x7101c9f5f0 / 0x7101c9f600). Its ends are kept as the end 1 px down and the pivot
//!   2 px up (1.6 down at creation, 0x7101c9ff60); a step reads them as last left.
//! - A step: the direction n and the stretch t = clamp(|d| / L, 0, 1); a rope stretched past
//!   L + 4 last step loses 0.3 of the end's outward speed; gravity −0.1875 (floor −4, in a liquid
//!   −2); taut (t > 0.98), the rope's turn since the last step × 0.05 adds to a turn rate. Each
//!   time the end crosses below the pivot (or hangs straight) the turn rate is raised to at least
//!   0x2d82d8 (0.25°) its way; it then decays × 0.95 within ±5° and turns the direction: the
//!   speed gains t · (R(rate)·n − n), plus 0.1 · t · sin(turn) along the rope when taut. The
//!   speed is held within ±7, the end moves by it (half in a liquid), and an end past L + 4
//!   (measured 2 px up from the end) is pulled back onto that circle.
//!
//! So an empty claw hanging at rest starts to sway by itself and builds up slowly: the trace of
//! course W_3010660 (an empty claw from rest, ±0.79 / ±1.23 / ±1.54 / ±1.77 px at frames 33 / 89
//! / 143 / 197) is matched bit for bit over its 240 frames.
//!
//! The binary's held-object coupling (the end set to the held actor's own hold point, which its
//! Crane state moves) is replaced by holding the player at the end: the rope keeps its own
//! motion, the player's speed is taken into it at the catch, a held player's left / right nudge
//! it by ±0.05 a frame (0x71013e8bf0) and the rope's spacing follows the pivot distance while
//! holding (0x71013e7f30).

use super::{Actor, Kind};
use crate::fmath::{table_atan2, table_sin_cos};

/// The pivot above the placement [obs v3.0.3 0x71013e7280: + DAT 0x7102318b28].
pub const PIVOT_UP: f32 = 48.0;
/// On a track the pivot rides this much higher [obs v3.0.3 0x71013e7780: +0x5cc += 8].
const TRACK_UP: f32 = 8.0;
/// Link count and spacing [obs v3.0.3 vfuncs 0x7101c9f5f0 = 6, 0x7101c9f600 = +0xec, set 7.0].
const LINKS: i32 = 6;
const SPACING: f32 = 7.0;
/// Over-stretch allowance past L, and the outward speed lost the next step [obs v3.0.3
/// 0x7101ca03f0: 4.0, DAT 0x71023195ec].
const SLACK: f32 = 4.0;
const OUTWARD_LOSS: f32 = 0.3;
/// Gravity and its floors (in a liquid: −2).
const GRAVITY: f32 = -0.1875;
/// Taut from this stretch on (DAT 0x7102318d1c).
const TAUT: f32 = 0.98;
/// The rope's turn feeds the turn rate × 0.05 (DAT 0x7102318e84), the rate decays × 0.95 (DAT
/// 0x7102318c10) within ±5°, and is at least 0x2d82d8 when the end crosses below the pivot.
const TURN_GAIN: f32 = 0.05;
const TURN_DECAY: f32 = 0.95;
const TURN_MAX: i32 = 0x38e_38e3;
const TURN_MIN: i32 = 0x2d_82d8;
/// The push along the rope: 0.1 · t · sin(turn) (DAT 0x7102318b04).
const PUSH: f32 = 0.1;
/// Speed bound.
const VMAX: f32 = 7.0;
/// The catch's reach: the hold point under this from the pivot [obs v3.0.3 0x71013e87c0: DAT
/// 0x7102318978].
pub const REACH: f32 = 46.0;
/// A held player's left / right nudge (table 0x7102319c00) [obs v3.0.3 0x71013e8bf0].
pub const NUDGE: f32 = 0.05;
/// While holding: spacing = min(pivot distance, 48) / 5 from 24 px on, else 4.8 (DAT
/// 0x710231912c); then eased back toward 7 by 0.95 of the excess (at least 0.0475, DAT
/// 0x7102318880) [obs v3.0.3 0x71013e7f30].
const HOLD_NEAR: f32 = 24.0;
const HOLD_FAR: f32 = 48.0;
const HOLD_SHORT: f32 = 4.8;
const EASE_MIN: f32 = 0.0475;

/// The rope's state (the step's record at the claw's +0x5c0; v1.0.0 +0x5b8).
#[derive(Clone, Debug, PartialEq)]
pub struct ClawRope {
    /// The end's speed.
    pub vx: f32,
    pub vy: f32,
    /// The turn rate (binary angle).
    turn: i32,
    /// The rope direction at the last step ((0, 0) before the first).
    prev: (f32, f32),
    /// Stretched past L + 4 at the last step.
    stretched: bool,
    /// The rope's ends as the last step left them: the end side and the pivot side.
    end: (f32, f32),
    pivot: (f32, f32),
    /// The link spacing (7; follows the pivot distance while holding).
    spacing: f32,
}

impl ClawRope {
    fn new(x: f32, y: f32, px: f32, py: f32) -> Self {
        Self {
            vx: 0.0,
            vy: 0.0,
            turn: 0,
            prev: (0.0, 0.0),
            stretched: false,
            end: (x, y - 1.6),
            pivot: (px, py + 2.0),
            spacing: SPACING,
        }
    }

    fn length(&self) -> f32 {
        self.spacing * (LINKS - 1) as f32
    }

    fn unit(dx: f32, dy: f32) -> (f32, f32, f32) {
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.0 {
            let r = 1.0 / len;
            (dx * r, dy * r, len)
        } else {
            (dx, dy, len)
        }
    }

    /// One step of the rope (0x7101ca03f0 with its blend flag clear): moves the end `(x, y)`
    /// hanging from the pivot `(px, py)`; `liquid`: the claw is in water, lava or poison.
    fn step(&mut self, x: &mut f32, y: &mut f32, px: f32, py: f32, liquid: bool) {
        let (nx, ny, len) = Self::unit(self.end.0 - self.pivot.0, self.end.1 - self.pivot.1);
        let ratio = len / self.length();
        let t = if ratio < 0.0 { 0.0 } else { ratio.min(1.0) };
        let (mut vx, mut vy) = (self.vx, self.vy);
        if self.stretched {
            let m = (vx * vx + vy * vy).sqrt();
            if m > 0.0 {
                let r = 1.0 / m;
                let inward = r * vx * -nx - r * vy * ny;
                if inward < 0.0 {
                    let k = (m * inward * OUTWARD_LOSS).abs();
                    vx += k * -nx;
                    vy += k * -ny;
                }
            }
        }
        vy = (vy + GRAVITY).max(if liquid { -2.0 } else { -4.0 });
        let mut turned = 0i32;
        if t > TAUT && self.prev != (0.0, 0.0) {
            let now = table_atan2(ny, nx);
            let was = table_atan2(self.prev.1, self.prev.0);
            let mut d = now.wrapping_sub(was) as i32;
            if liquid {
                d = (d as f32 * 0.5) as i32;
            }
            turned = (d as f32 * TURN_GAIN) as i32;
            self.turn = self.turn.wrapping_add(turned);
        }
        // (the end crossed below the pivot, or hangs straight)
        if nx * self.prev.0 <= 0.0 && self.turn.unsigned_abs() < TURN_MIN as u32 {
            self.turn = if self.turn < 0 { -TURN_MIN } else { TURN_MIN };
        }
        self.turn = ((self.turn as f32 * TURN_DECAY) as i32).clamp(-TURN_MAX, TURN_MAX);
        if self.turn != 0 {
            let (s, c) = table_sin_cos(self.turn as u32);
            vx += t * ((nx * c - s * ny) - nx);
            vy += t * ((nx * s + ny * c) - ny);
            if t > TAUT {
                let push = t * table_sin_cos(turned as u32).0 * PUSH;
                vx += nx * push;
                vy += ny * push;
            }
        }
        vx = if vx < -VMAX { -VMAX } else { vx.min(VMAX) };
        vy = if vy < -VMAX { -VMAX } else { vy.min(VMAX) };
        self.vx = vx;
        self.vy = vy;
        let (sx, sy) = if liquid {
            (vx * 0.5, vy * 0.5)
        } else {
            (vx, vy)
        };
        *x += sx;
        *y += sy;
        // held on the circle of L + 4 round the pivot (measured 2 px up from the end)
        let (ux, uy, len) = Self::unit(*x - px, (*y - 2.0) - (py + 2.0));
        let over = len - (self.length() + SLACK);
        self.stretched = over > 0.0;
        if over > 0.0 {
            *x -= ux * over;
            *y -= uy * over;
        }
        self.prev = (nx, ny);
        self.end = (*x, *y - 1.0);
        self.pivot = (px, py + 2.0);
    }

    /// The spacing a held thing sets: min(pivot distance, 48) / 5 from 24 px on, else 4.8.
    fn spacing_at(at: (f32, f32), pivot: (f32, f32)) -> f32 {
        let (dx, dy) = (at.0 - pivot.0, at.1 - pivot.1);
        let d = (dx * dx + dy * dy).sqrt();
        if d >= HOLD_NEAR {
            d.min(HOLD_FAR) / 5.0
        } else {
            HOLD_SHORT
        }
    }

    /// While holding (0x71013e7f30): the spacing from the pivot distance, eased toward 7; the
    /// rope's ends as the end `at` and the pivot.
    pub fn hold(&mut self, at: (f32, f32), pivot: (f32, f32)) {
        let s = Self::spacing_at(at, pivot);
        let ease = if s - SPACING >= TURN_GAIN {
            (s - SPACING) * TURN_DECAY
        } else {
            EASE_MIN
        };
        self.spacing = if s > SPACING {
            let n = s - ease;
            if n <= SPACING || s < n { SPACING } else { n }
        } else if s < SPACING {
            let n = s + ease;
            if n >= SPACING || n < s { SPACING } else { n }
        } else {
            s
        };
        self.end = (at.0, at.1 - 1.0);
        self.pivot = (pivot.0, pivot.1 + 2.0);
    }

    /// The catch (0x71013e87c0) at the caught one's hold point `at` (where the end goes): the
    /// spacing from the pivot distance and the caught one's speed `v` taken into the rope's (x
    /// no lower than −7).
    pub fn catch(&mut self, at: (f32, f32), pivot: (f32, f32), v: (f32, f32)) {
        self.spacing = Self::spacing_at(at, pivot);
        self.vx = (v.0 + self.vx).max(-VMAX);
        self.vy += v.1;
    }
}

impl Actor {
    /// A claw's rope at its creation, with the init's one step (0x71013e7780).
    pub(crate) fn claw_start(&mut self) {
        if self.kind != Kind::Claw {
            return;
        }
        self.home_x = self.x;
        self.home_y = self.y + PIVOT_UP;
        self.claw = Some(ClawRope::new(self.x, self.y, self.home_x, self.home_y));
        if self.rail.is_some() {
            // on a track the pivot rides 8 px higher (set after the rope is made)
            self.home_y += TRACK_UP;
        }
        self.step_claw();
    }

    /// One frame of the claw (0x71013e7b10): the re-grab cooldown, then the rope's step.
    pub(super) fn step_claw(&mut self) {
        let liquid = self.w3.flooded || self.w3.liquid.is_some_and(|(y, _)| self.y < y);
        let (px, py) = (self.home_x, self.home_y);
        let (mut x, mut y) = (self.x, self.y);
        if let Some(rope) = self.claw.as_mut() {
            rope.step(&mut x, &mut y, px, py, liquid);
        }
        self.x = x;
        self.y = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claw() -> Actor {
        let mut a = Actor::new(Kind::Claw, 488.0, 112.0);
        a.claw_start();
        a
    }

    #[test]
    fn an_empty_claw_sways_up_from_rest_as_the_trace() {
        let mut a = claw();
        assert_eq!((a.home_x, a.home_y), (488.0, 160.0));
        // the game's object fields over the recorded frames (W_3010660; record 0 is the
        // creation step plus one), bit for bit
        let want = [
            (0, 0x43f4_0172, 0x42fa_0000),
            (33, 0x43f4_64e7, 0x42fa_0414),
            (89, 0x43f3_6270, 0x42fa_09f3),
            (143, 0x43f4_c4da, 0x42fa_0f88),
            (197, 0x43f3_1dff, 0x42fa_147a),
            (239, 0x43f4_c997, 0x42fa_104a),
        ];
        let mut k = 0;
        for f in 0..240 {
            a.step_claw();
            if want[k].0 == f {
                assert_eq!(
                    (a.x.to_bits(), a.y.to_bits()),
                    (want[k].1, want[k].2),
                    "frame {f}"
                );
                k += 1;
            }
        }
        assert_eq!(k, want.len());
    }

    #[test]
    fn a_held_player_nudges_the_rope_and_the_spacing_eases_back() {
        let mut r = ClawRope::new(0.0, 0.0, 0.0, 48.0);
        r.catch((0.0, 0.0), (0.0, 48.0), (1.0, 0.0));
        assert_eq!(r.spacing, 48.0 / 5.0);
        assert_eq!(r.vx, 1.0);
        // 40 px from the pivot: 8, eased to 7 + 0.05
        r.hold((0.0, 8.0), (0.0, 48.0));
        assert_eq!(r.spacing, 8.0 - (8.0 - SPACING) * TURN_DECAY);
        // at the rope's own length nothing to ease
        r.hold((0.0, 13.0), (0.0, 48.0));
        assert_eq!(r.spacing, SPACING);
        r.hold((0.0, 40.0), (0.0, 48.0));
        assert_eq!(r.spacing, HOLD_SHORT + EASE_MIN);
    }
}
