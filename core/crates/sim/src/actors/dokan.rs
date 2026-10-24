//! What a pipe produces, actor side: the EnemySys **Dokan** context (shared enemy draft §5.3,
//! 9-pipe.md "Dokan context per preset"). A generator's part is created at the mouth (Px, Py:
//! the mouth's outer edge) and slides out with terrain collision off, then hands over through
//! the shared return-to-normal rule (`end_shot`). The presets that differ: a parachute is shot
//! out, a winged RedPataPata flies out, a winged PataTogezo leaves hovering, a Lava Bubble is
//! launched from the mouth, and big parts squeeze out (DokanOutBig → JamBig → JumpOutBig).
//! The world side (when a pipe spawns) is world.rs `step_generators`. Values are [obs v3.0.3]
//! (Default init 0x710105e400); v1.0.0 agrees (0x71005f9390 … 0x71005fbdf0).

use super::{Actor, ENEMY_GRAVITY, ITEM_SPEED, Kind, MAX_FALL, Mode, powerups};
use crate::level::{Level, PipeDir};

/// DokanOut: 0.5 px/f sideways (vf 0x1d0), 1.0 up or down (vf 0x1d8); out of a side pipe the
/// part starts 20 px inside and 8 px (14 when taller than 16) below the mouth line (vf 0x1f0)
/// (enter 0x71010615e0, move 0x7101061370) [obs].
const SIDE_SPEED: f32 = 0.5;
const VERT_SPEED: f32 = 1.0;
const SIDE_START: f32 = 20.0;
/// A parachute (0x71010615e0, 0x7101060be0, end 0x710105f3d0): 2.5 px/f from 10 px inside a
/// side pipe, ±4.0 out of a vertical one, no gravity until clear of the mouth; then 18 frames
/// (10 out of a down pipe) of falling before it opens.
const CHUTE_SIDE: f32 = 2.5;
const CHUTE_SIDE_START: f32 = 10.0;
const CHUTE_VY: f32 = 4.0;
const CHUTE_FRAMES: i32 = 18;
const CHUTE_FRAMES_DOWN: i32 = 10;
/// Winged RedPataPata (0x7101062490 / 0x7101062600 / 0x7101062760): 1 px/f, no gravity;
/// collision once its centre is 16 px past the mouth; done 23 px (side) / 78 px (vertical,
/// 0x71023188fc) out, or on touching anything.
const PATA_SPEED: f32 = 1.0;
const PATA_CLEAR: f32 = 16.0;
const PATA_SIDE_END: f32 = 23.0;
const PATA_VERT_END: f32 = 78.0;
/// Winged PataTogezo family leave a side pipe at 0.5 px/f (exit 0x710105f4e0).
const PT_SIDE_SPEED: f32 = 0.5;
/// Bubble (0x710109e500 / 0x710109e830): launched at the mouth, vy 6.0 up, 2.0 sideways.
const BUBBLE_VY: f32 = 6.0;
const BUBBLE_SIDE: f32 = 2.0;
/// DokanOutBig (0x710105f580 / 0x710105f820): from 11.5 px inside a side pipe, 23 below an up
/// pipe's mouth, 1 above a down pipe's; 0.5 px/f to the mouth (side), 16 below it (up), 8 below
/// it (down; vf 0x1e0, 0x71010609a0).
const BIG_SPEED: f32 = 0.5;
const BIG_SIDE_START: f32 = 11.5;
const BIG_UP_START: f32 = 23.0;
const BIG_UP_STOP: f32 = 16.0;
const BIG_DOWN_STOP: f32 = 8.0;
/// DokanJamBig (0x710105fb00 / 0x710105fc00): 60 frames stuck, shaking ±0.5 px across the
/// pipe on odd frames (not in 3D World).
const JAM_FRAMES: u16 = 60;
const JAM_SHAKE: f32 = 0.5;
/// DokanJumpOutBig (0x710105fec0 / 0x7101060060 / 0x71010603d0): vy 3.0 up, −2.0 down with
/// gravity; sideways 2.0 easing to 0 by 0.03 with gravity −0.01, full gravity once clear; ends
/// at |speed| ≤ 0.5 or a wall (side), on landing (vertical).
const JUMP_UP: f32 = 3.0;
const JUMP_DOWN: f32 = -2.0;
const JUMP_SIDE: f32 = 2.0;
const JUMP_SIDE_ACCEL: f32 = 0.03;
const JUMP_SIDE_GRAVITY: f32 = -0.01;
const JUMP_SIDE_END: f32 = 0.5;

