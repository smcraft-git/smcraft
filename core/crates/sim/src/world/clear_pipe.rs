//! Clear pipes in play (3D World): the player's state DrawDokanMove (0x6c), the enemies' and
//! items' DrawDokan context (In / Out), and how riders are drawn inside. The traveller itself is
//! crate::clear_pipe; the pipes' walls are terrain (level/clear_pipe.rs).
//!
//! Player [obs v3.0.3]: the entry tests (0x71015aa8c0, from the pipe check 0x71015e7840 that
//! tries the ordinary pipe first on each side), DrawDokanMove enter 0x71015afe50, exe
//! 0x71015b0300, exit 0x71015b0d40 (player vtable 0x7102954498 +0xdf8). Enemies and items
//! [obs v3.0.3]: the entry tests 0x71012100e0 / 0x71012a4ef0 (the enemy update 0x7101290e30),
//! the default cDrawDokan context (vtable 0x71028b3d20: enter 0x7101055e90, In 0x71010560d0 /
//! 0x7101056410 / 0x7101056760, Out 0x7101056aa0 / 0x7101056c00 / 0x7101056f40) and its
//! EnemyDB columns.

use super::{Event, World};
use crate::actors::{Actor, Kind, Mode};
use crate::clear_pipe::{self, Leg, Paths, Side, Traveller};
use crate::fmath::DetMath;
use crate::input::{self, Input};
use crate::level::Level;
use crate::player::{Control, Form};
use crate::render::Sprite;

/// What a rider's sound cue is (with crate::world::Event::ClearPipe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueKind {
    /// Into a mouth: `route_dokan_in`, and the ride's loop `route_dokan_move` starts.
    In,
    /// Out of the far end: the loop stops, `route_dokan_out`.
    Out,
}

/// A clear-pipe sound cue: who (None = the player, else an actor id) and what.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cue {
    pub actor: Option<u32>,
    pub kind: CueKind,
}

/// The player's entry radius around a mouth's centre: 7.89 px, 12.89 for a small player
/// swimming in a course of version ≥ 1 [obs 0x71015aa8c0: DAT 0x710231961c, 0x7102318cdc].
const PLAYER_RADIUS: f32 = 7.89;
const PLAYER_RADIUS_SWIM: f32 = 12.89;
/// An enemy's or item's entry radius: 16 px [obs 0x71012100e0].
const ACTOR_RADIUS: f32 = 16.0;
/// The player's offset from the path: (0, −0.1) [obs enter: +0x2aec = DAT 0x7102319394].
const PLAYER_OFF_Y: f32 = -0.1;
/// The player's side probe height above the feet: 11.9 px (8 while crouching) [obs
/// 0x71015e7840: DAT 0x71023194e8].
const SIDE_PROBE_Y: f32 = 11.9;
/// A side entry: the direction held 10 frames on the ground (the shared pipe predicate
/// 0x71015a9d00, counter +0x23e4 > 9) [obs].
const SIDE_HOLD: u32 = 10;
/// An enemy's way out: 4 px a frame, gravity −0.1875, max fall −4; a horizontal speed over
/// 0.5 eases to 0.5 × facing by 0.4 a frame (to 0 for the Pakkun out type); the Float type
/// leaves at ¼ speed with no gravity, as do flying enemies [obs 0x7101056760, 0x7101056aa0].
const OUT_SPEED: f32 = 4.0;
const OUT_GRAVITY: f32 = -0.1875;
const OUT_MAX_FALL: f32 = -4.0;
const OUT_ACCEL: f32 = 0.4;
const OUT_TARGET: f32 = 0.5;
/// Out ends 10 frames after it starts (sooner on landing or at a wall) [obs 0x7101056aa0:
/// +0x8c = 10, counted in 0x7101056c00].
const OUT_FRAMES: u8 = 10;
/// A vertical exit ignores the terrain for 2 frames (+0x8d = 2, +0x524 bit 25) [obs].
const OUT_NO_TERRAIN: u8 = 2;
/// Out of a horizontal exit an enemy looks 16 px ahead for another clear pipe's mouth and goes
/// straight in [obs 0x7101056c00 → 0x71012a53b0(±16)].
const OUT_LOOKAHEAD: f32 = 16.0;
/// Item riders (and forced small ones) are drawn at 66 % [obs 0x71010560d0: 0x3f28f5c3].
const ITEM_SCALE: f32 = 0.66;

/// How a rider is turned inside (EnemyDB `drawdokan_rotate_type`; only read in 3D World, else
/// none) [obs 0x710129db00: Normal 0, Screw 1, Item 2].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotate {
    /// `Invalid`: drawn as it is.
    None,
    /// Rolled to point along the pipe.
    Normal,
    /// Rolled along the pipe and spinning.
    Screw,
    /// Shrunk to 66 %.
    Item,
}

/// How a rider comes out (EnemyDB `drawdokan_outtype`) [obs 0x710129d900: Default 0, Togemet
/// 1, Pakkun 2, Float 3].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutType {
    Default,
    /// Straight back to its normal context, no launch.
    Togemet,
    /// Launched, its speed easing to 0.
    Pakkun,
    /// Launched at ¼ speed, no gravity.
    Float,
}

/// An actor kind's clear-pipe row (EnemyDB `enable_drawdokan`, `drawdokan_offset_y`,
/// `drawdokan_rotate_offset`, `drawdokan_rotate_type`, `drawdokan_outtype`, the 3D World rows)
/// [obs enemydb.tsv].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub offset_y: f32,
    pub rotate_offset: f32,
    pub rotate: Rotate,
    pub out: OutType,
}

impl Profile {
    /// The traveller's offset y: `offset_y`, less (`rotate_offset` − 8) for the rolled types
    /// [obs vtable +0x160, 0x71010570e0].
    pub fn travel_off_y(&self) -> f32 {
        match self.rotate {
            Rotate::Normal | Rotate::Screw => self.offset_y - (self.rotate_offset - 8.0),
            _ => self.offset_y,
        }
    }
}

