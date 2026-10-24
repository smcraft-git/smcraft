//! What a block releases, actor side: the EnemySys **Block** context (shared enemy draft §5.4,
//! §5.9, 4-brick.md B9). A part that is not a simple item (those rise out in `Mode::Emerge`;
//! the P-Balloon and the wearables ease out in their own preset here) enters
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
/// note block) easing toward ±0.5 by 0.06 a frame — in water by a step that depends on |vy|
/// (thresholds 0.3 / 0.7 / 1.2, steps 0.006 / 0.009 / 0.012 / 0.024; v3.0.3: the in-water test
/// 0x71012a9180 or the +0x658 bit-31 liquid contact picks the bands, DAT 0x7102318bbc = 0.06
/// otherwise); ends 74 px from where it started, or on touching a ceiling (up) or the ground
/// (down).
const PATA_VY: f32 = 2.0;
const PATA_NOTE_VY: f32 = 0.8;
const PATA_GLIDE: f32 = 0.5;
const PATA_DRY_STEP: f32 = 0.06;
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
/// FishBone (Out 0x71010d8850 / 0x71010d88d0, v3.0.3): Pukupuku's, a big one out of a
/// downward block starting 16 px lower.
const FISHBONE_BIG_DOWN: f32 = 16.0;
/// Bubble (Out 0x710109c4c0 / 0x710109c970): vy 6.0 (6.8 out of a note block), 3.5 (5.0) in
/// liquid (+0x658 bit 31 or the in-water test 0x71012a9180), Default gravity; hands over at
/// the top of its leap [obs v3.0.3].
const BUBBLE_VY: f32 = 6.0;
const BUBBLE_NOTE_VY: f32 = 6.8;
const BUBBLE_WET_VY: f32 = 3.5;
const BUBBLE_WET_NOTE_VY: f32 = 5.0;
/// Kuribo in water (Block vtable 0x71028eaaf0: Out 0x71011a53a0 / 0x71011a5470, v3.0.3): the
/// Default Out's enter, but its motion never runs — it stays where it starts for 1.5× its clear
/// time (2.5× out of a note block), then hands over [obs].
const KURIBO_WET_HOLD: f32 = 1.5;
const KURIBO_WET_NOTE_HOLD: f32 = 2.5;
/// MapObjDefault (Out 0x7101370680, velocity 0x7101370a80, end 0x71013709d0): vy 3.0 (4.0
/// winged out of a note block), −1.0 downward (winged −4.0 without gravity); a winged object
/// ends at vy ≤ −2 (downward: once vy ≥ 0), others on landing.
const MAPOBJ_VY: f32 = 3.0;
const MAPOBJ_NOTE_WING_VY: f32 = 4.0;
const MAPOBJ_DOWN_VY: f32 = -1.0;
const MAPOBJ_DOWN_WING_VY: f32 = -4.0;
const MAPOBJ_WING_END: f32 = -2.0;
/// ItemBalloon / ItemHeaddress (P-Balloon, the 3D World wearables; vtables 0x7102920d40 /
/// 0x7102923cf0, Out 0x7101339d60 / 0x7101339de0 and 0x71013451a0 / 0x7101345220, velocity vf
/// 0x170 0x710133a050, end vf 0x168 0x7101339ff0) [obs v3.0.3]: no launch (vy, gravity 0); the
/// feet ease to Yb + 7 (down Yb − 24; out of a note block Yb + 39 / Yb − 40) by max(0.05,
/// 0.1 × the distance) a frame, 0.05 × in water, and it hands over on arriving. Contact and
/// terrain stay off for the whole Out (their clear time only runs under a parachute,
/// 0x71013475a0). Unlike ItemDefault it is not moved 8 px back in (vf 0x1d0 0x710133a260 = 0).
const EASE_UP: f32 = 7.0;
const EASE_DOWN: f32 = -24.0;
const EASE_NOTE_UP: f32 = 39.0;
const EASE_NOTE_DOWN: f32 = -40.0;
const EASE_RATE: f32 = 0.1;
const EASE_WET_RATE: f32 = 0.05;
const EASE_MIN_STEP: f32 = 0.05;
/// Arrived: within 2e-5 of the target (DAT 0x7102318820 / 0x7102318cf4 = ∓1e-5).
const EASE_ARRIVED: f32 = 2e-5;
/// Winged: up vy 4.0 under −0.25, done once vy ≤ 0; down vy −0.24 under −0.0625 for 9
/// frames (+0xa9 = 9, vf 0x1e8 0x7101347770).
const EASE_WING_VY: f32 = 4.0;
const EASE_WING_GRAVITY: f32 = -0.25;
const EASE_WING_DOWN_VY: f32 = -0.24;
const EASE_WING_DOWN_GRAVITY: f32 = -0.0625;
const EASE_WING_DOWN_FRAMES: i32 = 9;
/// The headdress is drawn in front once its feet are 2 px under the block top (down: its head
/// 2 px over the block bottom; 0x7101345220).
const HEADDRESS_DRAW: f32 = 2.0;
/// An ItemDefault-family item out of a note block starts 4 px higher (0x7101340330, the note
/// flag +0x98 from 0x71013400d0), where others start 8 px further in.
const ITEM_NOTE_UP: f32 = 4.0;
/// The Coin actor (Block vtable 0x7102932130; Out 0x7101366650 / 0x71013666b0, velocity vf
/// 0x170 0x71013668e0, end vf 0x168 0x71013668b0) [obs v3.0.3]: vy 2.5 under −0.14 (also
/// winged or out of a note block), done at vy ≤ −1; down vy −2.5 with its gravity turned over
/// (+0x530 bit 16), done once vy ≥ 0. No terrain (+0x524 bit 4, kept for a parachute's) and
/// its hit box out of the grid (0x7100e26430 each frame) until its feet are 24 px over the
/// block origin (down: 16 px under it), then the box is entered (0x7100e265e0) and terrain
/// turns on.
const COIN_VY: f32 = 2.5;
const COIN_GRAVITY: f32 = -0.14;
const COIN_END: f32 = -1.0;
const COIN_CLEAR_UP: f32 = 24.0;
const COIN_CLEAR_DOWN: f32 = -16.0;
/// Hanachan (Wiggler) in water (context enter 0x7101112840 sets the water mode +0x540 = 1;
/// Out 0x7101112d00 / 0x7101112de0) [obs v3.0.3]: contact on from the start but no terrain
/// (+0x524 bit 4); down vy −2.5 (−3.375 out of a note block, ×1.5 big) with gravity × −0.7
/// (DAT 0x7102318f80); done once |vy| ≤ 0.2 (DAT 0x7102319384). Out of water: the Default Out,
/// also handing over as soon as it is in water.
const WIGGLER_WET_VY: f32 = -2.5;
const WIGGLER_WET_NOTE_VY: f32 = -3.375;
const WIGGLER_WET_BIG: f32 = 1.5;
const WIGGLER_WET_GRAVITY: f32 = -0.7;
const WIGGLER_WET_STOP: f32 = 0.2;
/// Togezo (Spiny) in water: its ball form, mode +0x540 = 3 (0x710127f300: not winged, not the
/// 0x4 form, in water). Out 0x710127f750 / 0x710127f890 [obs v3.0.3]: down vy −2.4 (−1.2 × 2;
/// a note block's −2.0 × 2) with the gravity turned over; done once |vy| ≤ 0.5, or while
/// faster once its top is out of the water (the surface ≤ its feet + 4). Out of water (mode
/// 0): the Default Out, handing over once clear of the block (or its clear time) in liquid.
const SPINY_WET_DOWN_VY: f32 = -1.2;
const SPINY_WET_STOP: f32 = 0.5;
const SPINY_WET_TOP: f32 = 4.0;

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
    /// Pukupuku (Cheep Cheep, Fish Bone): hands over when the clear time ends (a Cheep Cheep
    /// faces the player first).
    Pukupuku,
    /// Bubble (Lava Bubble): a high leap, handed over at its top.
    Bubble,
    /// MapObjDefault / MapObjYoshiEgg (P Switch, POW, springs, Stone, Yoshi's egg, …).
    MapObj,
    /// ItemBalloon / ItemHeaddress (the P-Balloon, the 3D World wearables): eased out to the
    /// block top (the world picks it for the P-Balloon, which is only SMW's part 81).
    ItemEase,
    /// The Coin actor (a winged or parachuted coin).
    Coin,
    /// Hanachan (Wiggler): its water variant.
    Hanachan,
    /// Togezo (a wingless Spiny): its water ball form.
    Togezo,
}