/// A coin out of a pipe (Coin Dokan enter 0x7101367800, launch vf 0x230 0x7101367d10): out of
/// a side pipe 2 px past the mouth with the blaster coin's tables (shot.rs) × 0.6; out of an up
/// / down pipe from the slide's start with its own tables by slot & 7 (0x71023cee60,
/// 0x71023cee20 × 0.6); terrain collision after 5 frames (vf 0x238 0x7101367f20) [obs].
const COIN_SIDE_START: f32 = 2.0;
const COIN_UP_VY: [f32; 8] = [3.5, 3.75, 4.0, 3.0, 3.15, 3.25, 3.65, 3.8];
const COIN_DOWN_VY: [f32; 8] = [-2.5, -2.75, -3.0, -2.0, -2.15, -2.25, -2.65, -2.8];
const COIN_UP_VX: [f32; 8] = [0.5, -0.5, 1.0, -1.0, 0.75, -0.75, 0.25, -0.25];
const COIN_DOWN_VX: [f32; 8] = [0.625, -0.625, 1.125, -1.125, 0.875, -0.875, 0.125, -0.125];
const COIN_VX_SCALE: f32 = 0.6;
const COIN_HOLD: i32 = 5;

/// Where a part is in the Dokan context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dokan {
    /// DokanOut: sliding to the target.
    Slide,
    /// A parachute's shot out: frames left of its fall, and whether it is clear of the mouth.
    Chute {
        left: i32,
        clear: bool,
    },
    /// A winged RedPataPata flying out.
    Pata {
        clear: bool,
    },
    /// DokanOutBig, then DokanJamBig (`t` frames left), then DokanJumpOutBig.
    OutBig,
    JamBig {
        t: u16,
    },
    JumpOutBig {
        clear: bool,
    },
}

/// The winged parts whose Dokan exit is PataTogezo's (Bombhei, Togezo, Togemet, Arihei).
fn pata_togezo(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::BobOmb
            | Kind::Buzzy
            | Kind::Spiny
            | Kind::SpikeTop { .. }
            | Kind::AntTrooper { .. }
            | Kind::LinkBomb
    )
}

/// Out of the pipe's mouth: the direction's unit vector (s for side pipes).
fn out_dir(dir: PipeDir) -> (f32, f32) {
    match dir {
        PipeDir::Right => (1.0, 0.0),
        PipeDir::Left => (-1.0, 0.0),
        PipeDir::Up => (0.0, 1.0),
        PipeDir::Down => (0.0, -1.0),
    }
}

