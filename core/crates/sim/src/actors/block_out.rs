//! What a block releases, actor side: the EnemySys **Block** context (shared enemy draft §5.4,
//! 4-brick.md B9). A part that is not a simple item (those rise out in `Mode::Emerge`) enters
//! its EnemyDB `action_block` preset's `cState_Out`: placed in the block and pushed out, with
//! its terrain collision off for the first frames, until the preset's end condition hands it
//! over through the shared return-to-normal rule (`end_shot`: Parachute, Wing, else Basic).
//! The world side (which blocks release what, and when) is in world.rs `release`.
//!
//! Positions follow the game's spawn: the actor starts at the block's origin (its bottom
//! centre) and the context keeps Yb = origin + 8, the block's centre [obs v3.0.3 context enter
//! 0x7101049100]. Values are [obs v3.0.3] (Default vtable 0x71028b2190); v1.0.0 agrees
//! (0x71005e62a0 … 0x71005e6d20) except where noted.

use super::{Actor, ENEMY_GRAVITY, Kind, MAX_FALL, Mode};
use crate::fmath::DetMath;
use crate::level::Level;

/// The Default launch (velocity vf 0x170 0x7101049d60): vy 2.5, or 1.8 winged, from the block
/// centre; gravity −0.1875, −0.07 winged. Out of a note block (origin class 0x20000000) vy
/// gains 2.0 and a winged part falls at −0.1.
const OUT_VY: f32 = 2.5;
const WING_VY: f32 = 1.8;
const WING_GRAVITY: f32 = -0.07;
const NOTE_VY: f32 = 2.0;
const NOTE_WING_GRAVITY: f32 = -0.1;
/// Downward (block hit from above): vy 0 (big −0.7, note block −2.0), gravity −0.125 winged.
const DOWN_BIG_VY: f32 = -0.7;
const DOWN_NOTE_VY: f32 = -2.0;
const DOWN_WING_GRAVITY: f32 = -0.125;
/// Frames with terrain collision off: 6 (vf 0x188 0x7101049f40), 10 out of a note block or
/// downward (Out enter 0x710104a190).
const CLEAR_FRAMES: i32 = 6;
const CLEAR_FRAMES_LONG: i32 = 10;
/// A parachute: shot out at ±4.0 with gravity, opening 18 frames (10 downward) after it stops
/// rising (Out enter 0x710104a190, exe 0x710104a090).
const CHUTE_VY: f32 = 4.0;
const CHUTE_FRAMES: i32 = 18;
const CHUTE_FRAMES_DOWN: i32 = 10;
/// Coming up, a part's head starts at least 4 px above the block centre.
const HEAD_CLEAR: f32 = 4.0;

/// RedPataPata winged (Out 0x710104aee0 / 0x710104afc0): no gravity, vy ±2.0 (±0.8 more out of a
/// note block) easing toward ±0.5 by a step that depends on |vy| (thresholds 0.3 / 0.7 / 1.2,
/// steps 0.006 / 0.009 / 0.012 / 0.024); ends 74 px from where it started, or on touching a
/// ceiling (up) or the ground (down).
const PATA_VY: f32 = 2.0;
const PATA_NOTE_VY: f32 = 0.8;
const PATA_GLIDE: f32 = 0.5;
const PATA_RANGE: f32 = 74.0;
/// PataTogezo winged out of a downward block (Out 0x7101049540 / 0x71010496a0): vy −1.1 with
/// the clear time cut to 2 frames (4 big), then gravity +0.03 until it stops falling.
const PT_DOWN_VY: f32 = -1.1;
const PT_DOWN_LIFT: f32 = 0.03;
/// FloatSearch (Out 0x710104b430 / 0x710104b560): vy 1.2 (2.5 winged) with gravity −0.04
/// (−0.1 winged), +1.0 out of a note block (mirrored downward; big −0.7 down); ends when its
/// speed is down to 0.2 [v3 table 0x7102319cb0].
const FLOAT_VY: [f32; 2] = [1.2, 2.5];
const FLOAT_GRAVITY: [f32; 2] = [-0.04, -0.1];
const FLOAT_NOTE: f32 = 1.0;
const FLOAT_STOP: f32 = 0.2;
/// Pukupuku (Out 0x7101228fa0 / 0x71012290d0): the Default pop with the clear time ×4, then
/// it hands over whether or not it has landed.
const CHEEP_CLEAR: i32 = 4;
/// Bubble (Out 0x710109c4c0 / 0x710109c970): vy 6.0 (6.8 out of a note block), Default
/// gravity; hands over at the top of its leap [obs; in liquid 3.5 / 5.0, not modelled].
const BUBBLE_VY: f32 = 6.0;
const BUBBLE_NOTE_VY: f32 = 6.8;
/// MapObjDefault (Out 0x7101370680, velocity 0x7101370a80, end 0x71013709d0): vy 3.0 (4.0
/// winged out of a note block), −1.0 downward (winged −4.0 without gravity); a winged object
/// ends at vy ≤ −2 (downward: once vy ≥ 0), others on landing.
const MAPOBJ_VY: f32 = 3.0;
const MAPOBJ_NOTE_WING_VY: f32 = 4.0;
const MAPOBJ_DOWN_VY: f32 = -1.0;
const MAPOBJ_DOWN_WING_VY: f32 = -4.0;
const MAPOBJ_WING_END: f32 = -2.0;