/// A part's `action_block` preset (enemydb.tsv; v3 Out slots 37–39 in vtables 0x71028b2350
/// PataTogezo, 0x71028b2570 RedPataPata, 0x71028b2730 FloatSearch, 0x71028bf270 Bubble,
/// 0x7102905608 Pukupuku, 0x7102933d48 MapObjDefault, 0x7102923cf0 ItemHeaddress, 0x7102932130
/// Coin, 0x71028d7358 Hanachan, 0x7102913238 Togezo) [obs]. Only winged RedPataPata /
/// PataTogezo parts use their own Out; wingless ones take Default's [obs]. The P-Balloon
/// (ItemBalloon 0x7102920d40) is part 81 in SMW only: the world passes its preset.
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
        // (3D World's Blooper, Gesso3W, collides from the start: world.rs `release`)
        Kind::Boo | Kind::Blooper | Kind::Stingby => Preset::FloatSearch,
        // FishBone's Out (0x71010d8850 / 0x71010d88d0, vtable 0x71028cab38) is Pukupuku's
        Kind::CheepCheep { .. } | Kind::FishBone => Preset::Pukupuku,
        Kind::LavaBubble => Preset::Bubble,
        Kind::PSwitch
        | Kind::Pow
        | Kind::Spring
        | Kind::SideSpring
        | Kind::Stone
        | Kind::Crate { .. }
        | Kind::YoshiEgg { .. } => Preset::MapObj,
        // (the wearables are 3D World's only; the P-Balloon is the world's call)
        Kind::PowerItem { what } if what >= super::WEAR => Preset::ItemEase,
        Kind::FloatCoin => Preset::Coin,
        Kind::Wiggler => Preset::Hanachan,
        Kind::Spiny => Preset::Togezo,
        _ => Preset::Default,
    }
}