/// The 3D World row of an actor kind, None where the game disables the pipe (`Disable`, or no
/// row in play here) [obs enemydb.tsv].
pub fn profile(kind: Kind, big: bool) -> Option<Profile> {
    use OutType as O;
    use Rotate as R;
    let row = |oy: f32, ro: f32, rotate: R, out: O| Profile {
        offset_y: oy,
        rotate_offset: ro,
        rotate,
        out,
    };
    // (small row, big row)
    let pick = |s: Profile, b: Profile| if big { b } else { s };
    Some(match kind {
        // Kuribo3W / KuriboBig3W
        Kind::Goomba => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-8.0, 10.0, R::Screw, O::Default),
        ),
        // Kuribon
        Kind::Goombrat => pick(
            row(-8.0, 8.0, R::None, O::Default),
            row(-10.0, 8.0, R::None, O::Default),
        ),
        // NokonokoGreen / NokonokoRed (their shells are the same actor)
        Kind::Koopa { red } | Kind::Shell { red } => pick(
            row(if red { -8.0 } else { -10.0 }, 12.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        // Karon, KaronShell (the Buzzy and Spiny shells are disabled)
        Kind::DryBones => pick(
            row(-8.0, 12.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        Kind::EmptyShell { what: 3 } => row(-8.0, 8.0, R::None, O::Default),
        // Togezo, Met, TogemetYellow, Bombhei
        Kind::Spiny => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-10.0, 10.0, R::Screw, O::Default),
        ),
        Kind::Buzzy => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        Kind::SpikeTop { .. } => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        Kind::BobOmb => pick(
            row(-8.0, 10.0, R::Screw, O::Default),
            row(-9.0, 16.0, R::Screw, O::Default),
        ),
        // Pakkun3W / FirePakkun3W
        Kind::Piranha { .. } => pick(
            row(-8.0, 12.0, R::Screw, O::Pakkun),
            row(-10.0, 16.0, R::Screw, O::Pakkun),
        ),
        // Gesso3W
        Kind::Blooper => pick(
            row(-8.0, 12.0, R::Screw, O::Float),
            row(-10.0, 12.0, R::Screw, O::Float),
        ),
        Kind::FishBone => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        // Arihei, Pyonchu, MechaKoopa
        Kind::AntTrooper { .. } => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-10.0, 12.0, R::Screw, O::Default),
        ),
        Kind::Skipsqueak { .. } => pick(
            row(-8.0, 8.0, R::Screw, O::Default),
            row(-8.0, 10.0, R::Screw, O::Default),
        ),
        Kind::Mechakoopa { .. } => pick(
            row(-8.0, 12.0, R::Screw, O::Default),
            row(-10.0, 16.0, R::Screw, O::Default),
        ),
        // items: SuperKinoko, FireFlower, SuperStar, 1upKinoko, SuperBell, SuperHammer, PSwitch,
        // PowBlock, JumpStep(Side), Stone, WoodBox, Coin10
        Kind::Mushroom
        | Kind::FireFlower
        | Kind::Star
        | Kind::OneUp
        | Kind::StyleItem
        | Kind::SuperHammer
        | Kind::PSwitch
        | Kind::Pow
        | Kind::Trampoline { .. }
        | Kind::SideSpring
        | Kind::Stone
        | Kind::Crate { .. } => row(-8.0, 8.0, R::Item, O::Default),
        Kind::BigCoin { value: 0 } => row(-12.0, 16.0, R::Item, O::Default),
        _ => return None,
    })
}

/// The player in a pipe: the traveller and the drawn roll (+0x264, a binary angle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerRide {
    pub t: Traveller,
    pub roll: u32,
}

/// Where an actor is in its DrawDokan context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Carried by the traveller (Mode::ClearPipe: no collision, no contact).
    In,
    /// Launched from the exit (Mode::Normal again): frames of Out left, frames without terrain,
    /// the exit's side.
    Out {
        left: u8,
        no_terrain: u8,
        side: Side,
    },
}

/// An enemy or item in a pipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rider {
    pub id: u32,
    pub t: Traveller,
    pub phase: Phase,
    pub profile: Profile,
    /// Drawn roll (+0x264; the Screw type's spin about the vertical, +0x260, is not drawn).
    pub roll: u32,
    /// Out: the speed it eases toward and the step (0: none).
    target: f32,
    accel: f32,
    gravity: f32,
    /// In: the last move (its heading launches it out); its speeds show 0.
    moved: (f32, f32),
}

/// The world's clear-pipe riders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClearPipes {
    pub player: Option<PlayerRide>,
    pub riders: Vec<Rider>,
    /// The tick the player last came out (no new entry on that frame).
    left_at: Option<u64>,
    /// This tick's sound cues (cleared with the events).
    pub cues: Vec<Cue>,
    /// Out of a downward mouth (x, y): the body starts inside the pipe's last tiles.
    drop: Option<(f32, f32)>,
    /// The player came out this frame: its own update runs on, without input.
    pub(super) out_step: bool,
}

impl Rider {
    /// Still being carried (In), not coming out.
    pub fn inside(&self) -> bool {
        self.phase == Phase::In
    }
}

impl ClearPipes {
    /// The world moves this actor (in a pipe or coming out of one), not its own update.
    pub fn carries(&self, id: u32) -> bool {
        self.riders.iter().any(|r| r.id == id)
    }
}

/// A binary angle to degrees, counter-clockwise.
fn degrees(a: u32) -> f32 {
    a as i32 as f32 * (180.0 / 2_147_483_648.0)
}

/// Turn `from` toward `to` by at most `step` (binary angles, the short way).
fn ease_angle(from: u32, to: u32, step: u32) -> u32 {
    let d = to.wrapping_sub(from) as i32;
    if d.unsigned_abs() <= step {
        to
    } else if d > 0 {
        from.wrapping_add(step)
    } else {
        from.wrapping_sub(step)
    }
}

fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v < target {
        (v + step).min(target)
    } else {
        (v - step).max(target)
    }
}

/// Any solid cell on the segment from `a` toward `d` for `len` px (sampled every 4 px and at
/// the end) — the game's terrain ray (0x7100e2d2e0) on the tile grid.
fn ray_hits(level: &Level, a: (f32, f32), d: (f32, f32), len: f32) -> bool {
    let steps = (len / 4.0).ceil().max(1.0) as i32;
    (0..=steps).any(|k| {
        let s = len * k as f32 / steps as f32;
        level.cell_at(a.0 + d.0 * s, a.1 + d.1 * s).is_solid()
    })
}

