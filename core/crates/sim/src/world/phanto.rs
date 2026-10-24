//! Phanto (`MapObjKamen`), the mask that chases whoever holds a Cursed Key (the SMB1 Key's 0x4
//! form, `KeyCurse`). v3.0.3: states registered in 0x71013baa90 — Wait (enter 0x71013bb630,
//! exec 0x71013bb6d0), WarpWait (0x71013bb770 / 0x71013bb810), Attack (0x71013bb990 /
//! 0x71013bbbc0), Leave (0x71013bc610 / 0x71013bc7c0); the swoop start 0x71013bd110 and the
//! position 0x71013bd010; the manager 0x71013bd460 keeps one per player holding a cursed key
//! [obs v3.0.3].
//!
//! Phanto rides an ellipse (128 × 24 px) round a centre that eases toward its player: each half
//! turn (0.8° of the horizontal angle a frame) it starts over from where it is, swinging back
//! the other way; the vertical angle runs at 0.8° × a speed that eases toward 3 within 32 px of
//! the player's height (else 1). With no cursed key held it drifts off, and waits out of view
//! (at its offset from the view centre) until someone holds one again.

use super::World;
use crate::fmath::DetMath;
use crate::world::{VIEW_H, VIEW_W};

/// 180° in the angle units (sead: 2³¹ = 180°).
const HALF_TURN: i64 = 1 << 31;
const RX: f32 = 128.0;
const RY: f32 = 24.0;
/// The angle step's base, degrees a frame (+0x618).
const STEP: f32 = 0.8;
/// Frames in Wait / WarpWait before the swoop.
const WAIT: i32 = 100;
const WARP_WAIT: i32 = 60;
/// Frames at the swoop's start in which it can't hurt (+0x614 = 0x48) [inf: where it counts
/// down was not traced].
const HARMLESS: i32 = 72;
/// Leave's countdown (+0x670); 15 when the player warps / is gone.
const LEAVE: i32 = 500;
/// The offset from the view centre it waits at when it left in view: up-left, out of sight.
const HIDE: (f32, f32) = (-160.0, 112.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Wait,
    WarpWait,
    Attack,
    Leave,
}

/// One Phanto.
#[derive(Clone, Debug, PartialEq)]
pub struct Phanto {
    /// The player slot it chases (None: waits out of sight).
    pub target: Option<usize>,
    pub state: State,
    pub x: f32,
    pub y: f32,
    timer: i32,
    /// Its place relative to the view centre while waiting (+0x600).
    offset: (f32, f32),
    /// The ellipse's centre (+0x61c / +0x620) and the x it eases toward (+0x648).
    cx: f32,
    cy: f32,
    tx: f32,
    /// Horizontal angle (+0x628), its phase (+0x62c), vertical angle (+0x630).
    ax: i64,
    phase: i64,
    ay: i64,
    /// Swinging left (+0x26c = 1); the vertical direction is up (+0x644 = 2).
    left: bool,
    up: bool,
    /// The vertical angle's speed (+0x654), the centre's top speeds (+0x63c, +0x640).
    speed: f32,
    xmax: f32,
    ymax: f32,
    /// Its x speed (+0x23c), the ellipse's last x (+0x658), an extra centre speed (+0x660).
    vx: f32,
    last_x: f32,
    extra: f32,
    /// +0x674 bits: 1 chase the turn point, 2 resume, 4 key held again (Leave), 0x10 keep vx,
    /// 0x20 the turn point eases after the player.
    flags: u8,
    harmless: i32,
    /// Leave: the drift speed (+0x664) and directions (+0x668 / +0x66c).
    drift: f32,
    drift_left: bool,
    drift_down: bool,
}

/// The angle in radians (the units wrap at a full turn).
fn rad(a: i64) -> f32 {
    (a.rem_euclid(2 * HALF_TURN) as f64 * std::f64::consts::PI / HALF_TURN as f64) as f32
}

fn sin(a: i64) -> f32 {
    rad(a).dsin()
}

fn cos(a: i64) -> f32 {
    rad(a).dcos()
}

/// `v` toward `to` by `step`, not past it.
fn ease(v: f32, to: f32, step: f32) -> f32 {
    if v < to {
        (v + step).min(to)
    } else {
        (v - step).max(to)
    }
}