/// The `action_block` presets (EnemyDB) that differ in what this engine models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Default and the presets that keep its Out (Kuribo out of water, NokonokoGreen, Pakkun,
    /// Bros, Kameck, Wanwan, KoopaClown, …): a hop out of the block, done on landing.
    Default,
    /// RedPataPata (red Koopa, Dry Bones, Gabon): winged, a slowing climb of 74 px.
    RedPataPata,
    /// PataTogezo and its kin Togezo / Togemet / MechaKoopaBase (Bob-omb, Buzzy Beetle, Spiny,
    /// Spike Top, Ant Trooper, Mechakoopa): winged, done at the top of the hop.
    PataTogezo,
    /// FloatSearch (Boo, Blooper, Stingby): a slowing float, done when nearly still.
    FloatSearch,
    /// Pukupuku (Cheep Cheep): faces the player, hands over when the clear time ends.
    Pukupuku,
    /// Bubble (Lava Bubble): a high leap, handed over at its top.
    Bubble,
    /// MapObjDefault / MapObjYoshiEgg (P Switch, POW, springs, Stone, Yoshi's egg, …).
    MapObj,
}

/// A part's `action_block` preset (enemydb.tsv; v3 Out slots 37–39 in vtables 0x71028b2350
/// PataTogezo, 0x71028b2570 RedPataPata, 0x71028b2730 FloatSearch, 0x71028bf270 Bubble,
/// 0x7102905608 Pukupuku, 0x7102933d48 MapObjDefault) [obs]. Only winged RedPataPata /
/// PataTogezo parts use their own Out; wingless ones take Default's [obs].
pub fn preset(kind: Kind, wings: bool) -> Preset {
    match kind {
        Kind::Koopa { red: true } | Kind::DryBones if wings => Preset::RedPataPata,
        Kind::BobOmb
        | Kind::Buzzy
        | Kind::Spiny
        | Kind::SpikeTop { .. }
        | Kind::AntTrooper { .. }
        | Kind::Mechakoopa { .. }
        | Kind::LinkBomb
            if wings =>
        {
            Preset::PataTogezo
        }
        // the 3D World Blooper has its own Out (Gesso3W 0x71011037a0) [?: not traced]
        Kind::Boo | Kind::Blooper | Kind::Stingby => Preset::FloatSearch,
        Kind::CheepCheep { .. } => Preset::Pukupuku,
        Kind::LavaBubble => Preset::Bubble,
        Kind::PSwitch
        | Kind::Pow
        | Kind::Spring
        | Kind::SideSpring
        | Kind::Stone
        | Kind::Crate { .. }
        | Kind::YoshiEgg { .. } => Preset::MapObj,
        _ => Preset::Default,
    }
}