/// An enemy's or item's block test (0x71012965b0 → 0x7100d831e0 with 15): terrain within 14 px
/// past the end `p` toward `side`, on rays 8 px either side of the centre line (1 px in courses
/// of version 0); out of a side mouth, another clear pipe's mouth just ahead is no block (the
/// rider goes straight into it) [obs; the ray filters 0x7102852e78 / e98 are taken as terrain].
fn actor_way_blocked(level: &Level, p: (f32, f32), side: Side) -> bool {
    let paths = &level.clear_pipe_paths;
    let d = side.vector();
    if d.0 != 0.0
        && clear_pipe::find_entrance(paths, (p.0 + d.0, p.1), side, ACTOR_RADIUS, |_, _| true)
            .is_some()
    {
        return false;
    }
    let perp = (d.1.abs(), d.0.abs());
    let w = if paths.version >= 1 { 8.0 } else { 1.0 };
    [-w, w].iter().any(|&o| {
        let a = (p.0 + perp.0 * o + d.0, p.1 + perp.1 * o + d.1);
        ray_hits(level, a, d, 14.0)
    })
}

/// The player's block test (0x71015acf60, the traveller's callback +0x60 = player +0x2b40):
/// room for the body past the end `p` — sideways, rays at the centre line ± 8 from 0.1 px out,
/// the body's width long; up or down, rays 7.5 px either side of the centre from 0.1 px out,
/// 15 px long (31 for a big player) [obs; the big-body flags +0x4c0 / +0x4c1 read as `big`
/// inf]. (The game's downward rays start 8 px inside in courses of version ≥ 1, the body hanging
/// that far under the mouth; the part inside the pipe meets no face.)
fn player_way_blocked(level: &Level, big: bool, width: f32, p: (f32, f32), side: Side) -> bool {
    let (dx, dy) = side.vector();
    if dx != 0.0 {
        return [-8.0f32, 8.0]
            .iter()
            .any(|&o| ray_hits(level, (p.0 + dx * 0.1, p.1 + o), (dx, 0.0), width));
    }
    let len = if big { 31.0 } else { 15.0 };
    [-7.5f32, 7.5]
        .iter()
        .any(|&o| ray_hits(level, (p.0 + o, p.1 + dy * 0.1), (0.0, dy), len))
}

/// The mouth test (0x7100d83600): nothing solid just outside the mouth `p`, on rays 8 px either
/// side of its centre line from 0.1 px out, `len` px long (the rider's width; 15 px across an
/// up / down mouth) [obs]. `side` is the rider's way in.
fn mouth_clear(level: &Level, p: (f32, f32), side: Side, len: f32) -> bool {
    let (dx, dy) = side.vector();
    let back = (-dx, -dy);
    let perp = (dy.abs(), dx.abs());
    let len = if dx == 0.0 { 15.0 } else { len };
    ![-8.0f32, 8.0].iter().any(|&o| {
        let a = (
            p.0 + perp.0 * o + back.0 * 0.1,
            p.1 + perp.1 * o + back.1 * 0.1,
        );
        ray_hits(level, a, back, len)
    })
}

impl World {
    fn cue(&mut self, actor: Option<u32>, kind: CueKind) {
        self.cpipe.cues.push(Cue { actor, kind });
        if !self.events.contains(&Event::ClearPipe) {
            self.events.push(Event::ClearPipe);
        }
    }

    /// The paths of the area in play.
    fn pipe_paths(&self) -> &Paths {
        &self.level.clear_pipe_paths
    }

    /// The player's clear-pipe entry, after the ordinary pipes found none (the pipe check
    /// 0x71015e7840 → 0x71015aa8c0). Up: Up held at a downward mouth over the head (not while
    /// falling in courses of version ≥ 3, unless swimming); down: standing on an upward mouth
    /// with Down held; sideways: on the ground (or swimming), the direction held 10 frames, the
    /// wall sensor on the mouth's face (the binary's other way in, pushing the face while facing
    /// it, wants more than a push: `_pushing` unused [?]). The probe — the head, the feet − 1, or
    /// the body's side + 1 at 11.9 px up — must be within 7.89 px of the mouth's centre, the
    /// pipe must lead on that way and the mouth be clear. Returns whether the player went in.
    pub(super) fn clear_pipe_entry(&mut self, inp: Input, _pushing: bool) -> bool {
        if self.pipe_paths().pipes.is_empty()
            || self.cpipe.player.is_some()
            || self.cpipe.left_at == Some(self.tick)
            || self.player.control != Control::Normal
            || self.player.yoshi.is_some()
            || self.player.clown.is_some()
            || self.player.car.is_some()
        {
            return false;
        }
        let p = &self.player;
        let version = self.pipe_paths().version;
        let s = p.sensors();
        let swimming = p.in_water && !p.on_ground;
        let grounded = p.on_ground || swimming;
        let probe_y = p.y + if p.crouch { 8.0 } else { SIDE_PROBE_Y };
        let mut tries: Vec<(Side, (f32, f32))> = Vec::new();
        let falling_block = version >= 3 && p.vy < 0.0 && !p.in_water;
        if inp.held(input::UP) && !falling_block {
            tries.push((Side::Up, (p.x, p.y + s.head_y)));
        }
        if grounded {
            // (the direction held 10 frames on the ground: pushing the face while facing it
            // isn't enough by itself [obs: Ryujinx trace X_cpipe: pushing a mouth's face for 9
            // frames after landing, in on the 10th held frame; the push predicate 0x71015a9d00's
            // extra condition not found [?]])
            let hold = self.side_hold;
            if inp.held(input::RIGHT) && hold.right >= SIDE_HOLD {
                tries.push((Side::Right, (p.x + s.wall_x + 1.0, probe_y)));
            }
            if inp.held(input::LEFT) && hold.left >= SIDE_HOLD {
                tries.push((Side::Left, (p.x - (s.wall_x + 1.0), probe_y)));
            }
        }
        if p.on_ground && inp.held(input::DOWN) {
            tries.push((Side::Down, (p.x, p.y - 1.0)));
        }
        let radius = if version > 0 && p.form == Form::Small && swimming {
            PLAYER_RADIUS_SWIM
        } else {
            PLAYER_RADIUS
        };
        let width = 2.0 * s.wall_x;
        let found = tries.into_iter().find_map(|(side, probe)| {
            let level = &self.level;
            clear_pipe::find_entrance(self.pipe_paths(), probe, side, radius, |m, sd| {
                mouth_clear(level, m, sd, width)
            })
            .filter(|&(k, last)| {
                // the head or feet against the mouth's face (the sensor reports its kind)
                let pts = &self.pipe_paths().pipes[k];
                let m = if last { pts[pts.len() - 1] } else { pts[0] };
                // (sideways: the wall sensor on the mouth's face, the player pushed flush against
                // it [obs: Ryujinx trace X_cpipe: walking in with Right held for 90 frames, in
                // only from the frame after the one that stops it at 184.1 by a mouth at 192,
                // the probe 7.7 px from the mouth's centre two frames before])
                match side {
                    Side::Up => probe.1 >= m.1 - 1.0,
                    Side::Down => (p.y - m.1).abs() < 1.0,
                    Side::Right => p.x + s.wall_x >= m.0 - 0.01,
                    Side::Left => p.x - s.wall_x <= m.0 + 0.01,
                }
            })
        });
        let Some((k, last)) = found else {
            return false;
        };
        let t = Traveller::enter(self.pipe_paths(), k, last, (0.0, PLAYER_OFF_Y));
        let p = &mut self.player;
        p.control = Control::Demo;
        (p.vx, p.vy) = (0.0, 0.0);
        p.crouch = false;
        // (it stays where it stood this frame, on the ground: the ride moves it from the mouth
        // from the next [obs: X_cpipe, 3 px past the mouth the frame after])
        self.cpipe.player = Some(PlayerRide { t, roll: 0 });
        self.cue(None, CueKind::In);
        true
    }

