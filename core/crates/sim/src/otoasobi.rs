//! Placed sound effects, the game's "Otoasobi" (research/specs/sound-effect-parts.md, private):
//! what each of the course's effects plays, what sets it off, and the music they change.
//!
//! A course keeps them in three places: each area's spot records (`Area::sound_effects`: an
//! effect on an empty tile), a part's `sound_effect_id` (an effect put on the part) and the
//! header's `player_sound_effect` (the effect put on Mario at the start). The id is the effect's
//! course id 0–55, the number of its sound and picture users `Otoasobi<id>` (47 of them are
//! offered; 21, 22, 27, 28, 33, 34, 39, 40 and 55 are left over from development) [obs v3.0.3:
//! the user names built in 0x71016aac20, the icon → id table 0x71023ca370].
//!
//! [`SoundParts`] watches the world after every tick (no hooks in the world's own update: it
//! reads the state the world exposes, like the host's sound and effect directors) and lists the
//! effects set off this tick ([`SoundParts::cues`]) and the music an effect has put on
//! ([`SoundParts::music`]). The host plays them: the sound (`cue`, or the music keys), the
//! picture (the effect user's `cue`).
//!
//! What sets an effect off [obs v3.0.3, the callers of the play function 0x7101537c40]:
//! - a spot: a player's box reaching the spot's centre (each entry, re-armed on leaving)
//!   (area update 0x7100e443c0);
//! - a part: what the part does: an enemy defeated, a coin or an item taken, a block hit, a
//!   Trampoline jumped on, a pipe or a door entered (see [`PartTrigger`]);
//! - Mario: the start of play, and one action chosen by the effect: a running jump, damage, a
//!   fall out of the view, a power-up, getting in a Shoe / Yoshi / Clown Car / Koopa Troopa Car,
//!   a skid (see [`PlayerTrigger`], 0x71015d4900–0x71015d4fa0).

use std::collections::{BTreeMap, BTreeSet};

use mmk_formats::smm2::{Area, Course, CourseObject};

use crate::World;
use crate::actors::Kind;
use crate::level::{Level, TILE};
use crate::world::{Event, VIEW_H, VIEW_W};

/// Course ids 0–55.
pub const IDS: usize = 56;

/// How an effect's sound is voiced [obs: the effect table 0x710294f468, word +4; music slot
/// word +0x10].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// One of 16 shared voices: a second start of the same effect in one frame reuses the
    /// first's; with all 16 busy a random one is taken (0x7101537c40 case 0).
    Shared,
    /// The one long-sound voice (Ninja Attack!, Scatting, Traditional): a new long sound
    /// replaces the old one, except within 5 frames of its start (case 1).
    Long,
    /// Its own voice: a new start restarts it, or (`hold`) waits while it still plays (case 4).
    Own { hold: bool },
    /// Replaces the course music with the effect user's `cue_bgm` (`cue_bgm_hurry` after
    /// hurry-up); the slot of the music table 0x7102c17050 (case 2).
    Music(u8),
    /// A sound in the music's place (Heartbeat, Silence, Peaceful, Horror): the user's `cue` over
    /// the course music its group ducks (case 3).
    Ambient(u8),
}

/// The effect table, by course id [obs 0x710294f468].
pub const VOICES: [Voice; IDS] = {
    use Voice::*;
    const O: Voice = Own { hold: false };
    const H: Voice = Own { hold: true };
    [
        O,
        O,
        Shared,
        Shared,
        O,
        O,
        Shared,
        Shared,
        Shared,
        Music(2), // 0–9
        Ambient(0),
        O,
        O,
        Shared,
        Shared,
        O,
        Shared,
        Shared,
        Shared,
        Music(3), // 10–19
        Ambient(1),
        O,
        O,
        Shared,
        Shared,
        O,
        Shared,
        Shared,
        Shared,
        Shared, // 20–29
        O,
        O,
        Long,
        Shared,
        Shared,
        H,
        Shared,
        Shared,
        Shared,
        Shared, // 30–39
        Shared,
        O,
        Long,
        Shared,
        Long,
        H,
        H,
        H,
        Music(4),
        Ambient(5), // 40–49
        Ambient(6),
        Music(7),
        Music(8),
        Music(9),
        Music(10),
        Shared, // 50–55
    ]
};

pub use mmk_formats::smm2::sound_effects::{
    CATEGORY_KEYS, ICON_IDS, ICON_LABELS, PAGES, icon, label, page_of, sheet_cell, valid,
};

/// The sound and picture user: `Otoasobi<id>`, or its Link / SMB2 version where the data has
/// one (8, 9, 18, 19, 48, 49: the Master Sword's and the SMB2 Mushroom's music and sounds)
/// [obs: masks 0x30000000c0300 / 0x3000000080200 in 0x71016aac20; which player state picks them
/// [inf]].
pub fn user(id: u8, variant: Variant) -> String {
    let base = format!("Otoasobi{id:02}");
    let has = |mask: u64| id < 64 && mask >> id & 1 != 0;
    match variant {
        Variant::Link if has(0x3_0000_000c_0300) => format!("{base}_Link"),
        Variant::Usa if has(0x3_0000_0008_0200) => format!("{base}_USA"),
        _ => base,
    }
}

/// Which version of an effect's user plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    Plain,
    /// Link (the SMB1 Master Sword).
    Link,
    /// SMB2 Mario (the SMB2 Mushroom).
    Usa,
}

/// Effects that change the sound of everything (Twisty Turny, Woozy): an audio filter on the
/// whole mix while they last. The play call 0x7101537c40 raises the audio manager's request bits
/// (+0xa62 bit 0 / bit 2, timers +0xa84 / +0xa88); its update 0x71017b15d0 inserts the
/// `OtoAsobi` bus's effects (`Otoasobi_Delay` + `OtoAsobi_Filter`, or `OtoAsobi_Vib` +
/// `OtoAsobi_Mod`) and runs them for [`FILTER_BEATS`] beats of the course music; the host's
/// mixer does the sound of it ([`Filter`]).
pub fn audio_filter(id: u8) -> bool {
    matches!(id, 46 | 47)
}

/// An audio filter runs for this many beats of the course music's tempo: frames = ⌈16 · 60 /
/// (bpm / 60)⌉ (the timers +0xa84 / +0xa88, 0x71017b15d0; 60.0 at 0x7102318f6c) [obs v3.0.3].
pub const FILTER_BEATS: f32 = 16.0;
/// The tempo when the course's isn't known: the audio manager's default, 120 bpm
/// (0x71023194b4) [obs v3.0.3].
pub const DEFAULT_BPM: f32 = 120.0;

/// Frames an audio filter lasts at `bpm`.
pub fn filter_frames(bpm: f32) -> u32 {
    let bpm = if bpm > f32::EPSILON { bpm } else { DEFAULT_BPM };
    (60.0 / (bpm / 60.0) * FILTER_BEATS).ceil() as u32
}

/// An audio filter in play: Twisty Turny (46) or Woozy (47), the frames it has left, and the
/// frames since it began (the host runs the filter's own clock off these).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub id: u8,
    pub left: u32,
    pub age: u32,
}

