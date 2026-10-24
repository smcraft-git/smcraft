//! The Fire Koopa Clown Car's own fireballs with an enemy rider (42-koopa-clown-car.md "Pass 5").
//!
//! The car actor (constructor 0x710117e960, v3.0.3) embeds a helper component (vtable
//! 0x71028e83f8, pointed to by the car's +0x4e8) whose init (0x7101191550) allocates a fire
//! record only for the Fire car (flag 0x4). The helper's update (0x7101191a20) runs the record:
//! with an enemy (not a player) rider, once its cooldown is out the car **charges** for 90
//! frames and fires one KoopaClownFire shot; Chase (0x710118bc60) brakes the car through the
//! charge and eases its steering back afterwards. Bosses in a Fire car (the engine merges the
//! car into the boss) run the same record. Values are [obs v3.0.3] unless marked.

use super::{Actor, Kind};
use crate::fmath::DetMath;

/// The cooldown at the first Chase, and the base of every later draw: [90, 180) frames
/// (Chase enter → 0x7101190d50; after a shot 0x7101191a20) [obs v3.0.3].
const COOLDOWN: i32 = 90;
/// The charge: 90 frames counted once the cooldown is out; the shot leaves on the next
/// (the counter is capped at 90, the shot taken when it already was) [obs v3.0.3 0x7101191a20].
const CHARGE: i32 = 90;
/// The charge weight (C − 15) / 45 (DAT 0x7102319010): the car starts braking at 0.1 and
/// brakes harder, holding its facing, from 0.8 (DAT 0x7102318b04 / 0x7102318a54) [obs v3.0.3].
const CHARGE_START: i32 = 15;
const CHARGE_SPAN: f32 = 45.0;
const BRAKE_FROM: f32 = 0.1;
const HARD_FROM: f32 = 0.8;
/// Per-frame braking while charging: × 0.99, × 0.95 once hard (table 0x7102319e98).
const BRAKE: [f32; 2] = [0.99, 0.95];
/// After the shot the steering weight comes back from 0 to 1 by 1/30 a frame
/// (DAT 0x7102318c18).
const RECOVER_STEP: f32 = 1.0 / 30.0;
/// The shot leaves 6 px ahead and 14 px above the car's origin (× the car's scale), at
/// 1.5 px/f plus the car's recent speed when that is along its facing (EnemyShot exe
/// 0x7100ddeb30; the speed is passed at the spawn) [obs v3.0.3].
const SHOT_AHEAD: f32 = 6.0;
const SHOT_UP: f32 = 14.0;
pub const SHOT_SPEED: f32 = 1.5;
/// Frames of the car's speed averaged for the shot (the car's 16-entry ring at +0x578)
/// [obs v3.0.3 capacity; inf: it holds the car's x speed per frame].
const HISTORY: usize = 16;
/// A player's shots (the helper's player branch, 0x7101191a20): the hold counter's cap and
/// gate, the cooldowns, the most out at once, and PlayerShot's speed (exe 0x7100ddf010) [obs
/// v3.0.3].
const PLAYER_CHARGE: i32 = 60;
const PLAYER_HOLD_GATE: i32 = 51;
const PLAYER_COOLDOWN: i32 = 12;
const PLAYER_COOLDOWN_LONG: i32 = 50;
const PLAYER_MAX: usize = 8;
pub const PLAYER_SHOT_SPEED: f32 = 3.625;
/// The charged shot (ChargeShot enter / exe 0x7100ddf380 / 0x7100ddf610): 4.0 px/f along its
/// angle (the sine table 0x71023385b8 × 4) plus the inherited speed, no life count; terrain
/// ends it in ChargeExplode (0x7100ddf950: a 3-frame burst snapped to the tile it hit,
/// sweeping the tile and its neighbours with hit kind 2, 0x7100e22210) [obs v3.0.3].
pub const CHARGE_SHOT_SPEED: f32 = 4.0;
/// The tiles a charged shot's ChargeExplode hits (enter 0x7100ddf950) [obs v3.0.3]: its
/// position snapped to the centre of its 16-px tile, moved one tile toward what it hit (`side`
/// 1 a wall, the way it faces; 2 the floor, down; 3 a ceiling, up); then the block hit
/// 0x7100e22210 along that tile's row and column ±16 (±32 big, plus the 3 × 3 box round it),
/// and, when it hit something (side ≠ 0), the column one tile behind it (±16 up and down).
/// That block hit is the one a Bob-omb's and a Mechakoopa missile's blasts use (hit kind 2).
pub fn charge_burst_tiles(x: f32, y: f32, side: u8, dir: i32, big: bool) -> Vec<(i32, i32)> {
    let mut tx = (x / 16.0).floor() as i32;
    let mut ty = (y / 16.0).floor() as i32;
    let back = if dir < 0 { -1 } else { 1 };
    match side {
        3 => ty += 1,
        2 => ty -= 1,
        1 => tx += back,
        _ => {}
    }
    let r = if big { 2 } else { 1 };
    let mut tiles = Vec::new();
    let mut add = |t: (i32, i32)| {
        if !tiles.contains(&t) {
            tiles.push(t);
        }
    };
    if big {
        for dy in -1..=1 {
            for dx in -1..=1 {
                add((tx + dx, ty + dy));
            }
        }
    }
    for d in -r..=r {
        add((tx + d, ty));
        add((tx, ty + d));
    }
    if side != 0 {
        for dy in -1..=1 {
            add((tx - back, ty + dy));
        }
    }
    tiles
}