    /// The player's frame in a pipe (DrawDokanMove's exe 0x71015b0300): the traveller's update,
    /// the position (path + (0, −0.1)), the facing from the heading and the roll — ±90° at
    /// once on a vertical segment (the front leading), eased by the traveller's 10° a frame in a
    /// bend — and the way out: a horizontal exit sets the player down by half its height (16 or
    /// 32: centred on the pipe's line), speeds stay 0, and a big player with no room to stand
    /// stoops. Returns false when there is no ride, or it came out (its update runs on).
    pub(super) fn clear_pipe_player(&mut self) -> bool {
        let Some(mut ride) = self.cpipe.player else {
            return false;
        };
        // (a form change finished meanwhile: still in the pipe)
        if self.player.control == Control::Normal {
            self.player.control = Control::Demo;
        }
        {
            let (big, width) = (self.player.big(), 2.0 * self.player.sensors().wall_x);
            let level = &self.level;
            let paths = &level.clear_pipe_paths;
            ride.t.update(paths, |pt, side| {
                player_way_blocked(level, big, width, pt, side)
            });
        }
        let t = ride.t;
        let p = &mut self.player;
        (p.x, p.y) = t.pos();
        (p.vx, p.vy) = (0.0, 0.0);
        p.on_ground = false;
        let right = p.facing > 0;
        const UP90: u32 = 0x4000_0000;
        const DOWN90: u32 = 0xc000_0000;
        if t.dy == 0.0 {
            if t.dx < 0.0 {
                p.facing = -1;
            } else if t.dx > 0.0 {
                p.facing = 1;
            }
            ride.roll = 0;
        } else if t.dx == 0.0 {
            // the front leads: up turns a right-facing player +90°, down −90°
            ride.roll = if (t.dy > 0.0) == right { UP90 } else { DOWN90 };
        } else {
            if t.dx < 0.0 {
                p.facing = -1;
            } else {
                p.facing = 1;
            }
            let right = p.facing > 0;
            let to = if t.leg == Some(Leg::BendFromVertical) {
                0
            } else if (t.dy > 0.0) == right {
                UP90
            } else {
                DOWN90
            };
            ride.roll = ease_angle(ride.roll, to, t.step);
        }
        p.drawn_facing = p.facing;
        if !t.done {
            self.cpipe.player = Some(ride);
            return true;
        }
        // out
        if matches!(t.leg, Some(Leg::Left | Leg::Right)) {
            let h = if p.big() || p.yoshi.is_some() {
                32.0
            } else {
                16.0
            };
            p.y -= h * 0.5;
        }
        p.control = Control::Normal;
        p.on_ground = false;
        // no room to stand: a big player comes out stooping (0x7101595010) [obs]
        if p.big() {
            let head = p.y + crate::player::SUPER.head_y;
            let lv = &self.level;
            if [p.x - 4.0, p.x + 4.0]
                .iter()
                .any(|&x| lv.cell_at(x, head).is_solid())
            {
                p.crouch = true;
            }
        }
        if t.leg == Some(Leg::Down) {
            let pts = &self.level.clear_pipe_paths.pipes[t.pipe];
            let end = if t.back { pts[0] } else { pts[pts.len() - 1] };
            self.cpipe.drop = Some(end);
        }
        self.cpipe.player = None;
        self.cpipe.left_at = Some(self.tick);
        self.cue(None, CueKind::Out);
        // the state it goes on in (Fall) runs this frame too, before any input counts [obs:
        // Ryujinx trace X_cpipe: out of a side mouth 8 px down and a fall step down that frame,
        // its speed still 0]
        self.cpipe.out_step = true;
        false
    }