/// The action that sets off an effect put on Mario (besides the start of play, which sets off
/// every one) [obs: the id masks of 0x71015d4900–0x71015d4fa0 and their callers].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerTrigger {
    /// A jump at a running speed (|speedF| ≥ 3.0) (0x71015d4bc0 from the jump states
    /// 0x71015c8a60).
    Jump,
    /// Damage or a miss (0x71015d4900 from 0x71015a4990 and the miss states).
    Damage,
    /// A miss that drops below the view (feet 32 px under its bottom), once a miss
    /// (0x71015d4a20).
    Fall,
    /// A power-up taken (0x71015d4d60 from the form change 0x71015a8990, the wearables
    /// 0x71015c53e0).
    PowerUp,
    /// Getting in a Goomba's Shoe, on Yoshi, in a Clown Car or a Koopa Troopa Car
    /// (0x71015d4e80: `kutsuIn`, `car_ride`).
    Ride,
    /// A skid (0x71015d4fa0 from the turn state 0x71015f36d0).
    Turn,
}

pub fn player_trigger(id: u8) -> Option<PlayerTrigger> {
    let has = |mask: u64| id < 64 && mask >> id & 1 != 0;
    if has(0x0820_0081_2024) {
        Some(PlayerTrigger::Jump)
    } else if has(0x8448_0200_0841) {
        Some(PlayerTrigger::Damage)
    } else if has(0x8100_1002) {
        Some(PlayerTrigger::Fall)
    } else if has(0x30_0000_4008) {
        Some(PlayerTrigger::PowerUp)
    } else if has(0x7200_4000_0000) {
        Some(PlayerTrigger::Ride)
    } else if id == 29 || id == 32 {
        Some(PlayerTrigger::Turn)
    } else {
        None
    }
}

/// The jump trigger's speed: the top bracket of the jump table, 3.0 px/frame (4.0 under a
/// player flag, +0x27d4, not modelled) [obs 0x710231c3cc].
pub const JUMP_SPEED: f32 = 3.0;

/// What a part does to set off its effect, by the kind of part [obs v3.0.3: the callers of the
/// play function 0x7101537c40 and their trigger kinds]:
/// - enemies: their action through 0x710128ef80 (kind 1; the defeat for most, from the enemy
///   base 0x7101286270), music through the source registry 0x7101538eb0 while alive
///   (0x710128b250);
/// - coins and items taken (0x71013a1c20, 0x710134ffd0, 0x71013ceb30), blocks hit
///   (0x710138a430…0x710138c690), springs and Mushroom Trampolines bounced (0x71013b9ef0,
///   0x71013db8f0, 0x71013f0260), doors (0x71013a5a00, 0x71013a6710) and pipes (the warp
///   sequence 0x71012d95f0) entered, a warp box left (0x71013f2060): kind 5;
/// - a Key collected (0x71013bf530), the claw catching (0x71013e87c0): kind 4;
/// - lifts and the P Switch stood on (0x7100d84810 from 0x71013c1bf0…0x71013c6930 and
///   0x71013d7d90): kind 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartTrigger {
    /// Defeated (stomped, knocked out, gone in a puff).
    Defeated,
    /// Taken by a player (coins, power-ups), or picked up (the P Switch, 0x71013e99a0).
    Taken,
    /// Hit from below or the side, or broken.
    Hit,
    /// Bounced on (springs, Mushroom Trampolines).
    Bounced,
    /// Entered (pipes, doors).
    Entered,
    /// Stood on (lifts: each landing, 8 frames apart; the P Switch: a landing 25 frames after
    /// the last).
    Ridden,
    /// A Key collected.
    Collected,
    /// A Bill Blaster's shot (0x71011406f0 → 0x710128ef80 at the muzzle).
    Shot,
    /// The Swinging Claw catching.
    Caught,
    /// A warp box left: the exit box's effect as the player comes out.
    Out,
    /// A Koopa Clown Car, Koopa Troopa Car or Lakitu's Cloud got into (the car's rider check
    /// 0x7101173ec0, the Clown's "ridden" 0x7101187ce0, the Cloud's 0x7101127d40): kind 1.
    Boarded,
    /// A Yoshi's Egg hatched (the egg's 0x71013f9050 after it lets Yoshi out, 0x71013f8cd0):
    /// kind 5.
    Hatched,
    /// A ! Block hit: each of its bound states (0x71013924e0…0x7101393090): kind 4.
    Bound,
    /// Set off by nothing it does: the POW's and the Crate's classes (0x71013d3dd0…0x71013d6ad0,
    /// 0x71013f5bb0…0x71013f7610) reach no play call; only their carry base's music source
    /// (0x710135a900 → 0x710128b250) plays a music effect while they are there [obs v3.0.3: their
    /// vtables' callees eight calls deep; captures `pow-effect`, `crate-effect`].
    Inert,
}

impl PartTrigger {
    /// The play call's trigger kind (the music's range and end rules, [`Trigger`]).
    fn trigger(self) -> Trigger {
        match self {
            PartTrigger::Defeated | PartTrigger::Shot | PartTrigger::Boarded => Trigger::Enemy,
            PartTrigger::Collected | PartTrigger::Caught | PartTrigger::Bound => Trigger::Held,
            PartTrigger::Ridden => Trigger::Ridden,
            _ => Trigger::Object,
        }
    }

    /// Parts whose music effect plays while the part is there, from the music source registry
    /// (0x7101538eb0): enemies and items while alive (0x710128b250), a stood-on lift or P Switch
    /// once it has been stood on (0x71013d7d90).
    fn music_source(self) -> bool {
        matches!(
            self,
            PartTrigger::Defeated | PartTrigger::Taken | PartTrigger::Ridden | PartTrigger::Inert
        )
    }
}

/// The play call's trigger kind (its 6th argument), which sets how far a music effect reaches
/// [obs 0x7101537ab0, 0x7100e443c0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// 0: Mario's own effect.
    Player,
    /// 1: an enemy's (and a part's music source).
    Enemy,
    /// 3: a spot's.
    Spot,
    /// 4: a held or collected object's (Key, claw, Icicle, ! Block).
    Held,
    /// 5: an object's.
    Object,
    /// 6: a stood-on object's (lifts, P Switch).
    Ridden,
}

impl Trigger {
    /// The play call's kind number.
    pub fn kind(self) -> u8 {
        match self {
            Trigger::Player => 0,
            Trigger::Enemy => 1,
            Trigger::Spot => 3,
            Trigger::Held => 4,
            Trigger::Object => 5,
            Trigger::Ridden => 6,
        }
    }

    /// A music effect from this source starts only within this distance of the players
    /// (0x7101537ab0): kinds 1 / 4 / 6 within 200 px (0x7102319208), 5 within 160
    /// (0x7102318710); Mario's and a spot's always.
    pub fn start_range(self) -> f32 {
        match self {
            Trigger::Enemy | Trigger::Held | Trigger::Ridden => 200.0,
            Trigger::Object => 160.0,
            Trigger::Player | Trigger::Spot => f32::INFINITY,
        }
    }

    /// The music ends when the players get farther than this from its source (0x7100e443c0):
    /// 160 px for a spot, 180 for an object (0x7102319060), 304 for kinds 1 / 4 / 6
    /// (0x71023189d8); Mario's lasts until the area changes or the run ends.
    pub fn end_range(self) -> f32 {
        match self {
            Trigger::Spot => 160.0,
            Trigger::Object => 180.0,
            Trigger::Enemy | Trigger::Held | Trigger::Ridden => 304.0,
            Trigger::Player => f32::INFINITY,
        }
    }
}