/// Hard cap on a shot's flight (it has no timer of its own; it ends on terrain or off
/// screen) [inf].
pub const SHOT_LIFE: i32 = 600;

/// The Fire car's fire record (+0x18 of the helper) and Chase's lean fields (+0x540 on the
/// car, +0x308 / +0x30c on its action).
#[derive(Clone, Debug, PartialEq)]
pub struct ClownFire {
    /// Cooldown (+0x08), counting down every frame.
    pub timer: i32,
    /// Charge counter (+0x0c).
    pub charge: i32,
    /// Chase has armed the record (+0xc3: set by Chase enter, cleared by its exit).
    pub armed: bool,
    /// No Chase yet: the first cooldown is exactly 90 (+0xc2).
    pub first: bool,
    /// Chase's lean phase (+0x540): 0 steering, 1 braking through the charge, 2 recovering.
    pub phase: u8,
    /// Steering weight while recovering (+0x308; −1 = none) and the direction the car was
    /// moving when the charge began (+0x30c / +0x310).
    pub weight: f32,
    pub lean: (f32, f32),
    /// The record's state this frame (+0x18): it fired.
    pub fired: bool,
    /// A shot to add: (x, y, vx).
    pub shot: Option<(f32, f32, f32)>,
    /// A player rider's fire input as last seen (+0x14 / +0x15).
    pub held: bool,
    history: [f32; HISTORY],
    filled: usize,
    next: usize,
}

impl Default for ClownFire {
    fn default() -> Self {
        Self {
            timer: 0,
            charge: 0,
            armed: false,
            first: true,
            phase: 0,
            weight: -1.0,
            lean: (0.0, 0.0),
            fired: false,
            shot: None,
            held: false,
            history: [0.0; HISTORY],
            filled: 0,
            next: 0,
        }
    }
}

impl ClownFire {
    /// The charge weight (C − 15) / 45, from 0 (none below 15) [obs v3.0.3].
    fn charge_weight(&self) -> f32 {
        if self.charge < CHARGE_START {
            0.0
        } else {
            (self.charge - CHARGE_START) as f32 / CHARGE_SPAN
        }
    }

    /// Facing may follow the target: not while recovering, nor once the charge is hard
    /// (Chase 0x710118bc60) [obs v3.0.3].
    pub fn may_turn(&self) -> bool {
        self.phase != 2 && self.charge_weight() < HARD_FROM
    }

    /// The car's recent x speed (the mean of the ring).
    fn mean_speed(&self) -> f32 {
        if self.filled == 0 {
            0.0
        } else {
            self.history[..self.filled].iter().sum::<f32>() / self.filled as f32
        }
    }