impl Actor {
    /// Enter the Dokan context at the mouth (px, py) of a pipe facing `dir` (enter
    /// 0x7101062020, DokanOut 0x71010615e0) [obs]. `self.wings`, `big` and `chute` must be set;
    /// `player_left`: the nearest player is left of the mouth (vertical pipes face it).
    pub fn dokan_out(&mut self, dir: PipeDir, px: f32, py: f32, player_left: bool) {
        self.pipe = Some((dir, px, py));
        self.vx = 0.0;
        self.vy = 0.0;
        self.on_ground = false;
        let (sx, sy) = out_dir(dir);
        let h = self.height();
        let side_y = py - if h > 16.0 { 14.0 } else { 8.0 };
        self.dir = if self.kind.is_item() {
            // items face right, left only out of a left pipe; winged ones left unless the
            // pipe faces right (0x7101342030) [obs]
            if self.wings {
                if dir == PipeDir::Right { 1 } else { -1 }
            } else if dir == PipeDir::Left {
                -1
            } else {
                1
            }
        } else if sx != 0.0 {
            sx as i32
        } else if player_left {
            -1
        } else {
            1
        };
        // big parts squeeze out (items never go big, 0x7101341e60)
        let state = if self.big && !self.kind.is_item() && !self.chute {
            (self.x, self.y) = match dir {
                PipeDir::Up => (px, py - BIG_UP_START),
                PipeDir::Down => (px, py + 1.0),
                _ => (px - BIG_SIDE_START * sx, side_y),
            };
            Dokan::OutBig
        } else if self.chute {
            (self.x, self.y) = match dir {
                PipeDir::Up => (px, py - h),
                PipeDir::Down => (px, py + 1.0),
                _ => (px - CHUTE_SIDE_START * sx, side_y),
            };
            self.vx = CHUTE_SIDE * sx;
            self.vy = CHUTE_VY * sy;
            Dokan::Chute {
                left: if dir == PipeDir::Down {
                    CHUTE_FRAMES_DOWN
                } else {
                    CHUTE_FRAMES
                },
                clear: false,
            }
        } else {
            (self.x, self.y) = match dir {
                PipeDir::Up => (px, py - h),
                PipeDir::Down => (px, py + 1.0),
                _ => (px - SIDE_START * sx, side_y),
            };
            if self.wings && matches!(self.kind, Kind::Koopa { red: true } | Kind::DryBones) {
                self.vx = PATA_SPEED * sx;
                self.vy = PATA_SPEED * sy;
                Dokan::Pata { clear: false }
            } else {
                Dokan::Slide
            }
        };
        // the slide's target: the mouth, plus the body's half width out of a side pipe
        let (tx, ty) = match dir {
            PipeDir::Up => (px, py),
            PipeDir::Down => (px, py - h),
            _ => (px + (self.half_w() - 0.1) * sx, side_y),
        };
        self.mode = Mode::DokanOut { tx, ty, state };
    }

    /// A coin of a pipe's TenCoin burst (slot 0–9) leaves the mouth: it bounces like a shot
    /// coin from here on and never leaves that state (exe 0x71013679c0, bounce 0x71013a23b0).
    pub fn coin_dokan(&mut self, slot: usize, dir: PipeDir, px: f32, py: f32) {
        let (sx, _) = out_dir(dir);
        let i = slot % 8;
        (self.x, self.y) = match dir {
            PipeDir::Up => (px, py - self.height()),
            PipeDir::Down => (px, py + 1.0),
            _ => (px + COIN_SIDE_START * sx, py - 8.0),
        };
        (self.vx, self.vy) = match dir {
            PipeDir::Up => (COIN_UP_VX[i] * COIN_VX_SCALE, COIN_UP_VY[i]),
            PipeDir::Down => (COIN_DOWN_VX[i] * COIN_VX_SCALE, COIN_DOWN_VY[i]),
            _ => {
                let (vx, vy) = super::shot::coin_slot(slot);
                (vx * COIN_VX_SCALE * sx, vy)
            }
        };
        let creep = if sx != 0.0 { sx } else { self.vx.signum() };
        self.speed = super::shot::COIN_CREEP * creep;
        self.dir = if creep < 0.0 { -1 } else { 1 };
        self.hops = 0;
        self.timer = 0;
        // the frame's own decrement comes first: 5 frames without terrain
        self.turn_lock = COIN_HOLD + 1;
        self.mode = Mode::Shot {
            x0: self.x,
            y0: self.y,
            t: 0,
            fast: false,
            landed: false,
            wet: false,
        };
    }