/// The part kinds whose effect the engine sets off, by their course object.
fn part_trigger(o: &CourseObject, lvl: &Level) -> Option<PartTrigger> {
    let actorish = o.flags & 0x2 != 0 || o.flags & 0x400 != 0;
    // blocks, on the grid or as moving blocks (wings, tracks: hit through the blocks' code
    // 0x710138a430…)
    if crate::level::block_kind(o.id).is_some() {
        return Some(PartTrigger::Hit);
    }
    match o.id {
        // coins and pink coins on the grid; a coin with actor flags is the Coin actor
        8 | 92 if !actorish && o.flags & 0x8000 == 0 && o.flags & 1 == 0 => {
            return Some(PartTrigger::Taken);
        }
        8 if actorish => return Some(PartTrigger::Taken),
        // pipes and doors
        9 | 55 => return Some(PartTrigger::Entered),
        // a Bill Blaster or Cannon on the grid shoots (a winged one flies as an actor: not
        // yet); the Cannon's "shoot" calls 0x710128ef80 too (0x710125ed90)
        13 | 47 if !actorish => return Some(PartTrigger::Shot),
        97 => return Some(PartTrigger::Out),
        // the Dash Block (0x71013a3f30) and snake blocks (0x71013e1490) are stood on (kind 6)
        80 | 84 if !actorish => return Some(PartTrigger::Ridden),
        119 => return Some(PartTrigger::Bound),
        _ => {}
    }
    let kind = crate::level::part_kind(o, lvl)?;
    match kind {
        // the ON/OFF Trampoline is the Trampoline's class (0x71013eebb0)
        Kind::Spring
        | Kind::SideSpring
        | Kind::Trampoline { .. }
        | Kind::OnOffTrampoline { .. } => Some(PartTrigger::Bounced),
        Kind::ClownCar { .. } | Kind::KoopaCar | Kind::Cloud => Some(PartTrigger::Boarded),
        Kind::YoshiEgg { .. } => Some(PartTrigger::Hatched),
        Kind::Lift { .. } | Kind::LavaLift { .. } | Kind::PSwitch => Some(PartTrigger::Ridden),
        Kind::Key => Some(PartTrigger::Collected),
        Kind::Claw => Some(PartTrigger::Caught),
        // the POW's and the Crate's classes (0x71013d3dd0…, 0x71013f5bb0…) never read their
        // effect
        Kind::Pow | Kind::Crate { .. } => Some(PartTrigger::Inert),
        Kind::BigCoin { .. } | Kind::FloatCoin => Some(PartTrigger::Taken),
        k if k.is_item() => Some(PartTrigger::Taken),
        k if defeatable(k) => Some(PartTrigger::Defeated),
        _ => None,
    }
}

/// Kinds that can be defeated (their defeat is what sets their effect off).
fn defeatable(k: Kind) -> bool {
    k.is_enemy()
        || matches!(
            k,
            Kind::Spiny
                | Kind::Buzzy
                | Kind::SpikeTop { .. }
                | Kind::HammerBro
                | Kind::BobOmb
                | Kind::DryBones
                | Kind::Boo
                | Kind::Lakitu
                | Kind::CheepCheep { .. }
                | Kind::Blooper
                | Kind::Magikoopa
                | Kind::RockyWrench
                | Kind::Wiggler
                | Kind::ShoeGoomba { .. }
                | Kind::MontyMole
                | Kind::ChainChomp { .. }
                | Kind::FishBone
                | Kind::AntTrooper { .. }
                | Kind::Stingby
                | Kind::Skipsqueak { .. }
                | Kind::Porcupuffer
                | Kind::Bully
                | Kind::Pokey { .. }
                | Kind::Mechakoopa { .. }
                | Kind::Koopaling { .. }
                | Kind::Spike
                | Kind::Thwomp { .. }
                // a Muncher and a Charvaargh are enemies of the enemy base (0x7101286270),
                // defeated by a Super Star
                | Kind::Muncher
                | Kind::Charvaargh
        )
}

/// The course's effects the engine doesn't play, for `mmk coverage`: on a part whose action
/// isn't modelled (`sound effect on part <id>`).
pub fn unsupported(course: &Course) -> Vec<String> {
    let mut out = BTreeSet::new();
    let style = course.header.game_style.as_str_lossy();
    let mut probe = Level::flat(1);
    probe.style = style;
    for area in course.areas() {
        probe.theme = area.header.theme;
        for o in area.objects.iter() {
            if o.sound_effect().is_none() {
                continue;
            }
            if part_trigger(o, &probe).is_none() {
                out.insert(format!("sound effect on part {}", o.id));
            }
        }
    }
    out.into_iter().collect()
}

/// Who set an effect off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Player `n` (0 = player 1).
    Player(u8),
    Spot,
    Part(PartTrigger),
}

impl Source {
    /// The play call's trigger kind for this source.
    pub fn trigger(self) -> Trigger {
        match self {
            Source::Player(_) => Trigger::Player,
            Source::Spot => Trigger::Spot,
            Source::Part(t) => t.trigger(),
        }
    }
}

/// An effect set off this tick: its course id and where (world pixels, the source's feet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub source: Source,
}

/// A music effect in play: its course id, its slot (`Voice::Music` / `Voice::Ambient`), where
/// its source is and by what it was set off; `actor`: the part it follows (a music source of
/// the registry), `dist` its distance from the players when it took the music.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Music {
    pub id: u8,
    pub slot: u8,
    pub ambient: bool,
    pub x: f32,
    pub y: f32,
    pub trigger: Trigger,
    pub actor: Option<u32>,
    pub dist: f32,
}

/// A spot: its effect, its centre (px) and which players are inside its box.
#[derive(Clone, Debug, PartialEq)]
struct Spot {
    id: u8,
    x: f32,
    y: f32,
    inside: u8,
}

/// One area's effects.
#[derive(Clone, Debug, Default, PartialEq)]
struct AreaParts {
    spots: Vec<Spot>,
    /// Parts on the grid (blocks, coins, pipes, doors): tile → (effect, trigger).
    tiles: BTreeMap<(i32, i32), (u8, PartTrigger)>,
    /// Placed actors: their spawn's place (feet centre, 1/10 px) → (effect, trigger).
    actors: BTreeMap<(i32, i32), (u8, PartTrigger)>,
    /// Bill Blasters and warp boxes: their place (bottom centre, 1/10 px) → effect.
    blasters: BTreeMap<(i32, i32), u8>,
    boxes: BTreeMap<(i32, i32), u8>,
    /// Snake blocks and ! Blocks by their tile (the level's records), Cannons by their place.
    snakes: BTreeMap<(i32, i32), u8>,
    exclams: BTreeMap<(i32, i32), u8>,
    cannons: BTreeMap<(i32, i32), u8>,
    /// The area's tempo (bpm): the course music's, normal and hurried.
    tempo: [f32; 2],
}