    fn record(&mut self, vx: f32) {
        self.history[self.next] = vx;
        self.next = (self.next + 1) % HISTORY;
        self.filled = (self.filled + 1).min(HISTORY);
    }
}

impl Actor {
    /// Chase begins (0x710118baa0 → 0x7101190d50): the cooldown is 90 the first time, else
    /// [90, 180); the charge and the lean restart [obs v3.0.3].
    fn clown_fire_arm(&mut self) {
        let draw = COOLDOWN + self.rand(COOLDOWN as u32) as i32;
        let Some(cf) = self.clown_fire.as_mut() else {
            return;
        };
        cf.timer = if cf.first { COOLDOWN } else { draw };
        cf.first = false;
        cf.charge = 0;
        cf.armed = true;
        cf.phase = 0;
        cf.weight = -1.0;
    }

    /// The car is not chasing (Free, KoopaJrStop, no rider): Chase's exit (0x710118d070)
    /// disarms the record and drops the charge; the cooldown keeps running. Also records the
    /// car's speed.
    pub(super) fn clown_fire_idle(&mut self) {
        let vx = self.vx;
        // (the Chase exit 0x710118d070 lets the budget's unit go)
        if self.clown_fire.is_some() {
            self.budget_release();
        }
        if let Some(cf) = self.clown_fire.as_mut() {
            cf.record(vx);
            cf.fired = false;
            cf.armed = false;
            cf.charge = 0;
            cf.phase = 0;
            cf.weight = -1.0;
            cf.timer = (cf.timer - 1).max(0);
        }
    }

    /// One Chase frame of a Koopa Clown Car under an enemy rider: the car actor, or a boss
    /// carrying its car (`car` = the car's origin, `scale` its size). (dx, dy) is the vector
    /// to the target and `top` / `accel` the plain car's steering; this applies the Fire car's
    /// own rules (top speed × 0.5, acceleration × 0.75 [obs v3.0.3 0x710118bc60]) and its
    /// charge, and steers. Returns whether the rider may turn to face the target.
    pub(super) fn clown_chase(
        &mut self,
        (dx, dy): (f32, f32),
        (top, accel): (f32, f32),
        car: (f32, f32),
        scale: f32,
    ) -> bool {
        let d = dx.dhypot(dy);
        let unit = if d > 0.0 {
            (dx / d, dy / d)
        } else {
            (0.0, 0.0)
        };
        if self.clown_fire.is_none() {
            self.vx += accel * unit.0;
            self.vy += accel * unit.1;
            self.cap_speed(top);
            return true;
        }
        if !self.clown_fire.as_ref().is_some_and(|c| c.armed) {
            self.clown_fire_arm();
        }
        let (vx, vy, dir) = (self.vx, self.vy, self.dir);
        // the helper's update (0x7101191a20): the cooldown runs, then the charge — each frame
        // of it once the area's projectile budget grants a unit (0x7100de9960), held until the
        // Chase exit (0x710118d070) [obs v3.0.3]; on the frame after a full charge the shot
        // leaves and the cooldown is redrawn
        let ready = self.clown_fire.as_mut().is_some_and(|cf| {
            cf.record(vx);
            cf.timer = (cf.timer - 1).max(0);
            cf.fired = false;
            cf.timer < 1
        });
        let fire = ready
            && self.budget_request(1)
            && self.clown_fire.as_mut().is_some_and(|cf| {
                let before = cf.charge;
                cf.charge = (cf.charge + 1).min(CHARGE);
                before >= CHARGE
            });
        let draw = if fire {
            COOLDOWN + self.rand(COOLDOWN as u32) as i32
        } else {
            0
        };
        let Some(cf) = self.clown_fire.as_mut() else {
            return true;
        };
        if fire {
            cf.timer = draw;
            cf.charge = 0;
            cf.fired = true;
            let s = dir as f32;
            let inherit = cf.mean_speed();
            let inherit = if inherit * s < 0.0 { 0.0 } else { inherit };
            cf.shot = Some((
                car.0 + SHOT_AHEAD * s * scale,
                car.1 + SHOT_UP * scale,
                SHOT_SPEED * s + inherit,
            ));
        }
        // Chase's lean phases
        let w = cf.charge_weight();
        match cf.phase {
            2 => {
                let n = cf.weight + RECOVER_STEP;
                if n < 1.0 {
                    cf.weight = n;
                } else {
                    cf.weight = -1.0;
                    cf.phase = 0;
                }
            }
            1 => {
                if cf.fired {
                    cf.phase = 2;
                }
            }
            _ => {
                if cf.charge > CHARGE_START - 1 && w >= BRAKE_FROM {
                    let v = vx.dhypot(vy);
                    if v > 0.0 {
                        cf.lean = (vx / v, vy / v);
                    }
                    cf.weight = 0.0;
                    cf.phase = 1;
                }
            }
        }
        let turn = cf.may_turn();
        if cf.phase == 1 && cf.charge > CHARGE_START - 1 && w >= BRAKE_FROM {
            let k = BRAKE[usize::from(w.min(1.0) >= HARD_FROM)];
            self.vx *= k;
            self.vy *= k;
            return turn;
        }
        let (top, accel) = (top * 0.5, accel * 0.75);
        let (ax, ay) = if cf.weight >= 0.0 {
            let (a, b) = (
                accel * cf.weight * cf.weight,
                accel * cf.weight * (1.0 - cf.weight),
            );
            (a * unit.0 + b * cf.lean.0, a * unit.1 + b * cf.lean.1)
        } else {
            (accel * unit.0, accel * unit.1)
        };
        self.vx += ax;
        self.vy += ay;
        self.cap_speed(top);
        turn
    }