    /// Before the player's own update: out of a downward mouth, the body starts inside the
    /// pipe's last tiles; the game's terrain sensors meet faces, not insides, so nothing pushes
    /// it until it has dropped clear [inf: the BG check is by edges]. The mouth's cells within
    /// the body are lifted for the update (`clear_pipe_unlift` puts them back).
    pub(super) fn clear_pipe_lift(&mut self) -> Vec<(i32, i32)> {
        let Some((mx, my)) = self.cpipe.drop else {
            return Vec::new();
        };
        let p = &self.player;
        let head = p.y + p.sensors().head_y;
        if head <= my || p.control != Control::Normal {
            self.cpipe.drop = None;
            return Vec::new();
        }
        let (c0, r0, r1) = (
            (mx / 16.0) as i32 - 1,
            (my / 16.0) as i32,
            (head / 16.0) as i32,
        );
        let mut lifted = Vec::new();
        for tx in c0..c0 + 2 {
            for ty in r0..=r1 {
                if self.level.cell(tx, ty).is_solid() {
                    self.level.set(tx, ty, crate::level::Cell::Empty);
                    lifted.push((tx, ty));
                }
            }
        }
        lifted
    }

    pub(super) fn clear_pipe_unlift(&mut self, lifted: Vec<(i32, i32)>) {
        for (tx, ty) in lifted {
            self.level.set(tx, ty, crate::level::Cell::Solid);
        }
    }

    /// The player's drawn pose inside: centred on the path (the sprite's origin is the feet),
    /// rolled, in front of the pipe (the enter's draw-in-front flag, model +0xf1) [obs; the
    /// centring is inf: the in-pipe animation's root].
    pub(super) fn clear_pipe_player_pose(&self, s: &mut Sprite) {
        let Some(ride) = self.cpipe.player else {
            return;
        };
        s.y = ride.t.y - s.h * 0.5;
        s.roll = degrees(ride.roll);
        s.layer = super::CLEAR_PIPE_LAYER + 1;
    }

    /// Enemies and items in pipes, and new ones going in (after the actors' own updates).
    pub(super) fn clear_pipe_actors(&mut self) {
        if self.level.clear_pipe_paths.pipes.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.actors.iter().map(|a| a.id).collect();
        self.cpipe.riders.retain(|r| ids.contains(&r.id));
        let mut cues = Vec::new();
        let mut riders = std::mem::take(&mut self.cpipe.riders);
        riders.retain_mut(|r| {
            let Some(a) = self.actors.iter_mut().find(|a| a.id == r.id) else {
                return false;
            };
            if !a.alive() {
                return false;
            }
            match r.phase {
                Phase::In => {
                    let level = &self.level;
                    let paths = &level.clear_pipe_paths;
                    r.t.update(paths, |pt, side| actor_way_blocked(level, pt, side));
                    in_frame(a, r);
                    if r.t.done {
                        cues.push(Cue {
                            actor: Some(a.id),
                            kind: CueKind::Out,
                        });
                        // Out's first frame runs on the frame it comes out [obs: Ryujinx trace
                        // X_cpipeg: 8 px past the mouth, then 3.6 px on (4 eased by 0.4) and
                        // a gravity step, all that frame]
                        return come_out(a, r) && out_frame(a, r, &self.level);
                    }
                    true
                }
                Phase::Out { .. } => out_frame(a, r, &self.level),
            }
        });
        // a launched rider meeting another mouth goes straight in; then new riders
        for i in 0..self.actors.len() {
            let a = &self.actors[i];
            let out_side = riders
                .iter()
                .find(|r| r.id == a.id)
                .and_then(|r| match r.phase {
                    Phase::Out { side, .. } if matches!(side, Side::Left | Side::Right) => {
                        Some(side)
                    }
                    _ => None,
                });
            let fresh = !riders.iter().any(|r| r.id == a.id);
            if !(fresh || out_side.is_some()) {
                continue;
            }
            let Some(prof) = profile(a.kind, a.big) else {
                continue;
            };
            if let Some((k, last, side)) = self.actor_entrance(a, out_side) {
                let a = &mut self.actors[i];
                riders.retain(|r| r.id != a.id);
                let t = Traveller::enter(
                    &self.level.clear_pipe_paths,
                    k,
                    last,
                    (0.0, prof.travel_off_y()),
                );
                riders.push(go_in(a, t, prof, side));
                cues.push(Cue {
                    actor: Some(a.id),
                    kind: CueKind::In,
                });
            }
        }
        self.cpipe.riders = riders;
        for c in cues {
            self.cue(c.actor, c.kind);
        }
    }

    /// An actor's way into a pipe this frame: walking (or sliding) into a mouth's face — its
    /// side's probe (half width out, half height up) within 16 px of the mouth's centre — or,
    /// just out of a horizontal exit, a mouth 16 px ahead [obs 0x71012100e0, 0x7101056c00].
    fn actor_entrance(&self, a: &Actor, out: Option<Side>) -> Option<(usize, bool, Side)> {
        if !a.alive() || a.held || a.stacked {
            return None;
        }
        let (hw, h) = (a.half_w(), a.height());
        let (side, reach) = match out {
            Some(side) => (side, OUT_LOOKAHEAD),
            None => {
                let moving = matches!(a.mode, Mode::Normal | Mode::ShellSlide);
                if !moving || !a.wall || a.vx == 0.0 {
                    return None;
                }
                let side = if a.vx > 0.0 { Side::Right } else { Side::Left };
                (side, hw)
            }
        };
        let dir = side.vector().0;
        let probe = (a.x + dir * reach, a.y + h * 0.5);
        let level = &self.level;
        clear_pipe::find_entrance(
            &level.clear_pipe_paths,
            probe,
            side,
            ACTOR_RADIUS,
            |m, sd| mouth_clear(level, m, sd, 2.0 * hw),
        )
        .map(|(k, last)| (k, last, side))
    }

    /// An actor's drawn pose inside: rolled along the pipe (Normal / Screw) or shrunk to 66 %
    /// about its middle (Item), in front of the pipe.
    pub(super) fn clear_pipe_actor_pose(&self, a: &Actor, s: &mut Sprite) {
        let Some(r) = self.cpipe.riders.iter().find(|r| r.id == a.id) else {
            return;
        };
        if r.phase != Phase::In {
            return;
        }
        match r.profile.rotate {
            Rotate::Normal | Rotate::Screw => s.roll = degrees(r.roll),
            Rotate::Item => {
                let (cx, cy) = (s.x + s.w * 0.5, s.y + a.height() * 0.5);
                s.w *= ITEM_SCALE;
                s.h *= ITEM_SCALE;
                s.x = cx - s.w * 0.5;
                s.y = cy - a.height() * 0.5 * ITEM_SCALE;
            }
            Rotate::None => {}
        }
        s.layer = super::CLEAR_PIPE_LAYER + 1;
    }
}