    /// One frame of the Dokan context.
    pub(super) fn step_dokan(&mut self, level: &Level) {
        let Mode::DokanOut { tx, ty, state } = self.mode else {
            return;
        };
        let Some((dir, px, py)) = self.pipe else {
            self.mode = Mode::Normal;
            return;
        };
        let (sx, sy) = out_dir(dir);
        let h = self.height();
        // past the mouth: the body's trailing edge (side), feet (up) or head (down)
        let clear_of_mouth = |a: &Actor| match dir {
            PipeDir::Up => a.y > py,
            PipeDir::Down => a.y + h < py,
            _ => (a.x - a.half_w() * sx - px) * sx > 0.0,
        };
        let next = match state {
            Dokan::Slide => {
                let speed = if sx != 0.0 { SIDE_SPEED } else { VERT_SPEED };
                let step = |v: f32, t: f32| {
                    if (t - v).abs() <= speed {
                        t
                    } else {
                        v + speed * (t - v).signum()
                    }
                };
                self.x = step(self.x, tx);
                self.y = step(self.y, ty);
                if self.x == tx && self.y == ty {
                    return self.dokan_exit(dir);
                }
                state
            }
            Dokan::Chute { mut left, clear } => {
                let clear = clear || clear_of_mouth(self);
                if clear {
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                    if self.vy <= 0.0 && left > 0 {
                        left -= 1;
                    }
                    if self.on_ground || left < 1 || (sx != 0.0 && self.wall) {
                        return self.end_dokan();
                    }
                } else {
                    self.x += self.vx;
                    self.y += self.vy;
                }
                Dokan::Chute { left, clear }
            }
            Dokan::Pata { clear } => {
                let centre = (self.x - px, self.y + h / 2.0 - py);
                let out = centre.0 * sx + centre.1 * sy;
                let clear = clear || out >= PATA_CLEAR;
                if clear {
                    let was = (self.vx, self.vy);
                    self.collide(level);
                    let touched = self.on_ground || self.wall || (was.1 > 0.0 && self.vy == 0.0);
                    let end = if sx != 0.0 {
                        PATA_SIDE_END
                    } else {
                        PATA_VERT_END
                    };
                    if touched || out >= end {
                        return self.end_dokan();
                    }
                    (self.vx, self.vy) = was;
                } else {
                    self.x += self.vx;
                    self.y += self.vy;
                }
                Dokan::Pata { clear }
            }
            Dokan::OutBig => {
                let (bx, by) = match dir {
                    PipeDir::Up => (px, py - BIG_UP_STOP),
                    PipeDir::Down => (px, py - BIG_DOWN_STOP),
                    _ => (px, self.y),
                };
                let step = |v: f32, t: f32| {
                    if (t - v).abs() <= BIG_SPEED {
                        t
                    } else {
                        v + BIG_SPEED * (t - v).signum()
                    }
                };
                self.x = step(self.x, bx);
                self.y = step(self.y, by);
                if self.x == bx && self.y == by {
                    self.home_x = self.x;
                    self.home_y = self.y;
                    Dokan::JamBig { t: JAM_FRAMES }
                } else {
                    state
                }
            }
            Dokan::JamBig { t } => {
                // shakes across the pipe on odd frames, then jumps out
                let shake = if t % 2 == 1 && level.style != "3W" {
                    JAM_SHAKE * if (t / 2) % 2 == 0 { 1.0 } else { -1.0 }
                } else {
                    0.0
                };
                self.x = self.home_x + shake * sy.abs();
                self.y = self.home_y + shake * sx.abs();
                let t = t.saturating_sub(1);
                if t == 0 {
                    (self.x, self.y) = (self.home_x, self.home_y);
                    if sx != 0.0 {
                        self.vx = JUMP_SIDE * sx;
                        self.vy = 0.0;
                    } else {
                        self.vy = if dir == PipeDir::Up {
                            JUMP_UP
                        } else {
                            JUMP_DOWN
                        };
                    }
                    Dokan::JumpOutBig { clear: false }
                } else {
                    Dokan::JamBig { t }
                }
            }
            Dokan::JumpOutBig { clear } => {
                let clear = clear || clear_of_mouth(self);
                if sx != 0.0 {
                    let s = (self.vx.abs() - JUMP_SIDE_ACCEL).max(0.0);
                    self.vx = s * sx;
                }
                let g = if sx != 0.0 && !clear {
                    JUMP_SIDE_GRAVITY
                } else {
                    ENEMY_GRAVITY
                };
                self.vy = (self.vy + g).max(MAX_FALL);
                if clear {
                    self.collide(level);
                } else {
                    self.x += self.vx;
                    self.y += self.vy;
                }
                // a Chain Chomp's context ends as soon as it is on the ground, in any state
                // (Wanwan's Dokan, vtable 0x7102915ff0: slot 6 0x71012ae720) [obs v3.0.3]
                let chomp_down = matches!(self.kind, Kind::ChainChomp { .. }) && self.on_ground;
                let done = if sx != 0.0 {
                    self.vx.abs() <= JUMP_SIDE_END || self.wall || chomp_down
                } else {
                    clear && self.on_ground
                };
                if done {
                    return self.end_dokan();
                }
                Dokan::JumpOutBig { clear }
            }
        };
        self.mode = Mode::DokanOut {
            tx,
            ty,
            state: next,
        };
    }