    fn cap_speed(&mut self, top: f32) {
        let v = self.vx.dhypot(self.vy);
        if v > top {
            self.vx *= top / v;
            self.vy *= top / v;
        }
    }

    /// A player flying the Fire car (the helper's player branch, 0x7101191a20) [obs v3.0.3]:
    /// each change of the fire input (`held`: Run) fires when the cooldown is out, unless the
    /// input was held 60 frames — that release is the charged shot (ChargeShot, state 3:
    /// `phase` 1, faster and without a life count). The cooldown is then 12
    /// frames with none of its shots out, else 50; at most 8 are out (`alive`). A shot is one
    /// PlayerShot, or three at 0° / +15° / −15° when the player's +0x4a8 is 2 ([inf]: Fire
    /// form). Returns the shots: `Kind::Fireball` with `alt` (actors.rs: 3.625 px/f along its
    /// angle plus the car's speed, put out by terrain, 600 frames).
    pub fn clown_fire_player(&mut self, held: bool, triple: bool, alive: usize) -> Vec<Actor> {
        let (x, y, dir, scale) = (self.x, self.y, self.dir, if self.big { 2.0 } else { 1.0 });
        let vx = self.vx;
        let Some(cf) = self.clown_fire.as_mut() else {
            return Vec::new();
        };
        cf.record(vx);
        cf.timer = (cf.timer - 1).max(0);
        let old = cf.charge;
        let now = if !held {
            cf.charge = 0;
            false
        } else if cf.timer < PLAYER_HOLD_GATE {
            cf.charge = (cf.charge + 1).min(PLAYER_CHARGE);
            true
        } else {
            cf.charge = 0;
            true
        };
        let edge = now != cf.held;
        cf.held = now;
        let charged = old >= PLAYER_CHARGE;
        if !edge || !(charged || cf.timer == 0) || alive >= PLAYER_MAX {
            return Vec::new();
        }
        cf.timer = if !charged && alive == 0 {
            PLAYER_COOLDOWN
        } else {
            PLAYER_COOLDOWN_LONG
        };
        let s = dir as f32;
        let inherit = cf.mean_speed();
        let inherit = if inherit * s < 0.0 { 0.0 } else { inherit };
        let angles: &[f32] = if triple { &[0.0, 15.0, -15.0] } else { &[0.0] };
        angles
            .iter()
            .map(|&a| {
                let (sn, cs) = a.to_radians().dsin_cos();
                let mut f = Actor::new(
                    Kind::Fireball,
                    x + SHOT_AHEAD * s * scale,
                    y + SHOT_UP * scale,
                );
                f.alt = true;
                f.dir = dir;
                let v = if charged {
                    f.phase = 1;
                    CHARGE_SHOT_SPEED
                } else {
                    PLAYER_SHOT_SPEED
                };
                f.vx = v * cs * s + inherit;
                f.vy = v * sn;
                f.timer = SHOT_LIFE;
                f
            })
            .collect()
    }