impl Actor {
    /// Enter the Block context out of the block whose origin (bottom centre) is (x, `by`):
    /// the Out enter (0x710104a190 and the preset's) [obs]. `down`: the block was hit from
    /// above; `note`: a note block's (or a tree's) bounce-out. `self.wings`, `big` and `chute`
    /// (a parachute) must be set.
    pub fn block_out(&mut self, by: f32, down: bool, note: bool) {
        self.block_out_as(preset(self.kind, self.wings), by, down, note);
    }

    /// `block_out` with the preset given (the world's pick where it depends on the style).
    pub fn block_out_as(&mut self, preset: Preset, by: f32, down: bool, note: bool) {
        let yb = by + 8.0;
        // in water (the world senses the liquid into `w3` first)
        let wet = self.wet();
        let h = self.height();
        let winged = self.wings && !self.chute;
        self.vx = 0.0;
        self.y = match (down, note) {
            (false, false) => yb + (HEAD_CLEAR - (h - 8.0)).max(0.0),
            (false, true) => yb + 8.0,
            (true, false) => (yb - h).max(yb - 16.0 * (h / 16.0).ceil()),
            (true, true) => yb - 8.0 - h,
        };
        let sign = if down { -1.0 } else { 1.0 };
        if self.kind.is_item() {
            // ItemDefault's placement (0x7101340330): out of a note block 4 px higher (up),
            // else 8 px further in (vf 0x1d0 0x7101340fe0; 0 for ItemBalloon / ItemHeaddress)
            // [obs v3.0.3]
            if note {
                if !down {
                    self.y += ITEM_NOTE_UP;
                }
            } else if preset != Preset::ItemEase {
                self.y -= 8.0 * sign;
            }
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
                Preset::Pukupuku => {
                    self.timer *= CHEEP_CLEAR;
                    // a big Fish Bone out of a downward block starts 16 px lower
                    if self.kind == Kind::FishBone && self.big && down {
                        self.y -= FISHBONE_BIG_DOWN;
                    }
                }
                Preset::Bubble if !down => {
                    self.vy = match (wet || self.in_lava(), note) {
                        (false, false) => BUBBLE_VY,
                        (false, true) => BUBBLE_NOTE_VY,
                        (true, false) => BUBBLE_WET_VY,
                        (true, true) => BUBBLE_WET_NOTE_VY,
                    };
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
                Preset::ItemEase => {
                    (self.vy, gravity) = match (winged, down) {
                        (false, _) => (0.0, 0.0),
                        (true, false) => (EASE_WING_VY, EASE_WING_GRAVITY),
                        (true, true) => {
                            left = EASE_WING_DOWN_FRAMES;
                            (EASE_WING_DOWN_VY, EASE_WING_DOWN_GRAVITY)
                        }
                    };
                }
                Preset::Coin => {
                    self.vy = COIN_VY * sign;
                    gravity = COIN_GRAVITY * sign;
                }
                Preset::Hanachan if wet => {
                    // contact from the start (its clear-time flags cleared at once)
                    self.timer = 0;
                    if down {
                        let v = if note {
                            WIGGLER_WET_NOTE_VY
                        } else {
                            WIGGLER_WET_VY
                        };
                        self.vy = v * if self.big { WIGGLER_WET_BIG } else { 1.0 };
                        gravity *= WIGGLER_WET_GRAVITY;
                    }
                }
                Preset::Togezo if wet && !self.alt && down => {
                    let v = if note { self.vy } else { SPINY_WET_DOWN_VY };
                    self.vy = v * 2.0;
                    gravity = -gravity;
                }
                _ => {}
            }
        }
        if wet && !self.chute && matches!(self.kind, Kind::Goomba | Kind::Goombrat) {
            let k = if note {
                KURIBO_WET_NOTE_HOLD
            } else {
                KURIBO_WET_HOLD
            };
            left = (k * self.timer as f32) as i32;
            self.vy = 0.0;
            gravity = 0.0;
        }
        self.on_ground = false;
        self.mode = Mode::BlockOut {
            yb,
            y0: self.y,
            gravity,
            down,
            left,
            wet,
            preset,
            note,
            clear: false,
        };
    }

