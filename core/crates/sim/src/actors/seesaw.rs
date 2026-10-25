//! The Seesaw's plank (91-seesaw.md "Observed in v3.0.3"), as the game turns it [obs v3.0.3:
//! the per-frame physics 0x71013d7d90 and its angle integrator 0x71013d9950 (v1.0.0
//! 0x71008ad750 / 0x71008af1a0); the plank, its torque, angle and end points matched to a Ryujinx
//! trace of course W_3010660 with the Seesaw watched, bit for bit or within f32 rounding]:
//! - The tilt θ is a 32-bit binary angle (positive: the right end up) and the tip speed ω a
//!   float; ω > 0 turns the plank clockwise.
//! - A rider at distance d from the pivot (d ≥ 8) weighs w (the player 2) and pushes with
//!   τ = d · sign · w · cos(asin(8 / d) − |θ|), the sign + when it is right of the point 8 px
//!   under the top over the pivot (x ≥ pivot − 8 sin θ); the angles are the game's binary ones
//!   (its arctangent helper, its 256-step sine table). Only |τ| ≥ 15 counts.
//! - Δω = τ / (W² · 6 · 64), at least ±0.0015; with Δω beyond ±0.0001 ω gains it (held within
//!   ±0.4), else ω × 0.95. The angle then turns by the chord: cos dθ = 1 − ω² / 128, through
//!   sqrt / the arctangent helper, clockwise for ω > 0; past ±45° it stops there and ω bounces
//!   back × (W + 3) / 36.
//! - Unpushed (Δω within ±0.0001) and after the turn, ω moves the plank back toward level by
//!   0.0025 · min(|θ|, 25°) / 25° (at least 0.001); with |ω| < 0.00075 and |θ| ≤ 0x16c16b both
//!   stop at 0.
//! - The plank's ends (relative to the pivot, 8 px under the top) are the table's rotation of
//!   (±8W, 8), less 8 up: x = c·X − s·8, y = (s·X + c·8) − 8.
//!
//! What a rider sees of it (world.rs `step_seesaws`, player.rs): it stands on the top line, the
//! ends' line 8 px up, as the plank lay last frame; the plank's turn carries it by the ends'
//! movement taken at its place along them; and it walks along the plank at x · cos of the angle
//! the plank had two frames before (the player's ground angle, read from the floor contact a
//! frame late).

use crate::fmath::{table_atan, table_atan2, table_sin_cos};

/// Half the plank's thickness: the top line is 8 px over the line through the pivot.
pub const HALF: f32 = 8.0;
/// A ground pound's push: ω = ±7.2 / (W + 3), turned with bounce 0.25 until the stop [obs
/// v3.0.3 0x71013d7d90: DAT_71023192c4 / DAT_7102318dd0, the integrator called with 0x3e800000;
/// Ryujinx trace X_seesawp: ω 0.9 on a 5-tile plank the frame the pound lands, the 45° stop six
/// frames later, back at −0.225].
const SLAM: f32 = 7.2;
/// The plank's turn limit, ±45°.
const LIMIT: i32 = 0x2000_0000;
/// The tip speed's cap [obs v3.0.3: Seesaw +0x8e0 = 0.4 in the trace].
const MAX_OMEGA: f32 = 0.4;

/// The plank's state: angle, tip speed and its end points this frame and last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plank {
    pub theta: i32,
    pub omega: f32,
    /// (left x, left y, right x, right y) relative to the pivot.
    pub ends: [f32; 4],
    pub prev: [f32; 4],
    /// Turning from a ground pound (+0xc19): no torque, damping or return until the stop.
    pub slammed: bool,
}

impl Plank {
    pub fn new(width: u8) -> Self {
        let ends = ends(width, 0);
        Plank {
            theta: 0,
            omega: 0.0,
            ends,
            prev: ends,
            slammed: false,
        }
    }

    /// A rider's ground pound lands on it, right of the point 8 px under the top over the
    /// pivot or left of it: the plank takes the slam's tip speed (not against a stop it rests
    /// on that way).
    pub fn slam(&mut self, width: u8, right: bool) {
        let open = if right {
            self.theta > -LIMIT
        } else {
            self.theta < LIMIT
        };
        if open {
            self.omega = if right { SLAM } else { -SLAM } / (width as f32 + 3.0);
            self.slammed = true;
        }
    }

    /// The top line's slope (dy / dx).
    pub fn slope(&self) -> f32 {
        let e = self.ends;
        (e[3] - e[1]) / (e[2] - e[0])
    }

    /// The angle of last frame's plank: the ground angle a rider walks with next frame.
    pub fn prev_angle(&self) -> u32 {
        let p = self.prev;
        table_atan2(p[3] - p[1], p[2] - p[0])
    }