impl Actor {
    /// Enter the Block context out of the block whose origin (bottom centre) is (x, `by`):
    /// the Out enter (0x710104a190 and the preset's) [obs]. `down`: the block was hit from
    /// above; `note`: a note block's (or a tree's) bounce-out. `self.wings`, `big` and `chute`
    /// (a parachute) must be set.
    pub fn block_out(&mut self, by: f32, down: bool, note: bool) {
        let yb = by + 8.0;
        let h = self.height();
        let winged = self.wings && !self.chute;
        let preset = preset(self.kind, self.wings);
        self.vx = 0.0;
        self.y = match (down, note) {
            (false, false) => yb + (HEAD_CLEAR - (h - 8.0)).max(0.0),
            (false, true) => yb + 8.0,
            (true, false) => (yb - h).max(yb - 16.0 * (h / 16.0).ceil()),
            (true, true) => yb - 8.0 - h,
        };
        let sign = if down { -1.0 } else { 1.0 };
        if self.kind.is_item() {
            // an item (only a parachute one comes here) starts 8 px further in (vf 0x1d0
            // 0x7101340fe0) [obs]
            self.y -= 8.0 * sign;
        }
        let mut gravity = ENEMY_GRAVITY;
        let mut left = 0;
        if self.chute {
            self.vy = CHUTE_VY * sign;
            left = if down {
                CHUTE_FRAMES_DOWN
            } else {
                CHUTE_FRAMES
            };
        } else if !down {
            self.vy = if winged { WING_VY } else { OUT_VY };
            if winged {
                gravity = WING_GRAVITY;
            }
            if note {
                self.vy += NOTE_VY;
                if winged {
                    gravity = NOTE_WING_GRAVITY;
                }
            }
        } else {
            self.vy = 0.0;
            if winged {
                gravity = DOWN_WING_GRAVITY;
            }
            if self.big {
                self.vy += DOWN_BIG_VY;
            }
            if note {
                self.vy += DOWN_NOTE_VY;
            }
        }
        self.timer = if note || down {
            CLEAR_FRAMES_LONG
        } else {
            CLEAR_FRAMES
        };
        // the presets' own launches (none applies to a parachute)
        if !self.chute {
            match preset {
                Preset::RedPataPata => {
                    gravity = 0.0;
                    self.vy = (PATA_VY + if note { PATA_NOTE_VY } else { 0.0 }) * sign;
                }
                Preset::PataTogezo if down => {
                    self.vy = PT_DOWN_VY;
                    self.timer = if self.big { 4 } else { 2 };
                }
                Preset::FloatSearch => {
                    let w = winged as usize;
                    self.vy = FLOAT_VY[w] * sign;
                    gravity = FLOAT_GRAVITY[w] * sign;
                    if note {
                        self.vy += FLOAT_NOTE * sign;
                    } else if down && self.big {
                        self.vy += DOWN_BIG_VY;
                    }
                }
                Preset::Pukupuku => self.timer *= CHEEP_CLEAR,
                Preset::Bubble if !down => {
                    self.vy = if note { BUBBLE_NOTE_VY } else { BUBBLE_VY };
                }
                Preset::MapObj => {
                    self.vy = match (down, winged) {
                        (false, true) if note => MAPOBJ_NOTE_WING_VY,
                        (false, _) => MAPOBJ_VY,
                        (true, true) => {
                            gravity = 0.0;
                            MAPOBJ_DOWN_WING_VY
                        }
                        (true, false) => MAPOBJ_DOWN_VY,
                    };
                }
                _ => {}
            }
        }
        self.on_ground = false;
        self.mode = Mode::BlockOut {
            yb,
            y0: self.y,
            gravity,
            down,
            left,
        };
    }

    /// Past the clear time: terrain collision and contact are back (vf 0x160 0x7101049bb0,
    /// collision flags 0x1000000 / 0x2000000 / 0x4000000 at +0x524) [obs].
    pub(super) fn block_clear(&self) -> bool {
        matches!(self.mode, Mode::BlockOut { .. }) && self.timer <= 0
    }

    /// Still inside the block it came out of (drawn behind it): its feet below the block top
    /// (up), its head above the block bottom (down) (z swap vf 0x180) [obs].
    pub fn in_block(&self) -> bool {
        match self.mode {
            Mode::BlockOut { yb, down, .. } => {
                if down {
                    self.y + self.height() > yb - 8.0
                } else {
                    self.y < yb + 8.0
                }
            }
            _ => false,
        }
    }