impl AreaParts {
    fn of(area: &Area, lvl: &Level) -> Self {
        let mut a = AreaParts {
            tempo: [DEFAULT_BPM; 2],
            ..Default::default()
        };
        for s in area.sound_effects.iter() {
            if valid(s.id) {
                a.spots.push(Spot {
                    id: s.id,
                    x: s.x as f32 * TILE + 8.0,
                    y: s.y as f32 * TILE + 8.0,
                    inside: 0,
                });
            }
        }
        for o in area.objects.iter() {
            let Some(id) = o.sound_effect().filter(|&i| valid(i)) else {
                continue;
            };
            let Some(t) = part_trigger(o, lvl) else {
                continue;
            };
            let (ox, oy) = (o.x as f32 / 10.0, o.y as f32 / 10.0);
            let (tx, ty) = (((ox - 8.0) / TILE).round(), ((oy - 8.0) / TILE).round());
            match t {
                PartTrigger::Shot => {
                    // the level's column / block: x the tile centre, y its bottom (level.rs)
                    let at = spawn_key(tx * TILE + 8.0, ty * TILE);
                    if o.id == 47 {
                        a.cannons.insert(at, id);
                    } else {
                        a.blasters.insert(at, id);
                    }
                    continue;
                }
                PartTrigger::Ridden if o.id == 84 => {
                    a.snakes.insert((tx as i32, ty as i32), id);
                    continue;
                }
                PartTrigger::Bound => {
                    a.exclams.insert((tx as i32, ty as i32), id);
                    continue;
                }
                PartTrigger::Out => {
                    a.boxes.insert(spawn_key(ox, oy - 8.0), id);
                    continue;
                }
                _ => {}
            }
            let actorish = o.flags & 0x2 != 0 || o.flags & 0x400 != 0;
            let grid = (matches!(t, PartTrigger::Hit | PartTrigger::Entered) && !actorish)
                || (t == PartTrigger::Ridden && o.id == 80)
                || (t == PartTrigger::Taken && matches!(o.id, 8 | 92) && !actorish);
            if grid {
                let tx = ((ox - 8.0) / TILE).round() as i32;
                let ty = ((oy - 8.0) / TILE).round() as i32;
                let (w, h) = (o.width.max(1) as i32, o.height.max(1) as i32);
                // a pipe's mouth is at the end of its length (any of its tiles counts)
                let (w, h) = if o.id == 9 {
                    (w.max(2), h.max(2))
                } else {
                    (w, h)
                };
                for dx in 0..w {
                    for dy in 0..h {
                        a.tiles.insert((tx + dx, ty + dy), (id, t));
                    }
                }
            } else {
                a.actors.insert(spawn_key(ox, oy - 8.0), (id, t));
            }
        }
        a
    }
}

fn spawn_key(x: f32, y: f32) -> (i32, i32) {
    ((x * 10.0).round() as i32, (y * 10.0).round() as i32)
}

/// A player as of the last tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PlayerSeen {
    alive: bool,
    on_ground: bool,
    form: u8,
    riding: bool,
    skid: bool,
    /// The miss's fall cue went (0x71015d4a20's once flag, player +0x2d3c).
    fell: bool,
    /// The carried actor.
    held: Option<u32>,
}

/// A tagged actor as of the last tick.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tagged {
    id: u8,
    t: PartTrigger,
    x: f32,
    y: f32,
    /// Taken (items), ridden (springs, lifts), pushing (sideways springs) last tick.
    state: u8,
    /// Frames until a stood-on part sets off again (lifts 8, the P Switch 25).
    cool: u32,
    /// Stood on at least once (a stood-on part's music source registers from then on).
    armed: bool,
}

/// Lifts set off once a landing, 8 frames apart (+0x514, 0x71013c1bf0…); the P Switch on a
/// landing 25 frames after it was last stood on (+0x8e8, 0x71013d7d90) [obs v3.0.3].
const LIFT_COOL: u32 = 8;
const P_SWITCH_COOL: u32 = 25;

/// The voices the effects share, as of their start frames (the repeat rules of [`Voice`]).
#[derive(Clone, Debug, Default, PartialEq)]
struct Voices {
    /// Shared voices started this frame: effects.
    shared: Vec<u8>,
    /// The long voice: effect, start tick.
    long: Option<(u8, u64)>,
    /// Own voices: effect → (start tick, the tick the voice ends).
    own: BTreeMap<u8, (u64, u64)>,
}

/// How long an own voice counts as playing for the `hold` rule when nobody has said how long
/// its sound is: the game asks the voice itself (its handle's vtable[0], 0x7101537c40 case 4);
/// the host that plays it tells [`SoundParts::voice_ends`] the sound's real end, this covers the
/// longest of Twisty Turny's / Woozy's scratches (`Otoasobi_DJ03`, 2.0 s) until then.
const HOLD_FRAMES: u64 = 120;

/// What one copy of [`SoundParts`] tells another (the sound director, which knows the course's
/// tempo and how long its voices play, to the picture director's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sync {
    pub tempo: Vec<[f32; 2]>,
    pub voices: Vec<(u8, u64, u64)>,
}

/// Watches the world for the course's effects; step it once per world tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundParts {
    areas: Vec<AreaParts>,
    player_effect: Option<u8>,
    /// This tick's effects and the music effect in play.
    pub cues: Vec<Cue>,
    pub music: Option<Music>,
    /// The audio filters in play (Twisty Turny, Woozy) and those that ran out this tick (their
    /// user's `end` plays, their picture stops; 0x7100e443c0).
    pub filters: Vec<Filter>,
    pub filters_ended: Vec<u8>,
    /// A player (re)entered this tick: the start of the run, a respawn (the player's first
    /// states 144 / 145 set +0x490 bit 23, which Woozy's warble follows, 0x71017b15d0).
    pub entered: bool,
    started: bool,
    last_tick: u64,
    area: usize,
    players: Vec<PlayerSeen>,
    /// Tagged actors alive last tick: actor id → state.
    actors: BTreeMap<u32, Tagged>,
    /// Tagged blocks last tick: tile → bumping.
    blocks: BTreeMap<(i32, i32), bool>,
    /// Tagged coins on the grid last tick.
    coins: BTreeSet<(i32, i32)>,
    coin_count: u32,
    /// Shots already seen, the claw holding the player last tick, the warp-box transit.
    shots: BTreeSet<u32>,
    claw: Option<u32>,
    boxed: Option<(f32, f32)>,
    /// Stood-on grid parts and snake blocks last tick (stood on, frames until they set off
    /// again); ! Blocks bound last tick.
    stood: BTreeMap<(i32, i32), (bool, u32)>,
    snakes_on: BTreeMap<usize, (bool, u32)>,
    exclams_bound: BTreeSet<usize>,
    voices: Voices,
}

impl SoundParts {
    /// The effects of `course` (both areas).
    pub fn new(course: &Course) -> Self {
        let style = course.header.game_style.as_str_lossy();
        let areas = course
            .areas()
            .iter()
            .map(|a| {
                let mut lvl = Level::flat(1);
                (lvl.style, lvl.theme) = (style.clone(), a.header.theme);
                AreaParts::of(a, &lvl)
            })
            .collect();
        Self {
            areas,
            player_effect: course.header.player_effect().filter(|&i| valid(i)),
            ..Default::default()
        }
    }

    /// The course has effects at all.
    pub fn any(&self) -> bool {
        self.player_effect.is_some()
            || self.areas.iter().any(|a| {
                !a.spots.is_empty()
                    || !a.tiles.is_empty()
                    || !a.actors.is_empty()
                    || !a.blasters.is_empty()
                    || !a.boxes.is_empty()
                    || !a.snakes.is_empty()
                    || !a.exclams.is_empty()
                    || !a.cannons.is_empty()
            })
    }

    /// Area `area`'s course music tempo, normal and hurried (the audio manager's table of
    /// `Mush/BGMList.byml`, by style and theme: 0x71017b15d0 reads `+0xa58[style · 20 + theme ·
    /// 2 + hurry]`).
    pub fn set_tempo(&mut self, area: usize, bpm: [f32; 2]) {
        if let Some(a) = self.areas.get_mut(area) {
            a.tempo = bpm;
        }
    }

    /// The tempos, for another copy ([`Self::set_tempo`]).
    pub fn tempos(&self) -> Vec<[f32; 2]> {
        self.areas.iter().map(|a| a.tempo).collect()
    }