    /// How far a foot sensor `half` px either side of a walker's centre reaches past the ends
    /// of the plank's top segment, along it (dx, dy; dx ≤ 0 for a plank within ±45°) [obs
    /// v3.0.3 0x7100e32dd0, the floor test's open-end stretch (from 0x7100e2ccf0 in the
    /// collider's point test 0x7100e2b3c0): an end with no neighbour segment moves out by the
    /// sensor's half width × cos 2u along the segment's angle u; Ryujinx traces X_seesaw2 /
    /// X_seesaw3: a Goomba (±4) held 1.35 px past an end at 31.7° and let go 0.48 past at 43.3°].
    pub fn end_reach(&self, half: f32) -> (f32, f32) {
        let e = self.ends;
        let (dx, dy) = (e[2] - e[0], e[3] - e[1]);
        // (the collider's angle: axis-aligned within 1e-5)
        let u = if !(-1e-5..=1e-5).contains(&dx) {
            if !(-1e-5..=1e-5).contains(&dy) {
                table_atan2(dy, dx) as i32
            } else if dx < 0.0 {
                i32::MIN
            } else {
                0
            }
        } else if dy < 0.0 {
            -0x4000_0000
        } else {
            0x4000_0000
        };
        if u.unsigned_abs() > LIMIT as u32 {
            return (0.0, 0.0);
        }
        let h = (-half - half) * 0.5;
        if u == 0 {
            return (h, 0.0);
        }
        let (s, c) = table_sin_cos(u as u32);
        let (_, c2) = table_sin_cos(((u as f32) + (u as f32)) as i32 as u32);
        ((c * h - s * 0.0) * c2, (s * h + c * 0.0) * c2)
    }

    /// Where the plank's turn this frame takes a walker standing at (x, y), the plank's ends
    /// being relative to (ox, oy): its point of last frame's top segment (its place along it,
    /// held to the segment) goes where the ends take it, the walker's offset from that point
    /// turns with the segment's angle [obs: Ryujinx traces X_seesaw2 / X_seesaw3, the Goombas'
    /// carry +0x674: over the plank the lerp of the ends' movement, past an end that end's
    /// movement plus the turn about it, within 1e-5, where the extended lerp is 8e-5 off] [inf:
    /// the turn's exact form; the game's sine table is too coarse for it].
    pub fn carry_walker(&self, ox: f32, oy: f32, x: f32, y: f32) -> (f32, f32) {
        let (p, e) = (self.prev, self.ends);
        let t = ((x - ox - p[0]) / (p[2] - p[0])).clamp(0.0, 1.0);
        let at = |q: [f32; 4]| (q[0] + t * (q[2] - q[0]), q[1] + t * (q[3] - q[1]));
        let ((ax, ay), (bx, by)) = (at(p), at(e));
        // (the turn from the two segments' directions: the sine table's 2° is 7e-5 off)
        let (d0, d1) = ((p[2] - p[0], p[3] - p[1]), (e[2] - e[0], e[3] - e[1]));
        let n = (d0.0 * d0.0 + d0.1 * d0.1).sqrt() * (d1.0 * d1.0 + d1.1 * d1.1).sqrt();
        let (c, s) = (
            (d0.0 * d1.0 + d0.1 * d1.1) / n,
            (d0.0 * d1.1 - d0.1 * d1.0) / n,
        );
        let (vx, vy) = (x - (ox + ax), y - (oy + ay));
        ((ox + bx) + (c * vx - s * vy), (oy + by) + (s * vx + c * vy))
    }

    /// How the plank's turn this frame moves a rider at x (relative to the pivot): the ends'
    /// movement taken at its place along last frame's ends.
    pub fn carry(&self, x: f32) -> (f32, f32) {
        let (p, e) = (self.prev, self.ends);
        let t = (x - p[0]) / (p[2] - p[0]);
        (
            (e[0] - p[0]) + t * ((e[2] - e[0]) - (p[2] - p[0])),
            (e[1] - p[1]) + t * ((e[3] - e[1]) - (p[3] - p[1])),
        )
    }

    /// One frame with the riders' summed torque `tau` (see the module notes).
    pub fn step(&mut self, width: u8, tau: f32) {
        let w = width as f32;
        if self.slammed {
            self.turn(0.25);
            self.prev = self.ends;
            self.ends = ends(width, self.theta);
            return;
        }
        let tau = if tau.abs() >= 15.0 { tau } else { 0.0 };
        let mut dw = 0.0;
        if !(-1e-5..=1e-5).contains(&tau) {
            dw = tau / (w * w * 6.0 * 64.0);
            if dw.abs() < 0.0015 {
                dw = if dw >= 0.0 { 0.0015 } else { -0.0015 };
            }
        }
        let pushed = !(-1e-4..=1e-4).contains(&dw);
        self.omega = if pushed {
            (self.omega + dw).clamp(-MAX_OMEGA, MAX_OMEGA)
        } else {
            self.omega * 0.95
        };
        self.turn((w + 3.0) * 0.5 / 18.0);
        if !pushed {
            let th = self.theta.unsigned_abs();
            if self.omega.abs() >= 0.000_75 || th > 0x16_c16b {
                // (the game's 25° is 298261632 binary units, as a float)
                let mut back = (th as f32).min(298_261_632.0) / 298_261_632.0 * 0.0025;
                back = back.max(0.001);
                if self.theta < 1 {
                    self.omega -= back;
                } else {
                    self.omega += back;
                }
            } else {
                self.omega = 0.0;
                self.theta = 0;
            }
        }
        self.prev = self.ends;
        self.ends = ends(width, self.theta);
    }

