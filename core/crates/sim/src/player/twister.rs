//! A Twister's column carrying the player (76-twister.md). There is no player state for it: the
//! player update runs a twister block each frame the Twister's contact has filled the player's
//! receiver, before the movement integrates [obs v3.0.3: the update 0x710158a590 (after the
//! timers' update 0x710158d3f0), the receiver at player +0x29a8 made in the constructor
//! 0x7101587d10, the contact 0x71013ec330, the band gravity 0x71015d25f0, the integration
//! 0x71015d3cc0; v1.0.0 0x71009bc2c0 / 0x71008c0630]:
//! - **Rising or not**: a grounded player is lifted off (the ground flag cleared, a downward
//!   speed zeroed) and rises unless within 2 px under the column's top; in the air the player
//!   rises unless falling within 16 px under the top. Rising, the band gravity is turned upward;
//!   falling near the top it is the normal one, so the player bobs there.
//! - **Entering** (not yet in, and falling or 16 px or more under the top): the 30-frame steer
//!   timer starts if rising. While it runs the air control is replaced: the speed eases toward
//!   the Twister's own movement by 0.075 a frame and the player is pulled toward its centre by
//!   twice that movement.
//! - **Gravity**: the released-jump bands (the block clears the input's jump-held flag first).
//! - **Jump**: a press in the column marks the player; marked, a rising player gains 1.0 a frame.
//! - **Rise caps**: 3.0, marked 5.0; at night in the Sky theme 2.5, marked 3.5 (the low-gravity
//!   integration still scales the gravity by 0.2).
//! - **In water** (the flag word +0x230c bit 0, the swimming flag: the Crane exit and the
//!   state picker 0x71015c75a0 read it so too): the released bands at half strength (table
//!   0x71023d5488 × 0.5, turned upward the same way) in place of the swim's sinking, and caps
//!   1.5, marked 3.0 (Sky night as above).
//!
//! Not ported [inf]: the +0x494 bit 1 path (caps 2.0, in water 1.5), the entry's reset of the
//! jump-state flags (0x710158ed30), and the `Tornado_Up` / `Tornado_Down` sounds; in water,
//! whether the swim's own stroke and its 1.8 cap still apply in a column (the stroke is kept,
//! the cap left to the column's).

use super::{GRAVITY_RELEASED, Player, approach, vy_band};

/// The steer timer started on entering while rising (+0x2888 = 30), counted down each frame
/// before the block [obs v3.0.3 0x710158d3f0].
const STEER_FRAMES: i32 = 30;
/// The steer's ease toward the Twister's movement (DAT 0x7102318738 / 0x71023194c8).
const STEER_EASE: f32 = 0.075;
/// Under the top: within 16 px a falling player falls normally (and an unentered rising one is
/// left alone); a grounded one rises unless within 2 px.
const TOP_BAND: f32 = 16.0;
const TOP_GROUNDED: f32 = 2.0;
/// The marked rise boost and the caps (unmarked, marked; Sky night: unmarked, marked).
const BOOST: f32 = 1.0;
const CAP: [f32; 2] = [3.0, 5.0];
const CAP_NIGHT: [f32; 2] = [2.5, 3.5];
const CAP_WATER: [f32; 2] = [1.5, 3.0];
/// In water the band gravity is halved (0x710158a590: the band table × 0.5).
const WATER_GRAVITY: f32 = 0.5;

/// What a Twister's contact hands the player for this frame (the receiver's top +0x18, the
/// Twister's movement +0x20 and its column centre +0x2c) [obs v3.0.3 0x71013ec330].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwisterPull {
    pub top: f32,
    pub vx: f32,
    pub cx: f32,
}

/// The block's outcome for the rest of the frame's movement.
pub(super) struct TwisterMove {
    /// This frame's gravity (turned upward while rising).
    pub gravity: f32,
    /// The steer replaced the air control.
    pub steered: bool,
}

/// The player's twister record (+0x484 bits 28 / 29, the steer timer +0x2888).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TwisterState {
    /// In a column (bit 28).
    inside: bool,
    /// Jump pressed in the column (bit 29).
    marked: bool,
    steer: i32,
}