/// The centre's step toward a target: a fraction `k` of the distance, at least 0.01, at most
/// `max`.
fn chase(from: f32, to: f32, k: f32, max: f32) -> f32 {
    let d = ((to - from) * k).abs();
    let step = if d >= 0.01 { d.min(max) } else { 0.01 };
    ease(from, to, step)
}

impl Phanto {
    fn new(target: Option<usize>, centre: (f32, f32)) -> Self {
        Self {
            target,
            state: State::Wait,
            x: centre.0 + HIDE.0,
            y: centre.1 + HIDE.1,
            timer: WAIT,
            offset: HIDE,
            cx: 0.0,
            cy: 0.0,
            tx: 0.0,
            ax: 0,
            phase: 0,
            ay: 0,
            left: false,
            up: true,
            speed: 1.0,
            xmax: 1.0,
            ymax: 3.0,
            vx: 0.0,
            last_x: 0.0,
            extra: 0.0,
            flags: 0,
            harmless: 0,
            drift: 0.5,
            drift_left: false,
            drift_down: false,
        }
    }

    /// The swoop's start (0x71013bd110): the ellipse begins at Phanto, its centre 128 px toward
    /// the player.
    fn start(&mut self, (px, py): (f32, f32)) {
        self.up = py - self.y >= 0.0;
        self.left = px - self.x < 0.0;
        self.cy = self.y;
        self.last_x = self.x;
        self.cx = self.x + if self.left { -RX } else { RX };
        self.ax = 0;
        self.ay = 0;
        self.xmax = 1.0;
        self.ymax = 3.0;
        self.speed = 1.0;
        self.harmless = HARMLESS;
        self.enter_attack(State::Wait, px);
    }

    /// Attack's enter (0x71013bb990): each half turn picks the side again and, when still on
    /// the same side, carries the ellipse on from where it is.
    fn enter_attack(&mut self, from: State, px: f32) {
        if from != State::Attack {
            self.extra = 0.0;
        }
        if self.flags & 2 == 0 {
            self.ax = 0;
            let was = self.left;
            let d = px - self.x;
            if d.abs() >= 16.0 {
                self.left = d < 0.0;
            }
            self.tx = px;
            self.phase = if self.left { 0 } else { HALF_TURN };
            self.flags &= !1;
            if was == self.left && from == State::Attack {
                let r = if self.left { -RX } else { RX };
                self.cx = self.x + r;
                self.tx = px + r;
                self.flags |= 0x11;
                self.last_x = self.cx + cos(self.phase) * RX;
            } else if (self.x - px).abs() >= 256.0 && from == State::Attack {
                self.tx = px + if self.left { -RX } else { RX };
                self.flags |= 1;
            } else {
                self.xmax = 0.0;
            }
        }
        self.flags &= !2;
        self.state = State::Attack;
    }

    /// The position from the ellipse (0x71013bd010): x moves by the ellipse's change (or keeps
    /// its speed while flag 0x10 holds and the change is smaller), y is placed.
    fn place(&mut self) {
        let nx = self.cx + RX * cos(self.ax + self.phase);
        let mut dx = nx - self.last_x;
        self.last_x = nx;
        if self.flags & 0x10 != 0 && self.vx.abs() <= dx.abs() {
            self.flags &= !0x10;
        }
        if self.flags & 0x10 == 0 {
            self.vx = dx;
        } else {
            dx = self.vx;
        }
        self.x += dx;
        self.y = self.cy + RY * sin(self.ay);
    }

    /// Both angles a frame; true at the end of a half turn.
    fn turn(&mut self) -> bool {
        let step = (STEP * (128.0 / RX) * HALF_TURN as f32 / 180.0) as i64;
        let done = self.ax + step >= HALF_TURN;
        self.ax = if done { HALF_TURN } else { self.ax + step };
        let vstep = (STEP * self.speed * (24.0 / RY) * HALF_TURN as f32 / 180.0) as i64;
        self.ay = (self.ay + if self.up { vstep } else { -vstep }).rem_euclid(2 * HALF_TURN);
        done
    }