    /// The angle integrator: the chord ω on a 16 px circle, then the ±45° stop with bounce `e`.
    fn turn(&mut self, e: f32) {
        let c = self.omega * self.omega * (-1.0 / 128.0) + 1.0;
        let d = if c >= 0.0 {
            let s = (1.0 - c * c).sqrt();
            if c <= std::f32::consts::FRAC_1_SQRT_2 {
                0x4000_0000 - table_atan(c / s)
            } else {
                table_atan(s / c)
            }
        } else {
            // (|ω| > 16: never reached with ω held within ±0.4)
            0x4000_0000
        };
        let before = self.theta;
        let mut th = if self.omega <= 0.0 {
            before.wrapping_add(d)
        } else {
            before.wrapping_sub(d)
        };
        if !(-LIMIT..=LIMIT).contains(&th) {
            let limit = if th > 0 { LIMIT } else { -LIMIT };
            if before != limit {
                self.omega = -(self.omega * e);
            }
            th = limit;
            self.slammed = false;
        }
        // (resting on a limit, pushed into it: the tip speed stops)
        if th == before && (th == LIMIT && self.omega < 0.0 || th == -LIMIT && self.omega > 0.0) {
            self.omega = 0.0;
        }
        self.theta = th;
    }
}

/// A rider's torque at (rx, ry) about the pivot (px, py) with weight `w`, the plank at `theta`.
pub fn torque(theta: i32, px: f32, py: f32, rx: f32, ry: f32, w: f32) -> f32 {
    let (dx, dy) = (px - rx, py - ry);
    let d = (dx * dx + dy * dy).sqrt().max(8.0);
    let s = 8.0 / d;
    let c = (1.0 - s * s).sqrt();
    let phi = if s <= std::f32::consts::FRAC_1_SQRT_2 {
        table_atan(s / c)
    } else {
        0x4000_0000 - table_atan(c / s)
    };
    let (sin, _) = table_sin_cos(theta as u32);
    let sign = if px + sin * -8.0 <= rx { 1.0 } else { -1.0 };
    let (_, cos) = table_sin_cos(phi.wrapping_sub(theta.wrapping_abs()) as u32);
    d * sign * w * cos
}

/// The plank's end points at `theta` (see the module notes).
fn ends(width: u8, theta: i32) -> [f32; 4] {
    let (s, c) = table_sin_cos(theta as u32);
    let x = width as f32 * 8.0;
    let end = |x: f32| (c * x - s * HALF, (s * x + c * HALF) - HALF);
    let (l, r) = (end(-x), end(x));
    [l.0, l.1, r.0, r.1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rider_on_the_right_turns_it_clockwise_to_the_stop() {
        let mut p = Plank::new(5);
        p.step(5, 0.0);
        assert_eq!((p.theta, p.omega), (0, 0.0), "unpushed and level: it stays");
        let mut bounced = false;
        for _ in 0..400 {
            let tau = torque(p.theta, 0.0, 0.0, 30.0, 8.0, 2.0);
            assert!(tau > 0.0);
            let before = p.omega;
            p.step(5, tau);
            bounced |= before > 0.0 && p.omega < 0.0;
            assert!(p.theta <= 0 && p.theta >= -LIMIT);
        }
        assert_eq!(p.theta, -LIMIT);
        assert!(bounced, "it bounces off the 45° stop");
        // the right end is down, the top line falls to the right
        assert!(p.ends[3] < p.ends[1] && p.slope() < 0.0);
    }

    #[test]
    fn a_foot_reaches_past_the_ends_less_as_the_plank_turns() {
        let mut p = Plank::new(5);
        assert_eq!(
            p.end_reach(4.0),
            (-4.0, 0.0),
            "level: the sensor's half width"
        );
        let mut last = 4.0f32;
        // (up to about 43.6°, short of the stop)
        while p.theta > -0x1f00_0000 {
            p.step(5, 300.0);
            let (dx, _) = p.end_reach(4.0);
            assert!(dx <= 0.0 && -dx <= last + 1e-6, "{dx} after {last}");
            last = -dx;
        }
        // (4 · cos 2u · cos u: 1.53 at 31.7°, 0.16 at 43.5°)
        assert!(last < 0.5);
    }

    #[test]
    fn the_carry_follows_the_ends() {
        let mut p = Plank::new(5);
        p.step(5, 200.0);
        let (lx, ly) = p.carry(p.prev[0]);
        assert_eq!((lx, ly), (p.ends[0] - p.prev[0], p.ends[1] - p.prev[1]));
        let (rx, ry) = p.carry(p.prev[2]);
        assert!((rx - (p.ends[2] - p.prev[2])).abs() < 1e-5);
        assert!((ry - (p.ends[3] - p.prev[3])).abs() < 1e-5);
    }
}