    /// The shot the car's fire record let go this frame, if any: a KoopaClownFire in its
    /// EnemyShot state (`Kind::EnemyFire` with `alt`: flies level, snuffed by terrain).
    pub fn take_clown_shot(&mut self) -> Option<Actor> {
        let (x, y, vx) = self.clown_fire.as_mut()?.shot.take()?;
        let mut f = Actor::new(Kind::EnemyFire, x, y);
        f.alt = true;
        f.dir = if vx < 0.0 { -1 } else { self.dir };
        f.vx = vx;
        f.timer = SHOT_LIFE;
        Some(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_charged_shot_bursts_in_a_plus_with_the_column_behind() {
        // flying right into a wall at tile (10, 4): centre moved into the wall tile (11, 4)
        let t = charge_burst_tiles(10.0 * 16.0 + 15.0, 4.0 * 16.0 + 6.0, 1, 1, false);
        let mut t2 = t.clone();
        t2.sort();
        assert_eq!(
            t2,
            vec![
                (10, 3),
                (10, 4),
                (10, 5),
                (11, 3),
                (11, 4),
                (11, 5),
                (12, 4)
            ]
        );
        // a big one on the floor at tile (6, 3): centre (6, 2), the 3 × 3 box, the arms two
        // tiles out; the column behind (facing left: x + 1) is inside the box
        let mut b = charge_burst_tiles(100.0, 50.0, 2, -1, true);
        b.sort();
        let mut want = vec![(4, 2), (8, 2), (6, 0), (6, 4)];
        for dy in 1..=3 {
            for dx in 5..=7 {
                want.push((dx, dy));
            }
        }
        want.sort();
        assert_eq!(b, want);
    }

    fn fire_car() -> Actor {
        let mut a = Actor::new(Kind::ClownCar { fire: true }, 100.0, 100.0);
        a.rng = 7;
        a
    }

    /// Chase frames toward a target far to the right; the frame numbers of the shots.
    fn chase(a: &mut Actor, frames: usize) -> Vec<usize> {
        let mut shots = Vec::new();
        for f in 1..=frames {
            a.clown_chase((200.0, 0.0), (1.0, 0.02), (a.x, a.y), 1.0);
            if let Some(s) = a.take_clown_shot() {
                assert_eq!(s.kind, Kind::EnemyFire);
                shots.push(f);
            }
        }
        shots
    }

    #[test]
    fn first_shot_after_cooldown_and_charge() {
        let mut a = fire_car();
        let shots = chase(&mut a, 700);
        // 89 frames of cooldown after the Chase frame, 90 of charge, the shot on the next
        assert_eq!(shots[0], 180);
        // later: a [90, 180) cooldown + 91
        assert!(shots.len() >= 3);
        for w in shots.windows(2) {
            let gap = w[1] - w[0];
            assert!((181..=270).contains(&gap), "{gap}");
        }
    }

    #[test]
    fn brakes_while_charging_then_recovers() {
        let mut a = fire_car();
        chase(&mut a, 108); // charge 19: weight 0.089, still steering
        assert_eq!(a.clown_fire.as_ref().unwrap().phase, 0);
        let v0 = a.vx;
        assert!((v0 - 0.5).abs() < 1e-5, "Fire car top speed {v0}");
        chase(&mut a, 1); // charge 20: weight 0.111, braking
        assert_eq!(a.clown_fire.as_ref().unwrap().phase, 1);
        assert!((a.vx - v0 * 0.99).abs() < 1e-6);
        chase(&mut a, 30); // charge 50
        assert!(a.clown_fire.as_ref().unwrap().may_turn());
        let v = a.vx;
        chase(&mut a, 1); // charge 51: weight 0.8, hard braking, facing held
        assert!(!a.clown_fire.as_ref().unwrap().may_turn());
        assert!((a.vx - v * 0.95).abs() < 1e-6);
        assert_eq!(chase(&mut a, 40), vec![40]); // frame 180: the shot, recovery starts
        assert_eq!(a.clown_fire.as_ref().unwrap().phase, 2);
        assert!(!a.clown_fire.as_ref().unwrap().may_turn());
        chase(&mut a, 31);
        assert_eq!(a.clown_fire.as_ref().unwrap().phase, 0);
    }

    #[test]
    fn shot_leaves_ahead_with_the_cars_speed() {
        let mut a = fire_car();
        a.vx = 0.4;
        let mut shot = None;
        for _ in 0..200 {
            a.clown_chase((200.0, 0.0), (1.0, 0.02), (a.x, a.y), 1.0);
            if let Some(s) = a.take_clown_shot() {
                shot = Some(s);
                break;
            }
        }
        let s = shot.expect("shot");
        assert!(s.alt);
        assert!((s.x - (a.x + 6.0)).abs() < 1e-4);
        assert!((s.y - (a.y + 14.0)).abs() < 1e-4);
        assert!(s.vx > SHOT_SPEED, "{}", s.vx);
    }

    #[test]
    fn leaving_chase_disarms_and_rearms_with_a_random_cooldown() {
        let mut a = fire_car();
        chase(&mut a, 120);
        a.clown_fire_idle();
        let cf = a.clown_fire.as_ref().unwrap();
        assert!(!cf.armed);
        assert_eq!(cf.charge, 0);
        chase(&mut a, 1);
        let t = a.clown_fire.as_ref().unwrap().timer;
        assert!((89..179).contains(&t), "{t}");
    }

    #[test]
    fn a_player_fires_on_each_change_of_the_button_after_the_cooldown() {
        let mut a = fire_car();
        let mut fired = Vec::new();
        // press for 5 frames, release, wait; then hold 70 frames and release
        let script: Vec<bool> = (0..5)
            .map(|_| true)
            .chain((0..30).map(|_| false))
            .chain((0..70).map(|_| true))
            .chain((0..5).map(|_| false))
            .collect();
        for (f, held) in script.into_iter().enumerate() {
            let shots = a.clown_fire_player(held, false, 0);
            if !shots.is_empty() {
                assert_eq!(shots[0].kind, Kind::Fireball);
                let v = if shots[0].phase == 1 { 4.0 } else { 3.625 };
                assert!(shots[0].alt && (shots[0].vx - v).abs() < 1e-4);
                fired.push((f, shots[0].phase));
            }
        }
        // the press; the release 5 frames later is inside the 12-frame cooldown; the next press;
        // the release after a 60-frame hold (the charged shot)
        assert_eq!(
            fired.iter().map(|f| f.0).collect::<Vec<_>>(),
            vec![0, 35, 105]
        );
        assert_eq!(fired.iter().map(|f| f.1).collect::<Vec<_>>(), vec![0, 0, 1]);
        let three = {
            let mut b = fire_car();
            b.clown_fire_player(true, true, 0)
        };
        assert_eq!(three.len(), 3);
        assert!(three[1].vy > 0.0 && three[2].vy < 0.0);
    }

    #[test]
    fn plain_car_has_no_record() {
        let mut a = Actor::new(Kind::ClownCar { fire: false }, 0.0, 0.0);
        for _ in 0..400 {
            a.clown_chase((200.0, 0.0), (1.0, 0.02), (0.0, 0.0), 1.0);
            assert!(a.take_clown_shot().is_none());
        }
        assert!((a.vx - 1.0).abs() < 1e-4);
    }
}