    /// Attack's exec (0x71013bbbc0); false: leave.
    fn attack(&mut self, (px, py, pvx): (f32, f32, f32), holding: bool) -> bool {
        if !holding {
            return false;
        }
        self.harmless = (self.harmless - 1).max(0);
        let dy = py - self.cy;
        self.speed = if dy.abs() <= 32.0 {
            ease(self.speed, 3.0, 0.05)
        } else {
            ease(self.speed, 1.0, 0.05)
        };
        let done = self.turn();
        self.xmax = ease(self.xmax, 1.0, 0.01).max(0.011);
        let xmax = self.xmax;
        let old = self.cx;
        if self.flags & 1 == 0 {
            self.cx += self.extra;
            self.extra = ease(self.extra, 0.0, 0.01);
            let mut target = px;
            if self.flags & 0x20 != 0 {
                self.tx = chase(self.tx, px, 0.01, xmax);
                target = self.tx;
            }
            if (px - old) * self.vx > 0.0
                || (self.flags & 0x20 != 0 && self.vx * (target - self.cx) > 0.0)
            {
                self.cx = chase(self.cx, target, 0.005, xmax);
            }
        } else {
            // swinging past the player toward the turn point: the turn point follows them, the
            // centre matches their speed [obs; simplified: the speed match keeps the centre's
            // extra speed at the player's (at least −1)]
            let sign = if self.left { -1.0 } else { 1.0 };
            let ahead = px - self.x;
            let cand = px - sign * RX;
            if (cand - self.tx) * -sign > 0.0 && ahead * self.vx > 0.0 {
                self.tx = cand;
            }
            if (self.tx - self.cx) * -sign > 0.0 && pvx * -sign > 0.0 {
                self.extra = ease(self.extra, pvx.max(-1.0), 0.01);
            }
            self.cx = ease(self.cx, self.tx, self.extra.abs());
            self.cx = chase(self.cx, self.tx, 0.01, xmax);
            if ahead * self.vx < 0.0 && self.vx * -sign > 0.0 {
                self.flags = (self.flags & 0xee) | 0x20;
            }
        }
        self.ymax = ease(self.ymax, 3.0, 0.01);
        self.cy = chase(self.cy, py, 0.01, self.ymax);
        self.place();
        if done {
            self.enter_attack(State::Attack, px);
        }
        true
    }

    /// Leave's enter (0x71013bc610).
    fn leave(&mut self, gone: bool) {
        self.drift = 0.5;
        self.timer = if gone { 15 } else { LEAVE };
        self.drift_left = self.vx < 0.0;
        self.drift_down = self.y < self.last_y();
        self.flags = 0;
        self.state = State::Leave;
    }

    fn last_y(&self) -> f32 {
        self.cy + RY * sin(self.ay - 1)
    }
}

impl World {
    /// The view's centre (the camera's: 0x7100d82bf0).
    fn view_centre(&self) -> (f32, f32) {
        (self.camera_x + VIEW_W / 2.0, self.camera_y + VIEW_H / 2.0)
    }

    /// Slot `k`'s cursed keys, wherever it is this moment.
    pub fn cursed_of(&self, k: usize) -> u32 {
        self.cursed_slot(k)
    }