    /// The host that plays an effect's sound says when it ends: own voices that hold (35, 45,
    /// 46, 47) wait until then (the game asks the voice whether it still plays).
    pub fn voice_ends(&mut self, id: u8, end: u64) {
        if let Some(v) = self.voices.own.get_mut(&id) {
            v.1 = end;
        }
    }

    /// Own voices and when they end, for another copy ([`Self::set_voice_ends`]).
    pub fn voice_ends_all(&self) -> Vec<(u8, u64, u64)> {
        self.voices
            .own
            .iter()
            .map(|(&id, &(s, e))| (id, s, e))
            .collect()
    }

    /// Take the voices' ends another copy learned (only for voices started at the same tick).
    pub fn set_voice_ends(&mut self, ends: &[(u8, u64, u64)]) {
        for &(id, start, end) in ends {
            if let Some(v) = self.voices.own.get_mut(&id)
                && v.0 == start
            {
                v.1 = end;
            }
        }
    }

    /// Take what another copy knows ([`Sync`]).
    pub fn sync(&mut self, s: &Sync) {
        for (area, &bpm) in s.tempo.iter().enumerate() {
            self.set_tempo(area, bpm);
        }
        self.set_voice_ends(&s.voices);
    }

    fn view_has(world: &World, x: f32, y: f32) -> bool {
        let (x0, y0) = (world.camera_x, world.camera_y);
        x >= x0 && x <= x0 + VIEW_W && y >= y0 && y <= y0 + VIEW_H
    }

    /// The players' listening point: player 1, or the middle of everyone alive (0x7100e443c0).
    fn listener(world: &World) -> (f32, f32) {
        let mut n = 0.0;
        let (mut sx, mut sy) = (0.0, 0.0);
        for p in std::iter::once(&world.player).chain(&world.others) {
            if p.alive() {
                sx += p.x;
                sy += p.y + 8.0;
                n += 1.0;
            }
        }
        if n == 0.0 {
            (world.player.x, world.player.y + 8.0)
        } else {
            (sx / n, sy / n)
        }
    }

    fn dist(world: &World, x: f32, y: f32) -> f32 {
        let (lx, ly) = Self::listener(world);
        ((x - lx).powi(2) + (y - ly).powi(2)).sqrt()
    }

    /// Set off effect `id` at (x, y) by `source`; `forced`: no view test (spots, Mario's own
    /// effect in single player) [obs 0x7101537c40].
    fn fire(&mut self, world: &World, id: u8, x: f32, y: f32, source: Source, forced: bool) {
        if !valid(id) {
            return;
        }
        let trigger = source.trigger();
        let tick = world.tick;
        match VOICES[id as usize] {
            Voice::Music(_) | Voice::Ambient(_) => {
                if !forced && !Self::view_has(world, x, y) && trigger != Trigger::Spot {
                    return;
                }
                let d = Self::dist(world, x, y);
                if d >= trigger.start_range() {
                    return;
                }
                self.take_music(id, x, y, trigger, None, d);
            }
            voice => {
                if !forced && !Self::view_has(world, x, y) {
                    return;
                }
                match voice {
                    Voice::Shared => {
                        if self.voices.shared.contains(&id) {
                            return;
                        }
                        self.voices.shared.push(id);
                    }
                    Voice::Long => {
                        if let Some((old, t)) = self.voices.long
                            && matches!(old, 32 | 42)
                            && tick.saturating_sub(t) < 5
                        {
                            return;
                        }
                        self.voices.long = Some((id, tick));
                    }
                    Voice::Own { hold } => {
                        if let Some(&(t, end)) = self.voices.own.get(&id) {
                            if hold && tick < end {
                                // the voice and its picture wait; a filter still runs on
                                // (0x7101537c40 case 4 raises the request either way)
                                if audio_filter(id) {
                                    self.start_filter(world, id);
                                }
                                return;
                            }
                            if id == 41 && tick.saturating_sub(t) < 5 {
                                return;
                            }
                        }
                        self.voices.own.insert(id, (tick, tick + HOLD_FRAMES));
                    }
                    _ => {}
                }
                if audio_filter(id) {
                    self.start_filter(world, id);
                }
            }
        }
        self.cues.push(Cue { id, x, y, source });
    }

    /// A music effect takes the music (or a nearer source of the same takes it over).
    fn take_music(&mut self, id: u8, x: f32, y: f32, trigger: Trigger, actor: Option<u32>, d: f32) {
        let (slot, ambient) = match VOICES[id as usize] {
            Voice::Music(s) => (s, false),
            Voice::Ambient(s) => (s, true),
            _ => return,
        };
        if let Some(m) = &mut self.music
            && m.slot == slot
        {
            // the same music goes on; a nearer source takes over (0x7101538eb0)
            if d < m.dist - 2.0 {
                (m.x, m.y, m.trigger, m.actor, m.dist) = (x, y, trigger, actor, d);
            }
            return;
        }
        self.music = Some(Music {
            id,
            slot,
            ambient,
            x,
            y,
            trigger,
            actor,
            // a source that isn't the registry's leaves the distance open (+0xc08 = FLT_MAX)
            dist: if actor.is_some() { d } else { f32::MAX },
        });
    }