    /// The slide's end (exit 0x7101061050 and the presets'): walking items set off, a down
    /// pipe's part leaves at vy −1, a winged PataTogezo hovers (0.5 px/f out of a side pipe),
    /// a Lava Bubble is launched [obs].
    fn dokan_exit(&mut self, dir: PipeDir) {
        let (sx, _) = out_dir(dir);
        self.vx = match self.kind {
            Kind::Mushroom | Kind::OneUp | Kind::Star => ITEM_SPEED * self.dir as f32,
            Kind::PowerItem { what } if powerups::walks(what, 0) => ITEM_SPEED * self.dir as f32,
            _ => 0.0,
        };
        self.vy = if dir == PipeDir::Down { -1.0 } else { 0.0 };
        if self.wings && pata_togezo(self.kind) {
            self.vy = 0.0;
            self.vx = PT_SIDE_SPEED * sx;
        }
        if self.kind == Kind::LavaBubble {
            // launched from the mouth, then its own fall (phase 3: no terrain) [obs; how a
            // wingless one's context ends: ?]
            self.vx = BUBBLE_SIDE * sx;
            self.vy = if dir == PipeDir::Up { BUBBLE_VY } else { 0.0 };
            self.phase = 3;
        }
        let keep = (self.vx, self.vy);
        self.end_dokan();
        if !self.wings || pata_togezo(self.kind) {
            (self.vx, self.vy) = keep;
        }
        // a Spiny or Buzzy Beetle out of a down pipe at normal size walks the ceiling
        // (Togezo's Basic start 0x71012825c0: previous context Dokan, direction 3, not big; its
        // DokanOut enter 0x71012806a0 already turns it over) [obs v3.0.3]
        if dir == PipeDir::Down
            && Actor::ceiling_walker(self.kind)
            && !self.big
            && !self.wings
            && self.mode == Mode::Normal
        {
            self.vy = 0.0;
            self.enter_ceiling();
        }
    }