    /// Past the clear time: terrain collision and contact are back (vf 0x160 0x7101049bb0,
    /// collision flags 0x1000000 / 0x2000000 / 0x4000000 at +0x524) [obs].
    pub(super) fn block_clear(&self) -> bool {
        matches!(self.mode, Mode::BlockOut { .. }) && self.timer <= 0
    }

    /// Still inside the block it came out of (drawn behind it): its feet below the block top
    /// (up), its head above the block bottom (down) (z swap vf 0x180) [obs]. The P-Balloon
    /// keeps the block's depth for its whole Out (its depth changes in the exit, vf 0x1a8
    /// 0x710104a050); a wearable changes once 2 px out (0x7101345220) [obs v3.0.3].
    pub fn in_block(&self) -> bool {
        match self.mode {
            Mode::BlockOut {
                yb, down, preset, ..
            } => {
                let d = match preset {
                    Preset::ItemEase if !self.chute => {
                        if !matches!(self.kind, Kind::PowerItem { what } if what >= super::WEAR) {
                            return true;
                        }
                        HEADDRESS_DRAW
                    }
                    _ => 0.0,
                };
                if down {
                    self.y + self.height() - d > yb - 8.0
                } else {
                    self.y + d < yb + 8.0
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
            wet,
            preset,
            note,
            mut clear,
        } = self.mode
        else {
            return;
        };
        if wet && !self.chute && matches!(self.kind, Kind::Goomba | Kind::Goombrat) {
            // a Goomba in water holds still, then hands over (0x71011a5470)
            self.timer = (self.timer - 1).max(0);
            left -= 1;
            if left > 0 {
                self.mode = Mode::BlockOut {
                    yb,
                    y0,
                    gravity,
                    down,
                    left,
                    wet,
                    preset,
                    note,
                    clear,
                };
            } else {
                self.end_shot();
            }
            return;
        }
        let winged = self.wings && !self.chute;
        if preset == Preset::ItemEase && !self.chute {
            return self.step_item_ease(yb, y0, gravity, down, left, wet, note);
        }
        // FloatSearch eases its speed, RedPataPata its climb (no gravity)
        if preset == Preset::RedPataPata && !self.chute {
            let v = self.vy.abs();
            let step = if !wet {
                PATA_DRY_STEP
            } else if v < 0.3 {
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
        // a Wiggler in water has no terrain for its Out (+0x524 bit 4) [obs v3.0.3]
        let no_terrain = preset == Preset::Hanachan && wet && !self.chute;
        if self.timer > 0 || no_terrain {
            self.timer = (self.timer - 1).max(0);
            self.x += self.vx;
            self.y += self.vy;
        } else {
            self.collide(level);
        }
        if preset == Preset::Coin {
            // the coin's box and terrain wait until it is clear of the block, once
            clear |= if down {
                self.y <= yb - 8.0 + COIN_CLEAR_DOWN
            } else {
                self.y >= yb - 8.0 + COIN_CLEAR_UP
            };
            if !clear {
                self.timer = self.timer.max(1);
            }
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
                Preset::Coin => {
                    if down {
                        self.vy >= 0.0
                    } else {
                        self.vy <= COIN_END
                    }
                }
                // in water: once nearly still; out of it: on landing, or once in water
                Preset::Hanachan if wet => self.vy.abs() <= WIGGLER_WET_STOP,
                Preset::Hanachan => self.on_ground || self.wet(),
                Preset::Togezo if wet && !self.alt => {
                    let out = self
                        .w3
                        .liquid
                        .is_some_and(|(y, water)| !water || y <= self.y + SPINY_WET_TOP);
                    self.vy.abs() <= SPINY_WET_STOP || out || self.on_ground
                }
                Preset::Togezo => {
                    let past = if down {
                        self.y + self.height() <= yb - 8.0
                    } else {
                        self.y >= yb + 8.0
                    };
                    self.on_ground || ((past || self.timer <= 0) && (self.wet() || self.in_lava()))
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
            wet,
            preset,
            note,
            clear,
        };
        if done {
            if self.kind == Kind::LavaBubble && !self.chute {
                // falls from here into its own cycle
                self.phase = 2;
            }
            self.end_shot();
        }
    }

    /// ItemBalloon / ItemHeaddress without a parachute (0x7101339de0 → 0x7101347480): eased
    /// out to its spot, or a winged one's hop; nothing touches it meanwhile (`timer` is left).
    #[allow(clippy::too_many_arguments)]
    fn step_item_ease(
        &mut self,
        yb: f32,
        y0: f32,
        gravity: f32,
        down: bool,
        mut left: i32,
        wet: bool,
        note: bool,
    ) {
        let done = if self.wings {
            self.vy = (self.vy + gravity).max(MAX_FALL);
            self.y += self.vy;
            if down {
                left -= 1;
                left < 1
            } else {
                self.vy <= 0.0
            }
        } else {
            let target = yb
                + match (down, note) {
                    (false, false) => EASE_UP,
                    (true, false) => EASE_DOWN,
                    (false, true) => EASE_NOTE_UP,
                    (true, true) => EASE_NOTE_DOWN,
                };
            let k = if wet { EASE_WET_RATE } else { EASE_RATE };
            let d = target - self.y;
            let step = (d * k).abs().max(EASE_MIN_STEP);
            self.y = if d > 0.0 {
                (self.y + step).min(target)
            } else {
                (self.y - step).max(target)
            };
            (target - self.y).abs() <= EASE_ARRIVED
        };
        self.mode = Mode::BlockOut {
            yb,
            y0,
            gravity,
            down,
            left,
            wet,
            preset: Preset::ItemEase,
            note,
            clear: false,
        };
        if done {
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
        // 2.0 → 0.5 by 0.06 a frame (25 frames, 31.75 px), then 0.5 a frame to 74 px
        let n = run_out(&mut a, &lvl);
        assert!(a.y - y0 >= 74.0 && a.y - y0 < 76.0, "{}", a.y - y0);
        assert!((108..=113).contains(&n), "{n}");
        assert!(a.wings && a.mode == Mode::Normal);
        // in water it slows by the speed bands: 2.0 → 1.2 at 0.024, then 0.012 …
        let mut w = Actor::new(Kind::Koopa { red: true }, BX, BY);
        w.wings = true;
        w.w3.flooded = true;
        w.block_out(BY, false, false);
        let n = run_out(&mut w, &lvl);
        assert!(n < 60, "{n}");
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
    fn a_fish_bone_leaves_as_a_cheep_cheep_does() {
        let lvl = course();
        let mut a = out(Kind::FishBone, false, false);
        assert_eq!(a.timer, 24);
        assert!(run_out(&mut a, &lvl) <= 24);
        // big, out of a downward block: 16 px lower
        let mut big = Actor::new(Kind::FishBone, BX, BY);
        big.big = true;
        big.block_out(BY, true, false);
        let mut plain = Actor::new(Kind::Goomba, BX, BY);
        plain.big = true;
        plain.block_out(BY, true, false);
        assert_eq!(big.y, plain.y - 16.0);
    }

    #[test]
    fn in_water_a_goomba_holds_still_and_a_lava_bubble_leaps_lower() {
        let lvl = course();
        let mut a = Actor::new(Kind::Goomba, BX, BY);
        a.w3.flooded = true;
        a.block_out(BY, false, false);
        let y = a.y;
        assert_eq!(run_out(&mut a, &lvl), 9, "1.5 x the 6-frame clear time");
        assert_eq!(a.y, y);
        let mut b = Actor::new(Kind::LavaBubble, BX, BY);
        b.w3.flooded = true;
        b.block_out(BY, false, false);
        assert_eq!(b.vy, 3.5);
        let mut c = Actor::new(Kind::LavaBubble, BX, BY);
        c.w3.flooded = true;
        c.block_out(BY, false, true);
        assert_eq!(c.vy, 5.0);
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

    #[test]
    fn a_wearable_eases_out_to_the_block_top_untouchable() {
        let lvl = course();
        let mut a = out(
            Kind::PowerItem {
                what: super::super::WEAR,
            },
            false,
            false,
        );
        // not moved 8 px back in; no launch
        assert_eq!((a.y, a.vy), (BY + 8.0, 0.0));
        let yb = BY + 8.0;
        let mut ys = vec![];
        while matches!(a.mode, Mode::BlockOut { .. }) && ys.len() < 200 {
            assert!(!a.block_clear() && !a.solid_contact(), "untouchable");
            a.step(&lvl, 0.0);
            ys.push(a.y);
        }
        // 7 px at a tenth of the distance (0.7, 0.63, …), then 0.05 a frame: 36 frames
        assert!((ys[0] - (yb + 0.7)).abs() < 1e-4, "{}", ys[0]);
        assert!((34..=38).contains(&ys.len()), "{}", ys.len());
        assert!(ys.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(*ys.last().unwrap(), yb + 7.0);
        assert_eq!(a.mode, Mode::Normal);
        // in water it eases at half the rate
        let mut w = Actor::new(
            Kind::PowerItem {
                what: super::super::WEAR + 2,
            },
            BX,
            BY,
        );
        w.w3.flooded = true;
        w.block_out(BY, false, false);
        assert!(run_out(&mut w, &lvl) > 50);
    }

    #[test]
    fn the_eased_targets_downward_and_out_of_a_note_block() {
        let lvl = course();
        let yb = BY + 8.0;
        let mut d = out(
            Kind::PowerItem {
                what: super::super::WEAR + 1,
            },
            false,
            true,
        );
        assert_eq!(d.y, yb - 16.0);
        run_out(&mut d, &lvl);
        assert_eq!(d.y, yb - 24.0, "hangs under the block");
        let mut n = Actor::new(
            Kind::PowerItem {
                what: super::super::WEAR + 1,
            },
            BX,
            BY,
        );
        n.block_out(BY, false, true);
        assert_eq!(n.y, yb + 12.0, "a note block's Yb + 8, 4 px higher");
        run_out(&mut n, &lvl);
        assert_eq!(n.y, yb + 39.0);
        // the P-Balloon is the world's pick (SMW's part 81)
        let mut b = Actor::new(
            Kind::PowerItem {
                what: super::super::STYLE2,
            },
            BX,
            BY,
        );
        b.block_out_as(Preset::ItemEase, BY, false, false);
        run_out(&mut b, &lvl);
        assert_eq!(b.y, yb + 7.0);
    }

    #[test]
    fn a_winged_wearable_hops_and_a_wearable_shows_once_out() {
        let lvl = course();
        let mut a = out(
            Kind::PowerItem {
                what: super::super::WEAR,
            },
            true,
            false,
        );
        assert_eq!(a.vy, 4.0);
        assert_eq!(run_out(&mut a, &lvl), 16, "4.0 / 0.25");
        assert!(a.wings && a.mode == Mode::Normal);
        let mut d = out(
            Kind::PowerItem {
                what: super::super::WEAR,
            },
            true,
            true,
        );
        assert_eq!(run_out(&mut d, &lvl), 9);
        // the wearable is drawn in front once its feet are 2 px under the block top; the
        // P-Balloon stays behind the block for its whole Out
        let mut h = out(
            Kind::PowerItem {
                what: super::super::WEAR,
            },
            false,
            false,
        );
        let mut b = Actor::new(
            Kind::PowerItem {
                what: super::super::STYLE2,
            },
            BX,
            BY,
        );
        b.block_out_as(Preset::ItemEase, BY, false, false);
        let mut front = 0;
        while matches!(h.mode, Mode::BlockOut { .. }) {
            assert!(b.in_block());
            front += !h.in_block() as i32;
            h.step(&lvl, 0.0);
            b.step(&lvl, 0.0);
        }
        assert!(front > 5, "{front}");
    }

    #[test]
    fn a_winged_coin_pops_and_is_touchable_only_once_clear_of_the_block() {
        let lvl = course();
        let mut a = out(Kind::FloatCoin, true, false);
        assert_eq!(a.vy, 2.5);
        let mut n = 0;
        while matches!(a.mode, Mode::BlockOut { .. }) {
            if a.solid_contact() {
                assert!(a.y >= BY + 24.0 - 3.0, "{}", a.y);
            }
            a.step(&lvl, 0.0);
            n += 1;
        }
        // 2.5 → −1.0 at −0.14 a frame
        assert_eq!(n, 25);
        assert!(a.y > BY + 24.0);
        assert!(a.wings && a.mode == Mode::Normal);
        // down: −2.5 slowing (its gravity turned over) until it stops
        let mut d = out(Kind::FloatCoin, true, true);
        assert_eq!(d.vy, -2.5);
        let n = run_out(&mut d, &lvl);
        assert!((17..=19).contains(&n), "{n}");
        assert!(d.y < BY - 16.0);
    }

    #[test]
    fn a_wiggler_in_water_has_contact_at_once_and_stops_slowing() {
        let lvl = course();
        let mut a = Actor::new(Kind::Wiggler, BX, BY);
        a.w3.flooded = true;
        a.block_out(BY, true, false);
        assert!(a.block_clear(), "contact from the start");
        assert_eq!(a.vy, -2.5);
        let n = run_out(&mut a, &lvl);
        // −2.5 + 0.13125 a frame to |vy| ≤ 0.2
        assert_eq!(n, 18);
        // up it leaves at |vy| ≤ 0.2 without landing (no terrain)
        let mut u = Actor::new(Kind::Wiggler, BX, BY);
        u.w3.flooded = true;
        u.block_out(BY, false, false);
        assert_eq!(run_out(&mut u, &lvl), 13);
        // out of water it hands over on reaching the water
        let mut l = Actor::new(Kind::Wiggler, BX, BY);
        l.w3.liquid = Some((BY - 4.0, true));
        l.block_out(BY, true, false);
        let n = run_out(&mut l, &lvl);
        assert!(l.y < BY - 4.0 && l.y > BY - 12.0, "{} after {n}", l.y);
    }

    #[test]
    fn a_spiny_in_water_balls_out_and_one_on_land_hands_over_in_water() {
        let lvl = course();
        let mut a = Actor::new(Kind::Spiny, BX, BY);
        a.w3.flooded = true;
        a.block_out(BY, true, false);
        assert_eq!(a.vy, -2.4);
        // −2.4 + 0.1875 a frame to |vy| ≤ 0.5
        assert_eq!(run_out(&mut a, &lvl), 11);
        // up: done once its top leaves the water
        let mut u = Actor::new(Kind::Spiny, BX, BY);
        u.w3.liquid = Some((BY + 16.0, true));
        u.block_out(BY, false, false);
        assert_eq!(u.vy, 2.5);
        run_out(&mut u, &lvl);
        assert!(u.y + 4.0 >= BY + 16.0 && u.y < BY + 16.0, "{}", u.y);
        // on land (above the water): handed over once out of the block and in the water
        let mut l = Actor::new(Kind::Spiny, BX, BY);
        l.w3.liquid = Some((BY - 12.0, true));
        l.block_out(BY, true, false);
        assert_eq!(l.vy, 0.0, "dry: the Default drop");
        assert_eq!(run_out(&mut l, &lvl), 9, "clear of the block, in the water");
        assert!(l.y < BY - 12.0 && l.y > BY - 24.0, "{}", l.y);
    }
}