/// Into the pipe (the context's enter and In's enter 0x7101055e90 / 0x71010560d0): no
/// collision or contact, speeds and gravity 0, a parachute dropped; it stays where it stood
/// this frame, the ride moving it from the mouth from the next [obs: Ryujinx trace X_cpipeg, a
/// Goomba stopped 0.1 px into the face's wall the frame it goes in, 3 px past the mouth the
/// next].
fn go_in(a: &mut Actor, t: Traveller, profile: Profile, _side: Side) -> Rider {
    a.mode = Mode::ClearPipe;
    (a.vx, a.vy) = (0.0, 0.0);
    a.chute = false;
    a.on_ground = false;
    // (the walk it comes back to is a new one: the mouth's face it pressed going in is no wall
    // to turn at [obs: X_cpipeg, the Goomba walks on the way it came out])
    (a.wall, a.wall_prev, a.touch_front) = (false, false, false);
    Rider {
        id: a.id,
        t,
        phase: Phase::In,
        profile,
        roll: 0,
        target: 0.0,
        accel: 0.0,
        gravity: 0.0,
        moved: (0.0, 0.0),
    }
}

/// In's exe (0x7101056410): the position, the facing turned to the heading, and the drawn roll
/// (atan2 of the heading, the rolled types) with the Screw spin.
fn in_frame(a: &mut Actor, r: &mut Rider) {
    let (x, y) = r.t.pos();
    r.moved = (x - a.x, y - a.y);
    (a.vx, a.vy) = (0.0, 0.0);
    (a.x, a.y) = (x, y);
    let len = (r.t.dx * r.t.dx + r.t.dy * r.t.dy).sqrt();
    let ux = if len > 0.0 { r.t.dx / len } else { r.t.dx };
    if (a.dir > 0 && ux < 0.0) || (a.dir < 0 && ux > 0.0) {
        a.dir = -a.dir;
    }
    if matches!(r.profile.rotate, Rotate::Normal | Rotate::Screw) {
        // atan2(−dx, dy) as a binary angle: 0 heading up, a quarter turn heading left
        let uy = if len > 0.0 { r.t.dy / len } else { r.t.dy };
        let rad = (-ux).datan2(uy);
        r.roll = (rad * (2_147_483_648.0 / std::f32::consts::PI)) as i32 as u32;
    }
    a.age += 1;
}

/// In's exit (0x7101056760) into Out, or straight back for the Togemet type: the launch from
/// the last move's heading. Returns whether the actor stays a rider.
fn come_out(a: &mut Actor, r: &mut Rider) -> bool {
    a.mode = Mode::Normal;
    if r.profile.out == OutType::Togemet {
        (a.vx, a.vy) = (0.0, 0.0);
        return false;
    }
    let (mx, my) = r.moved;
    let len = (mx * mx + my * my).sqrt();
    let (ux, uy) = if len > 0.0 {
        (mx / len, my / len)
    } else {
        (0.0, 0.0)
    };
    let eps = f32::from_bits(0x3400_0000);
    let side = if ux.abs() > eps {
        if ux < 0.0 { Side::Left } else { Side::Right }
    } else if uy <= 0.0 {
        Side::Down
    } else {
        Side::Up
    };
    let (mut vx, mut vy) = (mx, my);
    match side {
        Side::Right => vx = OUT_SPEED,
        Side::Left => vx = -OUT_SPEED,
        Side::Up => vy = OUT_SPEED,
        Side::Down => vy = -OUT_SPEED,
    }
    if r.profile.out == OutType::Float {
        (vx, vy) = (vx * 0.25, vy * 0.25);
    }
    a.vx = vx;
    a.vy = vy.max(OUT_MAX_FALL);
    a.dir = if vx < 0.0 {
        -1
    } else if vx > 0.0 {
        1
    } else {
        a.dir
    };
    let flies = a.wings || r.profile.out == OutType::Float;
    r.gravity = if flies { 0.0 } else { OUT_GRAVITY };
    // Out's enter: ease a fast launch to half a pixel the way it faces (the Pakkun type to a stop)
    (r.target, r.accel) = if r.profile.out == OutType::Pakkun {
        (0.0, OUT_ACCEL)
    } else if a.vx.abs() > OUT_TARGET {
        (OUT_TARGET * a.dir as f32, OUT_ACCEL)
    } else {
        (0.0, 0.0)
    };
    let vertical = matches!(side, Side::Up | Side::Down);
    r.phase = Phase::Out {
        left: OUT_FRAMES,
        no_terrain: if vertical { OUT_NO_TERRAIN } else { 0 },
        side,
    };
    r.roll = 0;
    true
}

/// Out's exe (0x7101056c00) on the world's side: the launch's flight (terrain off its first
/// frames out of a vertical exit), until it lands, meets a wall or 10 frames pass; then the
/// actor's own update takes over. Returns whether it is still out.
fn out_frame(a: &mut Actor, r: &mut Rider, level: &Level) -> bool {
    let Phase::Out {
        left,
        no_terrain,
        side,
    } = r.phase
    else {
        return false;
    };
    if r.accel > 0.0 {
        a.vx = approach(a.vx, r.target, r.accel);
    }
    a.vy = (a.vy + r.gravity).max(OUT_MAX_FALL);
    a.wall = false;
    if no_terrain > 0 {
        a.x += a.vx;
        a.y += a.vy;
    } else {
        a.collide(level);
    }
    a.age += 1;
    let left = left.saturating_sub(1);
    if left == 0 || a.on_ground || a.wall {
        if a.on_ground {
            a.vy = 0.0;
        }
        return false;
    }
    r.phase = Phase::Out {
        left,
        no_terrain: no_terrain.saturating_sub(1),
        side,
    };
    true
}