    fn end_dokan(&mut self) {
        self.pipe = None;
        self.end_shot();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::TILE;

    /// Flat ground, its top at y 32.
    fn floor() -> Level {
        Level::flat(100)
    }
    const PX: f32 = 20.0 * TILE;
    const PY: f32 = 2.0 * TILE;

    fn run(a: &mut Actor, lvl: &Level) -> usize {
        let mut n = 0;
        while matches!(a.mode, Mode::DokanOut { .. }) && n < 600 {
            a.step(lvl, 0.0);
            n += 1;
        }
        n
    }

    #[test]
    fn a_goomba_slides_out_of_an_up_pipe_at_1_px() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Goomba, 0.0, 0.0);
        a.dokan_out(PipeDir::Up, PX, PY, true);
        assert_eq!((a.y, a.dir), (PY - 16.0, -1));
        assert_eq!(run(&mut a, &lvl), 16);
        assert_eq!((a.y, a.mode), (PY, Mode::Normal));
    }

    #[test]
    fn an_item_faces_right_unless_the_pipe_faces_left() {
        let mut a = Actor::new(Kind::Mushroom, 0.0, 0.0);
        a.dokan_out(PipeDir::Up, PX, PY, true);
        assert_eq!(a.dir, 1);
        a.dokan_out(PipeDir::Left, PX, PY, false);
        assert_eq!(a.dir, -1);
    }

    #[test]
    fn a_parachute_is_shot_out_and_opens_after_18_frames_of_falling() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Goomba, 0.0, 0.0);
        a.chute = true;
        a.dokan_out(PipeDir::Right, PX, 6.0 * TILE, false);
        assert_eq!((a.x, a.vx), (PX - 10.0, 2.5));
        let n = run(&mut a, &lvl);
        // 10 + 8 px to clear the mouth at 2.5, then 18 frames of falling
        assert!((24..28).contains(&n), "{n}");
        assert!(matches!(a.mode, Mode::Parachute { .. }));
        assert_eq!(a.vy, 0.3);
    }

    #[test]
    fn a_winged_red_koopa_flies_78_px_out_of_an_up_pipe() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Koopa { red: true }, 0.0, 0.0);
        a.wings = true;
        a.dokan_out(PipeDir::Up, PX, PY, false);
        run(&mut a, &lvl);
        let out = a.y + 8.0 - PY;
        assert!((78.0..80.0).contains(&out), "{out}");
        assert!(a.wings && a.mode == Mode::Normal);
    }

    #[test]
    fn a_winged_bob_omb_leaves_a_down_pipe_hovering() {
        let lvl = floor();
        let mut a = Actor::new(Kind::BobOmb, 0.0, 0.0);
        a.wings = true;
        a.dokan_out(PipeDir::Down, PX, 8.0 * TILE, false);
        run(&mut a, &lvl);
        assert_eq!((a.vy, a.mode), (0.0, Mode::Normal));
        // a wingless one drops out at −1
        let mut b = Actor::new(Kind::BobOmb, 0.0, 0.0);
        b.dokan_out(PipeDir::Down, PX, 8.0 * TILE, false);
        run(&mut b, &lvl);
        assert_eq!(b.vy, -1.0);
    }

    #[test]
    fn a_big_goomba_squeezes_jams_and_jumps_out() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Goomba, 0.0, 0.0);
        a.big = true;
        a.dokan_out(PipeDir::Up, PX, PY, false);
        assert_eq!(a.y, PY - 23.0);
        let mut jammed = 0;
        let mut n = 0;
        while matches!(a.mode, Mode::DokanOut { .. }) && n < 400 {
            a.step(&lvl, 0.0);
            if matches!(
                a.mode,
                Mode::DokanOut {
                    state: Dokan::JamBig { .. },
                    ..
                }
            ) {
                jammed += 1;
            }
            n += 1;
        }
        assert_eq!(jammed, 60, "60 frames stuck");
        assert_eq!((a.mode, a.y), (Mode::Normal, PY));
    }

    #[test]
    fn a_big_chain_chomp_stops_jumping_out_on_landing() {
        let lvl = floor();
        let mut chomp = Actor::new(Kind::ChainChomp { chained: false }, 0.0, 0.0);
        chomp.big = true;
        // a side pipe whose mouth line is 16 px over the floor
        chomp.dokan_out(PipeDir::Right, PX, PY + 32.0, false);
        let mut n = 0;
        while matches!(chomp.mode, Mode::DokanOut { .. }) && n < 400 {
            chomp.step(&lvl, 0.0);
            n += 1;
        }
        assert_eq!(chomp.mode, Mode::Normal);
        assert!(chomp.on_ground);
        assert!(chomp.vx.abs() > JUMP_SIDE_END, "still moving: {}", chomp.vx);
    }

    #[test]
    fn a_pipe_coin_is_launched_by_its_slot_and_bounces() {
        let lvl = floor();
        let mut c = Actor::new(Kind::FloatCoin, 0.0, 0.0);
        c.coin_dokan(2, PipeDir::Up, PX, PY);
        assert_eq!((c.y, c.vy), (PY - 16.0, 4.0));
        assert!((c.vx - 0.6).abs() < 1e-6);
        let mut s = Actor::new(Kind::FloatCoin, 0.0, 0.0);
        s.coin_dokan(0, PipeDir::Left, PX, 4.0 * TILE);
        assert_eq!((s.x, s.vy), (PX - 2.0, 1.5));
        assert!((s.vx + 2.5 * 0.6).abs() < 1e-6);
        // it rises through the pipe (no terrain for 5 frames) and ends up creeping on the floor
        for _ in 0..300 {
            c.step(&lvl, 0.0);
        }
        assert!(c.on_ground && matches!(c.mode, Mode::Shot { .. }));
        assert_eq!(c.vx, 0.34);
    }

    #[test]
    fn a_lava_bubble_is_launched_from_the_mouth() {
        let lvl = floor();
        let mut a = Actor::new(Kind::LavaBubble, 0.0, 0.0);
        a.dokan_out(PipeDir::Up, PX, PY, false);
        run(&mut a, &lvl);
        assert_eq!((a.vy, a.phase), (6.0, 3));
    }
}