    /// One frame of the Out state.
    pub(super) fn step_block_out(&mut self, level: &Level) {
        let Mode::BlockOut {
            yb,
            y0,
            mut gravity,
            down,
            mut left,
        } = self.mode
        else {
            return;
        };
        let winged = self.wings && !self.chute;
        let preset = preset(self.kind, self.wings);
        // FloatSearch eases its speed, RedPataPata its climb (no gravity)
        if preset == Preset::RedPataPata && !self.chute {
            let v = self.vy.abs();
            let step = if v < 0.3 {
                0.006
            } else if v < 0.7 {
                0.009
            } else if v < 1.2 {
                0.012
            } else {
                0.024
            };
            let target = if down { -PATA_GLIDE } else { PATA_GLIDE };
            self.vy = if self.vy > target {
                (self.vy - step).max(target)
            } else {
                (self.vy + step).min(target)
            };
        } else {
            if preset == Preset::PataTogezo && winged && down && self.timer <= 0 {
                gravity = PT_DOWN_LIFT;
            }
            self.vy = (self.vy + gravity).max(MAX_FALL);
        }
        let ceiling_before = self.vy > 0.0;
        if self.timer > 0 {
            self.timer -= 1;
            self.x += self.vx;
            self.y += self.vy;
        } else {
            self.collide(level);
        }
        // the end conditions (the preset's exe and its vf 0x168)
        let done = if self.chute {
            // the countdown runs once it has stopped rising; landing opens it too
            if self.vy <= 0.0 && left > 0 {
                left -= 1;
            }
            self.vy <= 0.0 && (left < 1 || self.on_ground)
        } else {
            match preset {
                Preset::RedPataPata => {
                    (self.y - y0).abs() >= PATA_RANGE
                        || if down {
                            self.on_ground
                        } else {
                            ceiling_before && self.vy == 0.0
                        }
                }
                Preset::PataTogezo if winged => {
                    if down {
                        self.vy >= 0.0
                    } else {
                        self.vy <= 0.0
                    }
                }
                Preset::FloatSearch => self.vy.abs().dhypot(self.vx) <= FLOAT_STOP,
                Preset::Pukupuku => self.timer <= 0 || self.on_ground,
                Preset::Bubble if !down => self.vy <= 0.0,
                Preset::MapObj if winged => {
                    if down {
                        self.vy >= 0.0
                    } else {
                        self.vy <= MAPOBJ_WING_END
                    }
                }
                _ => self.on_ground,
            }
        };
        self.mode = Mode::BlockOut {
            yb,
            y0,
            gravity,
            down,
            left,
        };
        if done {
            if self.kind == Kind::LavaBubble && !self.chute {
                // falls from here into its own cycle
                self.phase = 2;
            }
            self.end_shot();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{Cell, TILE};

    /// Flat ground (top at y 32) and a block at tile (20, 5): bottom 80, top 96.
    fn course() -> Level {
        let mut lvl = Level::flat(100);
        lvl.set(20, 5, Cell::Solid);
        lvl
    }
    const BY: f32 = 5.0 * TILE;
    const BX: f32 = 20.5 * TILE;

    fn out(kind: Kind, wings: bool, down: bool) -> Actor {
        let mut a = Actor::new(kind, BX, BY);
        a.wings = wings;
        a.block_out(BY, down, false);
        a
    }

    fn run_out(a: &mut Actor, lvl: &Level) -> usize {
        let mut n = 0;
        while matches!(a.mode, Mode::BlockOut { .. }) && n < 600 {
            a.step(lvl, 0.0);
            n += 1;
        }
        n
    }

    #[test]
    fn a_goomba_hops_out_of_the_block_centre_and_lands_on_top() {
        let lvl = course();
        let mut a = out(Kind::Goomba, false, false);
        assert_eq!((a.y, a.vy, a.timer), (BY + 8.0, 2.5, 6));
        let mut peak: f32 = 0.0;
        let mut x = vec![];
        while matches!(a.mode, Mode::BlockOut { .. }) {
            a.step(&lvl, 0.0);
            peak = peak.max(a.y);
            x.push(a.x);
        }
        // 2.5 under −0.1875 a frame: about 15 px over the centre, 7 over the block top
        assert!((BY + 22.0..BY + 24.5).contains(&peak), "{peak}");
        assert_eq!(a.y, BY + TILE, "landed on the block");
        assert!(x.iter().all(|&v| v == BX), "straight up, no walking");
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn a_winged_goomba_rises_slowly_and_flies_off_on_landing() {
        let lvl = course();
        let mut a = out(Kind::Goomba, true, false);
        assert_eq!(a.vy, 1.8);
        let n = run_out(&mut a, &lvl);
        // 1.8 under −0.07: about 23 px up and back down onto the block
        assert!((45..60).contains(&n), "{n}");
        assert!(a.wings && a.mode == Mode::Normal);
    }

    #[test]
    fn a_downward_goomba_drops_out_below_the_block() {
        let lvl = course();
        let mut a = out(Kind::Goomba, false, true);
        assert_eq!((a.y, a.vy, a.timer), (BY + 8.0 - 16.0, 0.0, 10));
        run_out(&mut a, &lvl);
        assert_eq!((a.y, a.mode), (2.0 * TILE, Mode::Normal));
    }

    #[test]
    fn a_parachute_is_shot_up_and_opens_18_frames_after_the_top() {
        let lvl = course();
        let mut a = Actor::new(Kind::Goomba, BX, BY);
        a.chute = true;
        a.block_out(BY, false, false);
        assert_eq!(a.vy, 4.0);
        let mut falling = 0;
        while matches!(a.mode, Mode::BlockOut { .. }) {
            a.step(&lvl, 0.0);
            // (the frame it opens pops it up at +0.3)
            if a.vy <= 0.0 || !matches!(a.mode, Mode::BlockOut { .. }) {
                falling += 1;
            }
        }
        assert_eq!(falling, 18);
        assert!(matches!(a.mode, Mode::Parachute { .. }));
        assert_eq!(a.vy, 0.3, "it pops open");
    }

    #[test]
    fn a_winged_red_koopa_climbs_74_px_and_flies() {
        let lvl = course();
        let mut a = out(Kind::Koopa { red: true }, true, false);
        let y0 = a.y;
        assert_eq!(a.vy, 2.0);
        run_out(&mut a, &lvl);
        assert!(a.y - y0 >= 74.0 && a.y - y0 < 76.0, "{}", a.y - y0);
        assert!(a.wings && a.mode == Mode::Normal);
        // wingless it hops like a Goomba
        assert_eq!(out(Kind::Koopa { red: true }, false, false).vy, 2.5);
    }

    #[test]
    fn a_winged_bob_omb_is_done_at_the_top_of_its_hop() {
        let lvl = course();
        let mut a = out(Kind::BobOmb, true, false);
        let n = run_out(&mut a, &lvl);
        // 1.8 / 0.07 ≈ 26 frames
        assert!((25..28).contains(&n), "{n}");
        assert!(a.y > BY + TILE + 10.0, "{}", a.y);
    }

    #[test]
    fn a_boo_floats_out_and_slows_to_a_stop() {
        let lvl = course();
        let mut a = out(Kind::Boo, false, false);
        assert_eq!(a.vy, 1.2);
        let n = run_out(&mut a, &lvl);
        // 1.2 → 0.2 at −0.04 a frame
        assert!((25..=26).contains(&n), "{n}");
        assert!(a.y > BY + 20.0);
    }

    #[test]
    fn a_cheep_cheep_hands_over_when_its_long_clear_time_ends() {
        let lvl = course();
        let mut a = out(Kind::CheepCheep { red: false }, false, false);
        assert_eq!(a.timer, 24);
        let n = run_out(&mut a, &lvl);
        assert!(n <= 24, "{n}");
    }

    #[test]
    fn a_lava_bubble_leaps_and_falls_into_its_cycle() {
        let lvl = course();
        let mut a = out(Kind::LavaBubble, false, false);
        assert_eq!(a.vy, 6.0);
        let n = run_out(&mut a, &lvl);
        assert_eq!(n, 32, "6.0 / 0.1875");
        assert_eq!((a.mode, a.phase), (Mode::Normal, 2));
    }

    #[test]
    fn a_p_switch_pops_at_3() {
        let lvl = course();
        let mut a = out(Kind::PSwitch, false, false);
        assert_eq!(a.vy, 3.0);
        run_out(&mut a, &lvl);
        assert_eq!(a.y, BY + TILE);
        assert_eq!(out(Kind::PSwitch, false, true).vy, -1.0);
    }
}