    /// Keep one Phanto per player holding a cursed key and move them (0x71013bd460 and the
    /// states); a Phanto touching the front player hurts them [inf: the other players' touches
    /// are not checked].
    pub(super) fn step_phantos(&mut self) {
        let holders: Vec<usize> = (0..self.players())
            .filter(|&k| self.cursed_of(k) > 0)
            .collect();
        let centre = self.view_centre();
        // one per holder: keep the ones chasing a holder, give the rest the free holders
        for p in &mut self.phantos {
            if p.target.is_some_and(|k| !holders.contains(&k)) && p.state == State::Wait {
                p.target = None;
            }
        }
        for &k in &holders {
            if self.phantos.iter().any(|p| p.target == Some(k)) {
                continue;
            }
            match self.phantos.iter_mut().find(|p| p.target.is_none()) {
                Some(p) => p.target = Some(k),
                None => self.phantos.push(Phanto::new(Some(k), centre)),
            }
        }
        let mut hurt = false;
        for i in 0..self.phantos.len() {
            let mut p = self.phantos[i].clone();
            let target = p.target.filter(|&k| k < self.players());
            let (pl, holding, warping) = match target {
                Some(k) => {
                    let pl = self.slot(k);
                    let centre_y = pl.y + 8.0;
                    (
                        Some((pl.x, centre_y, pl.vx)),
                        self.cursed_of(k) > 0,
                        self.transit.is_some(),
                    )
                }
                None => (None, false, false),
            };
            match p.state {
                State::Wait | State::WarpWait => {
                    (p.x, p.y) = (centre.0 + p.offset.0, centre.1 + p.offset.1);
                    match pl {
                        Some((px, py, _)) if !(p.state == State::WarpWait && warping) => {
                            p.timer -= 1;
                            if p.timer <= 0 {
                                p.start((px, py));
                            }
                        }
                        Some(_) => p.timer = WARP_WAIT,
                        None => p.timer = WAIT,
                    }
                }
                State::Attack => {
                    let ok = pl.is_some_and(|q| !warping && p.attack(q, holding));
                    if !ok {
                        p.leave(warping || pl.is_none());
                    }
                }
                State::Leave => {
                    // the vertical speed toward 2; the drift grows to 1.5, or stops and the
                    // swoop resumes when its player holds a cursed key again
                    p.speed = ease(p.speed, 2.0, 0.05);
                    if p.turn() {
                        p.left = !p.left;
                        p.phase = if p.left { HALF_TURN } else { 0 };
                        p.ax = 0;
                    }
                    p.cx += p.drift * if p.drift_left { -1.0 } else { 1.0 };
                    p.cy += p.drift * if p.drift_down { -1.0 } else { 1.0 };
                    p.place();
                    if p.flags & 4 == 0 {
                        p.drift = ease(p.drift, 1.5, 0.01);
                        if holding && !warping {
                            p.flags |= 4;
                        }
                    } else {
                        p.drift = (p.drift - 0.03).max(0.0);
                        if p.drift <= 0.0 {
                            p.flags = 2;
                            p.xmax = 0.0;
                            p.ymax = 0.0;
                            p.speed = 1.0;
                            if let Some((px, _, _)) = pl {
                                p.enter_attack(State::Leave, px);
                            }
                            self.phantos[i] = p;
                            continue;
                        }
                    }
                    // in view: count down; out of view (or out of time): wait there
                    let v = self.view_rect(0.0);
                    let inside =
                        p.x + 12.0 > v.0 && p.x - 12.0 < v.2 && p.y + 20.0 > v.1 && p.y - 4.0 < v.3;
                    p.timer -= 1;
                    if !inside || p.timer <= 0 {
                        p.offset = if inside {
                            HIDE
                        } else {
                            (p.x - centre.0, p.y - centre.1)
                        };
                        p.state = State::Wait;
                        p.timer = WAIT;
                    }
                }
            }
            if p.state == State::Attack
                && p.harmless == 0
                && p.target == Some(self.lead_slot())
                && self.player.alive()
            {
                let b = self.player.hurt_box();
                hurt |= b.0 < p.x + 8.0 && b.2 > p.x - 8.0 && b.1 < p.y + 16.0 && b.3 > p.y;
            }
            self.phantos[i] = p;
        }
        if hurt && self.player.star == 0 {
            self.v3_hurt();
        }
    }

    /// After a warp: each Phanto waits 60 frames out of sight before it swoops again.
    pub(super) fn phantos_warped(&mut self) {
        for p in &mut self.phantos {
            p.state = State::WarpWait;
            p.timer = WARP_WAIT;
            p.offset = HIDE;
        }
    }

    pub(super) fn phanto_sprites(&self, out: &mut Vec<crate::world::Sprite>) {
        let v = self.view_rect(32.0);
        for p in &self.phantos {
            if p.x < v.0 || p.x > v.2 || p.y < v.1 || p.y > v.3 {
                continue;
            }
            // `wait_ftp`: 120 frames, eyes shut at 90–119 [obs data]
            let t = self.tick % 120;
            let frame = match t {
                0..90 => "wait.0",
                90..96 | 114.. => "wait.1",
                96..102 | 108..114 => "wait.2",
                _ => "wait.3",
            };
            out.push(Self::model_sprite(
                "M1_Object_phanto",
                frame,
                p.x - 12.0,
                p.y - 4.0,
                24.0,
                24.0,
                false,
                45,
            ));
        }
    }
}