/// The pipe a test course's player can enter: a straight run of `len` tiles going `right` (or
/// left) from base tile (x, y), its tiles solid (tests).
#[cfg(test)]
pub fn test_pipe(level: &mut Level, origin: (i32, i32), runs: &[(u8, i32)]) {
    use mmk_formats::smm2::clear_pipe::{Dir, Shape};
    let s = Shape {
        origin,
        runs: runs
            .iter()
            .map(|&(d, n)| (Dir::from_code(d).expect("dir"), n))
            .collect(),
    };
    assert!(s.valid(), "{s:?}");
    let rec = s
        .encode(level.clear_pipe_paths.pipes.len())
        .expect("encodes");
    for (tx, ty) in s.tiles() {
        level.set(tx, ty, crate::level::Cell::Solid);
    }
    level
        .clear_pipe_paths
        .pipes
        .push(crate::level::clear_pipe_path_points(&rec));
    level.clear_pipe_paths.version = 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;

    /// A flat 3D World course (ground two tiles deep, its top at y 32) with these pipes, each
    /// runs from a base tile (direction codes 0 right, 1 left, 2 up, 3 down).
    /// A pipe: its base tile and runs.
    type TestPipe<'a> = ((i32, i32), &'a [(u8, i32)]);

    fn course(pipes: &[TestPipe]) -> World {
        let mut lvl = Level::flat(60);
        lvl.style = "3W".into();
        for (origin, runs) in pipes {
            test_pipe(&mut lvl, *origin, runs);
        }
        let mut w = World::with_level(lvl);
        w.style = crate::player::Style::W3;
        w
    }

    fn hold(buttons: u32) -> Input {
        Input { buttons }
    }

    /// Step with `buttons` until `done` (at most `max` frames); the frames taken.
    fn run_until(w: &mut World, buttons: u32, max: u32, done: impl Fn(&World) -> bool) -> u32 {
        for k in 0..max {
            if done(w) {
                return k;
            }
            w.step(hold(buttons));
        }
        panic!("not done in {max} frames");
    }

    fn cues(w: &World) -> Vec<Cue> {
        w.cpipe.cues.clone()
    }

    #[test]
    fn the_player_walks_into_a_straight_pipe_and_comes_out_the_far_end() {
        // a 10-tile run going right from tile (10, 3): mouths at x 160 and 320, centre line y 48
        let mut w = course(&[((10, 3), &[(0, 10)])]);
        assert!(w.level.cell(12, 2).is_solid() && w.level.cell(19, 3).is_solid());
        w.player.x = 130.0;
        let entered = run_until(&mut w, input::RIGHT, 200, |w| w.cpipe.player.is_some());
        assert!(entered > 5, "walked to the mouth first");
        assert_eq!(w.player.control, Control::Demo);
        assert_eq!(
            cues(&w)[0],
            Cue {
                actor: None,
                kind: CueKind::In
            }
        );
        assert!(w.events.contains(&Event::ClearPipe));
        // still where it stood, then carried from the mouth at 3 px a frame, 0.1 under the
        // centre line
        assert!(w.player.x < 160.0);
        w.step(hold(0));
        assert!((w.player.x - 163.0).abs() < 1e-4);
        assert!((w.player.y - 47.9).abs() < 1e-4);
        let mut out = None;
        for _ in 0..80 {
            w.step(hold(0));
            if cues(&w).iter().any(|c| c.kind == CueKind::Out) {
                out = Some((w.player.x, w.player.y));
                break;
            }
        }
        // 8 px past the mouth, set down by half the small body (16): centred on the line, and
        // falling that frame
        let (x, y) = out.expect("came out");
        assert_eq!(x, 328.0);
        assert!((y - 39.65).abs() < 1e-4, "{y}");
        assert_eq!(w.player.control, Control::Normal);
        run_until(&mut w, 0, 30, |w| w.player.on_ground);
        assert_eq!(w.player.y, 32.0);
    }

    #[test]
    fn a_bent_pipe_turns_the_player_and_lets_go_on_top_of_the_mouth() {
        // right 4 tiles from (10, 3), then a bend and up 4: the end mouth faces up
        let mut w = course(&[((10, 3), &[(0, 4), (2, 4)])]);
        let pts = w.level.clear_pipe_paths.pipes[0].clone();
        assert_eq!(
            pts,
            [(160.0, 48.0), (224.0, 48.0), (240.0, 64.0), (240.0, 128.0)]
        );
        w.player.x = 130.0;
        run_until(&mut w, input::RIGHT, 200, |w| w.cpipe.player.is_some());
        let mut rolls = Vec::new();
        for _ in 0..80 {
            w.step(hold(0));
            if let Some(r) = w.cpipe.player {
                rolls.push(r.roll);
            } else {
                break;
            }
        }
        // level, then eased through the bend 10° a frame, then +90° (facing right, going up)
        assert_eq!(rolls[0], 0);
        assert!(rolls.contains(&0x071c_71c8));
        assert_eq!(*rolls.last().unwrap(), 0x4000_0000);
        assert!(w.cpipe.player.is_none());
        // an upward exit: the feet at the mouth's top, where it stands on the pipe
        assert_eq!((w.player.x, w.player.y), (240.0, 128.0));
        run_until(&mut w, 0, 10, |w| w.player.on_ground);
        assert_eq!(w.player.y, 128.0);
    }

    #[test]
    fn a_blocked_exit_turns_the_player_back() {
        let mut w = course(&[((10, 3), &[(0, 10)])]);
        // a block in front of the far mouth's lower row
        w.level.set(20, 2, crate::level::Cell::Solid);
        w.player.x = 130.0;
        run_until(&mut w, input::RIGHT, 200, |w| w.cpipe.player.is_some());
        run_until(&mut w, 0, 100, |w| {
            w.cpipe.player.is_some_and(|r| r.t.paused)
        });
        let far = w.player.x;
        assert!(far > 300.0 && far < 320.0, "{far}");
        run_until(&mut w, 0, 200, |w| w.cpipe.player.is_none());
        // back out of the near mouth, 8 px past it
        assert_eq!(w.player.x, 152.0);
        assert_eq!(w.player.control, Control::Normal);
    }

    #[test]
    fn a_goomba_walks_in_rides_through_and_comes_out_walking() {
        let mut w = course(&[((10, 3), &[(0, 10)])]);
        w.player.x = 40.0;
        // walking left into the far (right) mouth
        let mut g = Actor::new(Kind::Goomba, 340.0, 32.0);
        g.dir = -1;
        w.add_actor(g);
        let id = w.actors.last().unwrap().id;
        let goomba = |w: &World| w.actors.iter().find(|a| a.id == id).cloned();
        run_until(&mut w, 0, 400, |w| {
            goomba(w).is_some_and(|a| a.mode == Mode::ClearPipe)
        });
        assert!(w.cpipe.cues.iter().any(|c| c.actor == Some(id)));
        // Kuribo3W rides 8 px under the centre line (its rows' offset y −8), rolled along the pipe
        // (from the frame after it goes in)
        w.step(hold(0));
        let a = goomba(&w).unwrap();
        assert!((a.y - 40.0).abs() < 1e-4, "{}", a.y);
        let r = w.cpipe.riders[0];
        assert_eq!(r.roll, 0x4000_0000, "heading left: a quarter turn");
        run_until(&mut w, 0, 100, |w| {
            goomba(w).is_some_and(|a| a.mode == Mode::Normal)
        });
        let a = goomba(&w).unwrap();
        // launched 4 px a frame out of the near mouth
        assert!(a.x <= 152.0 && a.vx < -3.0, "{} {}", a.x, a.vx);
        run_until(&mut w, 0, 60, |w| !w.cpipe.carries(id));
        let a = goomba(&w).unwrap();
        assert!(a.alive() && a.x < 152.0);
        run_until(&mut w, 0, 60, |w| goomba(w).is_some_and(|a| a.on_ground));
        assert_eq!(goomba(&w).unwrap().y, 32.0);
    }

    #[test]
    fn pipes_joined_end_to_end_are_one_ride() {
        // two 5-tile runs, the second's start mouth on the first's end mouth (x 240)
        let mut w = course(&[((10, 3), &[(0, 5)]), ((15, 3), &[(0, 5)])]);
        w.player.x = 130.0;
        run_until(&mut w, input::RIGHT, 200, |w| w.cpipe.player.is_some());
        let mut outs = 0;
        for _ in 0..100 {
            w.step(hold(0));
            outs += cues(&w).iter().filter(|c| c.kind == CueKind::Out).count();
            if w.cpipe.player.is_none() {
                break;
            }
        }
        assert_eq!(outs, 1);
        assert_eq!(w.player.x, 328.0);
    }

    #[test]
    fn down_into_an_upward_mouth_and_out_of_the_bottom() {
        // a pipe going down 6 tiles from base tile (11, 12) (columns 10–11, rows 7–12): its top
        // mouth faces up at (176, 208), its bottom one down at (176, 112), 80 px over the ground
        let mut w = course(&[((11, 12), &[(3, 6)])]);
        let pts = w.level.clear_pipe_paths.pipes[0].clone();
        assert_eq!(pts, [(176.0, 208.0), (176.0, 112.0)]);
        (w.player.x, w.player.y) = (172.0, 208.0);
        run_until(&mut w, 0, 10, |w| w.player.on_ground);
        assert_eq!(w.player.y, 208.0, "stands on the mouth");
        run_until(&mut w, input::DOWN, 5, |w| w.cpipe.player.is_some());
        // heading down: rolled −90° (facing right), lets go 8 px under the mouth
        w.step(hold(0));
        assert_eq!(w.cpipe.player.unwrap().roll, 0xc000_0000);
        run_until(&mut w, 0, 60, |w| w.cpipe.player.is_none());
        assert_eq!(w.player.x, 176.0);
        assert!(
            (w.player.y - (112.0 - 8.1 - 0.25)).abs() < 1e-4,
            "{}",
            w.player.y
        );
        run_until(&mut w, 0, 60, |w| w.player.on_ground);
        assert_eq!((w.player.x, w.player.y), (176.0, 32.0));
    }

    #[test]
    fn a_mouth_onto_the_ground_sends_the_player_back_up() {
        // the same pipe standing on the ground: its bottom mouth is blocked
        let mut w = course(&[((11, 6), &[(3, 5)])]);
        assert_eq!(w.level.clear_pipe_paths.pipes[0][1], (176.0, 32.0));
        (w.player.x, w.player.y) = (176.0, 112.0);
        run_until(&mut w, 0, 10, |w| w.player.on_ground);
        run_until(&mut w, input::DOWN, 5, |w| w.cpipe.player.is_some());
        run_until(&mut w, 0, 200, |w| w.cpipe.player.is_some_and(|r| r.t.back));
        run_until(&mut w, 0, 200, |w| w.cpipe.player.is_none());
        // back out of the top: an upward exit, on the mouth
        assert_eq!((w.player.x, w.player.y), (176.0, 112.0));
    }

    #[test]
    fn an_item_is_drawn_shrunk_and_the_player_rolled_inside() {
        let mut w = course(&[((10, 3), &[(2, 6)])]);
        // a pipe going up from tile (10, 3): its bottom mouth faces down at (176, 48)
        assert_eq!(w.level.clear_pipe_paths.pipes[0][0], (176.0, 48.0));
        let t = Traveller::enter(&w.level.clear_pipe_paths, 0, false, (0.0, PLAYER_OFF_Y));
        w.player.control = Control::Demo;
        (w.player.x, w.player.y) = t.pos();
        w.cpipe.player = Some(PlayerRide {
            t,
            roll: 0x4000_0000,
        });
        let mut s = w.player_sprite().unwrap();
        w.clear_pipe_player_pose(&mut s);
        assert_eq!(
            (s.roll, s.layer),
            (90.0, super::super::CLEAR_PIPE_LAYER + 1)
        );
        assert_eq!(s.y + s.h * 0.5, 48.0, "centred on the line");
        assert_eq!(profile(Kind::Mushroom, false).unwrap().rotate, Rotate::Item);
        assert_eq!(
            profile(Kind::Koopa { red: false }, false)
                .unwrap()
                .travel_off_y(),
            -14.0
        );
        assert!(
            profile(Kind::Buzzy, false).is_some()
                && profile(
                    Kind::Thwomp {
                        side: false,
                        right: false
                    },
                    false
                )
                .is_none()
        );
    }
}