    /// A part's music effect as a source of the registry 0x7101538eb0, each tick while it is
    /// one: the music it took follows it; another starts in view and within the start range,
    /// and only if it is nearer than the music's own registry source (more than 2 px).
    fn register(&mut self, world: &World, id: u8, x: f32, y: f32, trigger: Trigger, actor: u32) {
        if !valid(id) || !matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
            return;
        }
        if let Some(m) = &mut self.music
            && m.actor == Some(actor)
        {
            (m.x, m.y) = (x, y);
            return;
        }
        if !Self::view_has(world, x, y) {
            return;
        }
        let d = Self::dist(world, x, y);
        if d >= trigger.start_range() || self.music.is_some_and(|m| m.dist - 2.0 <= d) {
            return;
        }
        let new = self.music.is_none_or(|m| m.id != id);
        self.take_music(id, x, y, trigger, Some(actor), d);
        if new {
            self.cues.push(Cue {
                id,
                x,
                y,
                source: Source::Part(match trigger {
                    Trigger::Ridden => PartTrigger::Ridden,
                    _ => PartTrigger::Defeated,
                }),
            });
        }
    }

    /// Twisty Turny / Woozy starts (or, running, starts over) its filter: 16 beats of the
    /// course music [obs 0x71017b15d0].
    fn start_filter(&mut self, world: &World, id: u8) {
        let tempo = self
            .areas
            .get(world.area)
            .map(|a| a.tempo)
            .unwrap_or([DEFAULT_BPM; 2]);
        let hurry = world.time >= 0 && world.time <= 100 * 60;
        let left = filter_frames(tempo[hurry as usize]);
        match self.filters.iter_mut().find(|f| f.id == id) {
            Some(f) => f.left = left,
            None => self.filters.push(Filter { id, left, age: 0 }),
        }
    }

    /// Forget a run: spots re-armed, music and filters off, voices free.
    fn reset_run(&mut self) {
        for a in &mut self.areas {
            for s in &mut a.spots {
                s.inside = 0;
            }
        }
        self.forget();
    }

    fn forget(&mut self) {
        self.music = None;
        self.filters.clear();
        self.voices = Voices::default();
        self.actors.clear();
        self.blocks.clear();
        self.coins.clear();
        self.shots.clear();
        self.claw = None;
        self.boxed = None;
        self.stood.clear();
        self.snakes_on.clear();
        self.exclams_bound.clear();
    }

    /// One world tick: this tick's effects into [`Self::cues`]; the music effect's start and end.
    pub fn step(&mut self, world: &World) {
        if world.tick == self.last_tick && self.started {
            return;
        }
        self.last_tick = world.tick;
        self.cues.clear();
        self.filters_ended.clear();
        self.voices.shared.clear();
        let players: Vec<&crate::player::Player> = std::iter::once(&world.player)
            .chain(&world.others)
            .collect();
        // (re)start: the first tick, a respawn, an area change; a respawn re-creates the
        // player, whose create-time virtual 0x710158a3a0 (vtable 0x7102954498 slot 7) sets off
        // Mario's effect again
        let respawn =
            self.started && !self.players.first().is_none_or(|p| p.alive) && world.player.alive();
        self.entered = !self.started || respawn;
        if world.area != self.area || respawn {
            // the pictures stop outside the music's area and the filters with them
            // (0x7100e443c0); no `end`
            self.forget();
            if let Some(a) = self.areas.get_mut(world.area) {
                for s in &mut a.spots {
                    s.inside = 0;
                }
            }
            self.area = world.area;
        }
        self.step_filters(world);
        if !self.started || respawn {
            if !self.started {
                self.reset_run();
            }
            self.started = true;
            self.coin_count = world.coins;
            if let Some(id) = self.player_effect {
                // every player's effect at the start; a music effect once (player 1's)
                for (n, p) in players.iter().enumerate() {
                    if n > 0 && matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
                        break;
                    }
                    self.fire(world, id, p.x, p.y, Source::Player(n as u8), true);
                }
            }
            self.players = players.iter().map(|p| Self::seen(p, false)).collect();
        } else {
            self.player_triggers(world, &players);
        }
        self.spots(world, &players);
        self.parts(world);
        self.music_end(world);
        self.snapshot(world);
    }

    /// The filters' frames left count down (the timers +0xa84 / +0xa88 tick per game frame);
    /// one at 0 ends (0x7100e443c0: the request bit +0xa62 bit 1 / 3).
    fn step_filters(&mut self, _world: &World) {
        let mut ended = Vec::new();
        self.filters.retain_mut(|f| {
            f.age += 1;
            if f.left <= 1 {
                ended.push(f.id);
                false
            } else {
                f.left -= 1;
                true
            }
        });
        self.filters_ended = ended;
    }

    fn seen(p: &crate::player::Player, fell: bool) -> PlayerSeen {
        PlayerSeen {
            alive: p.alive(),
            on_ground: p.on_ground,
            form: p.form as u8,
            riding: p.shoe.is_some() || p.yoshi.is_some() || p.clown.is_some() || p.car.is_some(),
            skid: p.pose == crate::player::Pose::Skid,
            fell,
            held: p.held,
        }
    }

    fn player_triggers(&mut self, world: &World, players: &[&crate::player::Player]) {
        let jumped = world.events.contains(&Event::Jump);
        let powered =
            world.events.contains(&Event::PowerUp) || world.events.contains(&Event::Wearing);
        let hurt = world.events.contains(&Event::PowerDown);
        let mut next = Vec::with_capacity(players.len());
        for (n, p) in players.iter().enumerate() {
            let old = self
                .players
                .get(n)
                .copied()
                .unwrap_or_else(|| Self::seen(p, false));
            let mut fell = old.fell && !p.alive();
            if let Some(id) = self.player_effect {
                let fire = match player_trigger(id) {
                    Some(PlayerTrigger::Jump) => {
                        jumped
                            && old.on_ground
                            && !p.on_ground
                            && p.vy > 0.0
                            && p.vx.abs() >= JUMP_SPEED
                    }
                    Some(PlayerTrigger::Damage) => {
                        (old.alive && !p.alive())
                            || (hurt && p.alive() && (p.form as u8) != old.form)
                    }
                    Some(PlayerTrigger::Fall) => {
                        let below = p.y < world.camera_y - 32.0;
                        if !p.alive() && !fell && below {
                            fell = true;
                            true
                        } else {
                            false
                        }
                    }
                    Some(PlayerTrigger::PowerUp) => {
                        powered && p.alive() && (p.form as u8) != old.form
                    }
                    Some(PlayerTrigger::Ride) => {
                        let s = Self::seen(p, false);
                        s.riding && !old.riding
                    }
                    Some(PlayerTrigger::Turn) => p.pose == crate::player::Pose::Skid && !old.skid,
                    None => false,
                };
                if fire {
                    // single player: no view test (0x71015d47b0's last argument)
                    let forced = players.len() == 1;
                    self.fire(world, id, p.x, p.y, Source::Player(n as u8), forced);
                }
            }
            next.push(Self::seen(p, fell));
        }
        self.players = next;
    }

    /// A player's box against the spots' centres: x within 8.5 px of the body's centre, y from
    /// 4 px under the feet to 10 px over the body's centre (0x7100e443c0) [obs].
    fn spots(&mut self, world: &World, players: &[&crate::player::Player]) {
        let Some(area) = self.areas.get(world.area) else {
            return;
        };
        let boxes: Vec<Option<(f32, f32, f32, f32)>> = players
            .iter()
            .map(|p| {
                if !p.alive() || p.hidden {
                    return None;
                }
                let h = if p.big() && !p.crouch { 32.0 } else { 16.0 };
                let (cx, cy) = (p.x, p.y + h / 2.0);
                let lo = cy.min(p.y + 4.0) - 8.0;
                let hi = cy + 2.0 + 8.0;
                Some((cx - 8.5, lo, cx + 8.5, hi))
            })
            .collect();
        let mut fired = Vec::new();
        let mut spots = area.spots.clone();
        for (i, s) in spots.iter_mut().enumerate() {
            let mut hit = false;
            for (n, b) in boxes.iter().enumerate().take(8) {
                let bit = 1u8 << n;
                let inside = b.is_some_and(|(x0, y0, x1, y1)| {
                    s.x >= x0 && s.x <= x1 && s.y >= y0 && s.y <= y1
                });
                if inside {
                    if s.inside & bit == 0 {
                        hit = true;
                    }
                    s.inside |= bit;
                } else {
                    s.inside &= !bit;
                }
            }
            if hit {
                fired.push(i);
            }
        }
        for i in fired {
            let s = &spots[i];
            let y = Self::spot_floor(world, s.x, s.y - 8.0);
            self.fire(world, s.id, s.x, y, Source::Spot, true);
        }
        if let Some(a) = self.areas.get_mut(world.area) {
            a.spots = spots;
        }
    }

    /// Where a spot's effect plays: at the spot's bottom, or on ground inside its tile — a ray
    /// from 15 px over the bottom down to 1 px over it (0x7100e2d2e0 from 0x7100e443c0) [obs]:
    /// a slope's surface, a moving solid's top.
    fn spot_floor(world: &World, x: f32, bottom: f32) -> f32 {
        let (top, low) = (bottom + 15.0, bottom + 1.0);
        let slopes = world
            .level
            .slopes
            .iter()
            .filter(|s| s.covers(x))
            .map(|s| s.surface(x));
        let solids = world
            .level
            .solids
            .iter()
            .filter(|s| x >= s.x0 && x <= s.x1)
            .map(|s| s.top_at(x));
        slopes
            .chain(solids)
            .filter(|&y| y >= low && y <= top)
            .fold(None, |best: Option<f32>, y| {
                Some(best.map_or(y, |b| b.max(y)))
            })
            .unwrap_or(bottom)
    }

    /// Whether a player stands on actor `id`.
    fn stood_on(world: &World, id: u32) -> bool {
        std::iter::once(&world.player)
            .chain(&world.others)
            .any(|p| p.alive() && p.on_ground && p.ride == Some(id))
    }

    /// The parts' actions since last tick.
    fn parts(&mut self, world: &World) {
        let Some(area) = self.areas.get(world.area) else {
            return;
        };
        let area = area.clone();
        let coins_up = world.coins > self.coin_count;
        // actors: defeated (alive → not), taken, stood on, bounced, gone in view while alive
        let mut now = BTreeMap::new();
        for a in &world.actors {
            let was = self.actors.get(&a.id).copied();
            let tag = was.map(|t| (t.id, t.t)).or_else(|| {
                let s = world.level.spawns.get(a.spawn?)?;
                area.actors.get(&spawn_key(s.x, s.y)).copied()
            });
            let Some((id, t)) = tag else { continue };
            let mut cur = Tagged {
                id,
                t,
                x: a.x,
                y: a.y,
                state: 0,
                cool: was.map_or(0, |w| w.cool.saturating_sub(1)),
                armed: was.is_some_and(|w| w.armed),
            };
            let music = matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_));
            match t {
                PartTrigger::Defeated => {
                    if was.is_some() && !a.alive() {
                        if !music {
                            self.fire(world, id, a.x, a.y, Source::Part(t), false);
                        }
                        continue;
                    }
                }
                PartTrigger::Taken | PartTrigger::Collected => {
                    if was.is_some_and(|w| w.state == 0) && (a.taken >= 2 || !a.alive()) {
                        if !music || t == PartTrigger::Collected {
                            self.fire(world, id, a.x, a.y, Source::Part(t), false);
                        }
                        continue;
                    }
                    cur.state = a.taken;
                }
                PartTrigger::Bounced => {
                    let ridden = match a.kind {
                        // a sideways spring's push (+pause 12, world::side_springs)
                        Kind::SideSpring => a.pause > 6,
                        _ => std::iter::once(&world.player)
                            .chain(&world.others)
                            .any(|p| p.ride == Some(a.id) && p.vy > 0.0 && !p.on_ground),
                    };
                    if ridden && !was.is_some_and(|w| w.state == 1) {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    }
                    cur.state = ridden as u8;
                    now.insert(a.id, cur);
                    continue;
                }
                PartTrigger::Ridden => {
                    let p_switch = a.kind == Kind::PSwitch;
                    let on = Self::stood_on(world, a.id);
                    let landed = on && !was.is_some_and(|w| w.state & 1 != 0);
                    // the P Switch picked up (its carry handler 0x71013e99a0, kind 5)
                    let held = std::iter::once(&world.player)
                        .chain(&world.others)
                        .any(|p| p.held == Some(a.id));
                    let pick = p_switch && held && !was.is_some_and(|w| w.state & 2 != 0);
                    if p_switch {
                        // stood on: fires when the 25-frame count is out, and keeps it full
                        if on {
                            if cur.cool == 0 && !music {
                                self.fire(world, id, a.x, a.y, Source::Part(t), false);
                            }
                            cur.cool = P_SWITCH_COOL;
                            cur.armed = true;
                        }
                    } else if landed && cur.cool == 0 {
                        if !music {
                            self.fire(world, id, a.x, a.y, Source::Part(t), false);
                        }
                        cur.cool = LIFT_COOL;
                        cur.armed = true;
                    }
                    if pick && !music {
                        self.fire(world, id, a.x, a.y, Source::Part(PartTrigger::Taken), false);
                    }
                    if cur.armed && music && a.alive() {
                        self.register(world, id, a.x, a.y + 8.0, Trigger::Ridden, a.id);
                    }
                    cur.state = on as u8 | (held as u8) << 1;
                    if a.alive() {
                        now.insert(a.id, cur);
                    }
                    continue;
                }
                PartTrigger::Caught => {
                    if self.claw != Some(a.id) && world.held_by == Some(a.id) {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    }
                    now.insert(a.id, cur);
                    continue;
                }
                // a moving block hit from below: the head meets it
                PartTrigger::Hit => {
                    let head = std::iter::once(&world.player)
                        .chain(&world.others)
                        .any(|p| p.head_solid == Some(a.id));
                    if head && !was.is_some_and(|w| w.state == 1) {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    }
                    cur.state = head as u8;
                    if a.alive() {
                        now.insert(a.id, cur);
                    }
                    continue;
                }
                PartTrigger::Boarded => {
                    let cloud = a.kind == Kind::Cloud;
                    let on = std::iter::once(&world.player)
                        .chain(&world.others)
                        .any(|p| {
                            p.clown == Some(a.id)
                                || p.car == Some(a.id)
                                || (cloud && p.ride == Some(a.id))
                        });
                    if on && !was.is_some_and(|w| w.state == 1) && !music {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    }
                    cur.state = on as u8;
                }
                PartTrigger::Hatched
                    if was.is_some()
                        && (!a.alive() || !matches!(a.kind, Kind::YoshiEgg { .. })) =>
                {
                    self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    continue;
                }
                _ => {}
            }
            if a.alive() && a.taken < 2 {
                if music && t.music_source() {
                    // enemies' and items' music plays while they are there (0x710128b250)
                    self.register(world, id, a.x, a.y + 8.0, Trigger::Enemy, a.id);
                }
                now.insert(a.id, cur);
            }
        }
        let gone: Vec<Tagged> = self
            .actors
            .iter()
            .filter(|(aid, _)| {
                !now.contains_key(aid) && !world.actors.iter().any(|a| a.id == **aid)
            })
            .map(|(_, &t)| t)
            .collect();
        for g in gone {
            if matches!(VOICES[g.id as usize], Voice::Music(_) | Voice::Ambient(_))
                && g.t != PartTrigger::Collected
            {
                continue;
            }
            // gone: a defeat in a puff in view, a coin / item taken
            let fire = match g.t {
                PartTrigger::Defeated => Self::view_has(world, g.x, g.y),
                PartTrigger::Taken => coins_up || Self::near_player(world, g.x, g.y),
                PartTrigger::Collected => g.state == 0,
                PartTrigger::Hatched => true,
                _ => false,
            };
            if fire {
                self.fire(world, g.id, g.x, g.y, Source::Part(g.t), false);
            }
        }
        self.actors = now;
        self.claw = world.held_by;
        // Bill Blasters: a new shot leaves the muzzle (x, bottom + height − 8)
        for a in &world.actors {
            let Some(bi) = a.blaster else { continue };
            if !self.shots.insert(a.id) {
                continue;
            }
            let Some(b) = world.level.blasters.get(bi) else {
                continue;
            };
            if let Some(&id) = area.blasters.get(&spawn_key(b.x, b.y)) {
                let y = b.y + b.h as f32 * TILE - 8.0;
                if matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
                    self.register(world, id, b.x, y, Trigger::Enemy, u32::MAX - bi as u32);
                } else {
                    self.fire(world, id, b.x, y, Source::Part(PartTrigger::Shot), false);
                }
            }
        }
        // Cannons: a new cannonball at the muzzle (world::step_cannons)
        for a in &world.actors {
            if a.kind != Kind::Cannonball || self.shots.contains(&a.id) {
                continue;
            }
            self.shots.insert(a.id);
            let near = world.level.cannons.iter().find(|c| {
                use crate::fmath::DetMath;
                let r = c.angle.to_radians();
                // the ball starts 8 px under the muzzle's centre
                let (mx, my) = (c.x + 8.0 * r.dcos(), c.y + 8.0 * r.dsin());
                (a.x - mx).abs() < 4.0 && (a.y - my).abs() < 4.0
            });
            if let Some(c) = near
                && let Some(&id) = area.cannons.get(&spawn_key(c.x, c.y))
            {
                let (x, y) = (c.x, c.y + 8.0);
                if matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
                    self.register(world, id, x, y, Trigger::Enemy, a.id);
                } else {
                    self.fire(world, id, x, y, Source::Part(PartTrigger::Shot), false);
                }
            }
        }
        self.shots
            .retain(|s| world.actors.iter().any(|a| a.id == *s));
        // Dash Blocks: a landing on one (its tile under the feet)
        let players: Vec<&crate::player::Player> = std::iter::once(&world.player)
            .chain(&world.others)
            .collect();
        for (&tile, &(id, t)) in &area.tiles {
            if t != PartTrigger::Ridden {
                continue;
            }
            let on = players.iter().any(|p| {
                p.alive()
                    && p.on_ground
                    && (p.x / TILE).floor() as i32 == tile.0
                    && ((p.y - 1.0) / TILE).floor() as i32 == tile.1
                    && world.level.v3.dash_under(p.x, p.y)
            });
            let (was, cool) = self.stood.get(&tile).copied().unwrap_or_default();
            let cool = cool.saturating_sub(1);
            let (x, y) = (tile.0 as f32 * TILE + 8.0, tile.1 as f32 * TILE + 16.0);
            if on && !was {
                if matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
                    self.register(
                        world,
                        id,
                        x,
                        y,
                        Trigger::Ridden,
                        u32::MAX / 2 + tile.0 as u32,
                    );
                } else {
                    self.fire(world, id, x, y, Source::Part(t), false);
                }
            }
            self.stood.insert(tile, (on, cool));
        }
        // snake blocks: a landing, 8 frames apart (as a lift); ! Blocks: each hit
        for (si, sn) in world.snakes.iter().enumerate() {
            let Some(&(tx, ty, ..)) = world.level.snakes.get(si) else {
                continue;
            };
            let Some(&id) = area.snakes.get(&(tx, ty)) else {
                continue;
            };
            let base = crate::world::SNAKE_OWNER + si as u32 * 32;
            let on = players
                .iter()
                .any(|p| p.ride.is_some_and(|r| r >= base && r < base + 32));
            let (was, cool) = self.snakes_on.get(&si).copied().unwrap_or_default();
            let mut cool = cool.saturating_sub(1);
            let (x, y) = sn
                .segments()
                .first()
                .copied()
                .unwrap_or((tx as f32 * TILE, ty as f32 * TILE));
            if on && !was && cool == 0 {
                if !matches!(VOICES[id as usize], Voice::Music(_) | Voice::Ambient(_)) {
                    self.fire(world, id, x, y, Source::Part(PartTrigger::Ridden), false);
                }
                cool = LIFT_COOL;
            }
            self.snakes_on.insert(si, (on, cool));
        }
        for (bi, b) in world.exclamations.iter().enumerate() {
            let Some(&id) = area.exclams.get(&b.tile) else {
                continue;
            };
            let bound = b.bound();
            if bound && !self.exclams_bound.contains(&bi) {
                let (x, y) = (b.tile.0 as f32 * TILE + 16.0, b.tile.1 as f32 * TILE);
                self.fire(world, id, x, y, Source::Part(PartTrigger::Bound), false);
            }
            if bound {
                self.exclams_bound.insert(bi);
            } else {
                self.exclams_bound.remove(&bi);
            }
        }
        // a warp box left: the exit box's effect as the player comes out (its "Out")
        let boxed = world.transit.and_then(|t| match t.to.kind {
            crate::level::WarpKind::Box { .. } => Some((t.to.x, t.to.y)),
            _ => None,
        });
        if let (Some((x, y)), None) = (self.boxed, boxed)
            && let Some(&id) = area.boxes.get(&spawn_key(x, y))
        {
            self.fire(world, id, x, y, Source::Part(PartTrigger::Out), false);
        }
        self.boxed = boxed;
        // blocks: a bump starting or the block gone
        for (&tile, &(id, t)) in &area.tiles {
            if t != PartTrigger::Hit {
                continue;
            }
            let (x, y) = (tile.0 as f32 * TILE + 8.0, tile.1 as f32 * TILE);
            match (
                self.blocks.get(&tile).copied(),
                world.blocks.get(tile.0, tile.1),
            ) {
                (Some(false), Some(b)) if b.bump.is_some() => {
                    self.fire(world, id, x, y, Source::Part(t), false)
                }
                (Some(_), None) => self.fire(world, id, x, y, Source::Part(t), false),
                _ => {}
            }
        }
        // coins on the grid: gone while the count went up
        let present = Self::coins_left(world, &area);
        let taken: Vec<(i32, i32)> = self.coins.difference(&present).copied().collect();
        for c in taken {
            let (x, y) = (c.0 as f32 * TILE + 8.0, c.1 as f32 * TILE);
            if coins_up || Self::near_player(world, x, y + 8.0) {
                let (id, t) = area.tiles[&c];
                self.fire(world, id, x, y, Source::Part(t), false);
            }
        }
        self.coins = present;
        // pipes and doors: entered (the warp's start, Event::Pipe) where a player stands
        if world.events.contains(&Event::Pipe) {
            for p in std::iter::once(&world.player).chain(&world.others) {
                let (tx, ty) = ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32);
                let hit = [(0, 0), (0, -1), (0, 1), (-1, 0), (1, 0), (0, 2)]
                    .iter()
                    .find_map(|(dx, dy)| {
                        area.tiles
                            .get(&(tx + dx, ty + dy))
                            .filter(|t| t.1 == PartTrigger::Entered)
                    })
                    .copied();
                if let Some((id, t)) = hit {
                    self.fire(world, id, p.x, p.y, Source::Part(t), false);
                }
            }
        }
        self.coin_count = world.coins;
    }

    /// The tagged coins still there: grid coins, and pink coins not yet in `World::pink`.
    fn coins_left(world: &World, area: &AreaParts) -> BTreeSet<(i32, i32)> {
        let pink = world
            .level
            .pink_coins
            .iter()
            .filter(|c| !world.pink.contains(&(world.area, c.x, c.y)));
        world
            .level
            .coins
            .iter()
            .chain(pink)
            .map(|c| (c.x, c.y))
            .filter(|c| area.tiles.get(c).is_some_and(|t| t.1 == PartTrigger::Taken))
            .collect()
    }

    fn near_player(world: &World, x: f32, y: f32) -> bool {
        std::iter::once(&world.player)
            .chain(&world.others)
            .any(|p| (p.x - x).abs() <= 24.0 && (p.y + 8.0 - y).abs() <= 32.0)
    }

    /// The music effect ends when the players are farther than its source's range.
    fn music_end(&mut self, world: &World) {
        let Some(m) = self.music else { return };
        if Self::dist(world, m.x, m.y) > m.trigger.end_range() {
            self.music = None;
        }
    }

    /// The tagged blocks' bump state and tagged coins, for next tick.
    fn snapshot(&mut self, world: &World) {
        let Some(area) = self.areas.get(world.area) else {
            return;
        };
        self.blocks = area
            .tiles
            .iter()
            .filter(|(_, t)| t.1 == PartTrigger::Hit)
            .filter_map(|(&tile, _)| {
                world
                    .blocks
                    .get(tile.0, tile.1)
                    .map(|b| (tile, b.bump.is_some()))
            })
            .collect();
        if self.coins.is_empty() {
            self.coins = Self::coins_left(world, area);
        }
        self.coin_count = world.coins;
    }
}

#[cfg(test)]
mod tests;