impl Player {
    /// The twister block, once a frame before the movement; None outside a column (or when the
    /// block leaves the player alone).
    pub(super) fn twister_block(&mut self, pressed: bool) -> Option<TwisterMove> {
        if self.tw.steer > 0 {
            self.tw.steer -= 1;
        }
        let Some(tw) = self.twister.filter(|_| self.yoshi.is_none()) else {
            self.tw.inside = false;
            self.tw.marked = false;
            return None;
        };
        let under_top = tw.top - self.y;
        let rising = if self.on_ground {
            self.on_ground = false;
            self.vy = self.vy.max(0.0);
            under_top >= TOP_GROUNDED
        } else {
            self.vy >= 0.0 || under_top >= TOP_BAND
        };
        if !(self.tw.inside || self.vy <= 0.0 || under_top >= TOP_BAND) {
            return None;
        }
        if !self.tw.inside {
            self.tw.inside = true;
            if rising {
                self.tw.steer = STEER_FRAMES;
            }
        }
        let steered = self.tw.steer != 0;
        if steered {
            self.vx = approach(self.vx, tw.vx, STEER_EASE);
            let pull = 2.0 * tw.vx.abs();
            if pull < (self.x - tw.cx).abs() {
                self.x = approach(self.x, tw.cx, pull);
            }
        }
        let mut g = GRAVITY_RELEASED[vy_band(self.vy)];
        if self.in_water {
            g *= WATER_GRAVITY;
        }
        if self.vy > 0.0 && self.tw.marked {
            self.vy += BOOST;
        }
        if pressed {
            self.tw.marked = true;
        }
        let caps = if self.low_gravity {
            CAP_NIGHT
        } else if self.in_water {
            CAP_WATER
        } else {
            CAP
        };
        self.vy = self.vy.min(caps[self.tw.marked as usize]);
        Some(TwisterMove {
            gravity: if rising { -g } else { g },
            steered,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;

    fn pull(top: f32) -> Option<TwisterPull> {
        Some(TwisterPull {
            top,
            vx: 0.5,
            cx: 100.0,
        })
    }

    #[test]
    fn a_grounded_player_is_lifted_and_rises_at_the_cap() {
        let lvl = Level::flat(40);
        let mut p = Player::new(96.0, 32.0);
        p.on_ground = true;
        p.twister = pull(140.0);
        let mut ys = vec![];
        for _ in 0..40 {
            p.step(crate::input::Input::default(), &lvl);
            ys.push(p.y);
        }
        // off the ground on the first frame, then the upward bands: 0.25, 0.25 + 0.25, … up to
        // the 3.0 cap (+0.34 integrated)
        assert!(ys[0] > 32.0 && ys[0] < 32.5, "{}", ys[0]);
        assert!(!p.on_ground);
        let v: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        assert!((v[30] - 3.34).abs() < 1e-3, "{v:?}");
        // the steer pulled the player toward the centre (the pull is 1 px a frame)
        assert!(p.x > 96.0);
    }

    #[test]
    fn a_jump_in_the_column_boosts_up_to_the_marked_cap() {
        let lvl = Level::flat(40);
        let mut p = Player::new(100.0, 60.0);
        p.on_ground = false;
        p.vy = 2.0;
        p.twister = pull(400.0);
        let jump = crate::input::Input {
            buttons: crate::input::JUMP,
        };
        // marked by the first frame's press: from the next frame +1.0 a frame (with the upward
        // 0.34) up to the marked cap 5.0
        let mut vys = vec![];
        for _ in 0..4 {
            p.step(jump, &lvl);
            vys.push(p.vy);
        }
        let want = [2.34, 3.68, 5.02, 5.34];
        for (v, w) in vys.iter().zip(want) {
            assert!((v - w).abs() < 1e-3, "{vys:?}");
        }
    }

    #[test]
    fn in_water_the_column_lifts_at_half_gravity_up_to_1_5() {
        let lvl = Level::flat(40);
        let mut p = Player::new(100.0, 60.0);
        p.on_ground = false;
        p.in_water = true;
        p.was_in_water = true;
        p.vy = 1.0;
        p.twister = pull(400.0);
        let m = p.twister_block(false).unwrap();
        assert!((m.gravity - 0.17).abs() < 1e-6, "{}", m.gravity);
        let mut vys = vec![];
        for _ in 0..30 {
            p.step(crate::input::Input::default(), &lvl);
            vys.push(p.vy);
        }
        // capped at 1.5 before the column's +0.17
        assert!((vys[29] - 1.67).abs() < 1e-3, "{vys:?}");
    }

    #[test]
    fn falling_near_the_top_takes_the_normal_gravity() {
        let mut p = Player::new(100.0, 100.0);
        p.on_ground = false;
        p.vy = -1.0;
        p.twister = pull(110.0);
        let m = p.twister_block(false).unwrap();
        assert!(m.gravity < 0.0);
        // 16 px or more under the top the column turns it upward again
        let mut p = Player::new(100.0, 90.0);
        p.on_ground = false;
        p.vy = -1.0;
        p.twister = pull(110.0);
        assert!(p.twister_block(false).unwrap().gravity > 0.0);
    }
}
