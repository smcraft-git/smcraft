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
    ICON_IDS, ICON_LABELS, PAGES, icon, label, page_of, valid,
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
/// whole mix while they last (0x7101537c40 sets the audio manager's filter flags; the
/// `OtoAsobi_Filter` / `OtoAsobi_Mod` / `OtoAsobi_Vib` buses, 0x71017b15d0). Not modelled: the
/// engine plays their sound only.
pub fn audio_filter(id: u8) -> bool {
    matches!(id, 46 | 47)
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

/// What a part does to set off its effect, by the kind of part [obs: the play calls' trigger
/// kinds: 1 enemies (the generic defeat 0x710128ef80), 5 objects (coins 0x71013a0650, items
/// 0x710134ffd0, blocks 0x710138a430…0x710138c690, Trampolines 0x71013b9ef0, doors
/// 0x71013a6710), 6 pipes 0x7100d84810].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartTrigger {
    /// Defeated (stomped, knocked out, gone in a puff).
    Defeated,
    /// Taken by a player (coins, power-ups).
    Taken,
    /// Hit from below or the side, or broken.
    Hit,
    /// Bounced on.
    Bounced,
    /// Entered.
    Entered,
}

impl PartTrigger {
    /// The play call's trigger kind (the music's range and end rules, [`Trigger`]).
    fn trigger(self) -> Trigger {
        match self {
            PartTrigger::Defeated => Trigger::Enemy,
            PartTrigger::Entered => Trigger::Pipe,
            _ => Trigger::Object,
        }
    }
}

/// The play call's trigger kind (its 6th argument), which sets how far a music effect reaches
/// [obs 0x7101537ab0, 0x7100e443c0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// 0: Mario's own effect.
    Player,
    /// 1: an enemy's.
    Enemy,
    /// 3: a spot's.
    Spot,
    /// 5: an object's.
    Object,
    /// 6: a pipe's.
    Pipe,
}

impl Trigger {
    /// A music effect from this source starts only within this distance of the players
    /// (0x7101537ab0: 200 px for kinds 1 / 4 / 6, 160 for 5; Mario's and the spots' always).
    pub fn start_range(self) -> f32 {
        match self {
            Trigger::Enemy | Trigger::Pipe => 200.0,
            Trigger::Object => 160.0,
            Trigger::Player | Trigger::Spot => f32::INFINITY,
        }
    }

    /// The music ends when the players get farther than this from its source: 160 px for a
    /// spot, 180 for an object (and 304 for kind 4, the parts not modelled); Mario's, an
    /// enemy's and a pipe's last until the area changes or the run ends (0x7100e443c0).
    pub fn end_range(self) -> f32 {
        match self {
            Trigger::Spot => 160.0,
            Trigger::Object => 180.0,
            _ => f32::INFINITY,
        }
    }
}

/// The part kinds whose effect the engine sets off, by their course object.
fn part_trigger(o: &CourseObject, lvl: &Level) -> Option<PartTrigger> {
    let actorish = o.flags & 0x2 != 0 || o.flags & 0x400 != 0;
    if crate::level::block_kind(o.id).is_some() && !actorish {
        return Some(PartTrigger::Hit);
    }
    match o.id {
        // coins and pink coins on the grid
        8 | 92 if !actorish && o.flags & 0x8000 == 0 && o.flags & 1 == 0 => {
            return Some(PartTrigger::Taken);
        }
        // pipes and doors
        9 | 55 => return Some(PartTrigger::Entered),
        _ => {}
    }
    let kind = crate::level::part_kind(o, lvl)?;
    match kind {
        Kind::Spring => Some(PartTrigger::Bounced),
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
        )
}

/// The course's effects the engine doesn't play, for `mmk coverage`: on a part whose action
/// isn't modelled (`sound effect on part <id>`) and the two audio filters.
pub fn unsupported(course: &Course) -> Vec<String> {
    let mut out = BTreeSet::new();
    let style = course.header.game_style.as_str_lossy();
    let mut probe = Level::flat(1);
    probe.style = style;
    let mut ids: Vec<u8> = course.header.player_effect().into_iter().collect();
    for area in course.areas() {
        probe.theme = area.header.theme;
        ids.extend(area.sound_effects.iter().map(|s| s.id));
        for o in area.objects.iter() {
            let Some(id) = o.sound_effect() else { continue };
            ids.push(id);
            if part_trigger(o, &probe).is_none() {
                out.insert(format!("sound effect on part {}", o.id));
            }
        }
    }
    for id in ids.into_iter().filter(|&i| audio_filter(i)) {
        out.insert(format!("sound effect {id} (audio filter)"));
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
/// it was set off and by what.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Music {
    pub id: u8,
    pub slot: u8,
    pub ambient: bool,
    pub x: f32,
    pub y: f32,
    pub trigger: Trigger,
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
}

impl AreaParts {
    fn of(area: &Area, lvl: &Level) -> Self {
        let mut a = AreaParts::default();
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
            let grid = matches!(t, PartTrigger::Hit | PartTrigger::Entered)
                || (t == PartTrigger::Taken && matches!(o.id, 8 | 92) && o.flags & 0x2 == 0);
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
}

/// The voices the effects share, as of their start frames (the repeat rules of [`Voice`]).
#[derive(Clone, Debug, Default, PartialEq)]
struct Voices {
    /// Shared voices started this frame: effects.
    shared: Vec<u8>,
    /// The long voice: effect, start tick.
    long: Option<(u8, u64)>,
    /// Own voices: effect → start tick.
    own: BTreeMap<u8, u64>,
}

/// How long an own / long voice counts as playing for the `hold` rule [inf: the game asks the
/// voice; the effects' sounds are 1–4 s long, 120 frames is the shortest hold that covers the
/// DJ scratches (Twisty Turny, Woozy)].
const HOLD_FRAMES: u64 = 120;

/// Watches the world for the course's effects; step it once per world tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundParts {
    areas: Vec<AreaParts>,
    player_effect: Option<u8>,
    /// This tick's effects and the music effect in play.
    pub cues: Vec<Cue>,
    pub music: Option<Music>,
    started: bool,
    last_tick: u64,
    area: usize,
    players: Vec<PlayerSeen>,
    /// Tagged actors alive last tick: actor id → (effect, trigger, x, y, taken).
    actors: BTreeMap<u32, (u8, PartTrigger, f32, f32, u8)>,
    /// Tagged blocks last tick: tile → bumping.
    blocks: BTreeMap<(i32, i32), bool>,
    /// Tagged coins on the grid last tick.
    coins: BTreeSet<(i32, i32)>,
    coin_count: u32,
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
            || self
                .areas
                .iter()
                .any(|a| !a.spots.is_empty() || !a.tiles.is_empty() || !a.actors.is_empty())
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

    /// Set off effect `id` at (x, y) by `source`; `forced`: no view test (spots, Mario's own
    /// effect in single player) [obs 0x7101537c40].
    fn fire(&mut self, world: &World, id: u8, x: f32, y: f32, source: Source, forced: bool) {
        if !valid(id) {
            return;
        }
        let trigger = source.trigger();
        let tick = world.tick;
        match VOICES[id as usize] {
            Voice::Music(slot) | Voice::Ambient(slot) => {
                let (lx, ly) = Self::listener(world);
                let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                if !forced && !Self::view_has(world, x, y) && trigger != Trigger::Spot {
                    return;
                }
                if d >= trigger.start_range() {
                    return;
                }
                let ambient = matches!(VOICES[id as usize], Voice::Ambient(_));
                if self.music.is_some_and(|m| m.slot == slot) {
                    // the same music goes on; a nearer source takes over (0x7101538eb0)
                    if let Some(m) = &mut self.music {
                        let dm = ((m.x - lx).powi(2) + (m.y - ly).powi(2)).sqrt();
                        if d < dm - 2.0 {
                            (m.x, m.y, m.trigger) = (x, y, trigger);
                        }
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
                });
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
                        if let Some(&t) = self.voices.own.get(&id) {
                            let age = tick.saturating_sub(t);
                            if (hold && age < HOLD_FRAMES) || (id == 41 && age < 5) {
                                return;
                            }
                        }
                        self.voices.own.insert(id, tick);
                    }
                    _ => {}
                }
            }
        }
        self.cues.push(Cue { id, x, y, source });
    }

    /// Forget a run: spots re-armed, music off, voices free.
    fn reset_run(&mut self) {
        for a in &mut self.areas {
            for s in &mut a.spots {
                s.inside = 0;
            }
        }
        self.music = None;
        self.voices = Voices::default();
        self.actors.clear();
        self.blocks.clear();
        self.coins.clear();
    }

    /// One world tick: this tick's effects into [`Self::cues`]; the music effect's start and end.
    pub fn step(&mut self, world: &World) {
        if world.tick == self.last_tick && self.started {
            return;
        }
        self.last_tick = world.tick;
        self.cues.clear();
        self.voices.shared.clear();
        let players: Vec<&crate::player::Player> = std::iter::once(&world.player)
            .chain(&world.others)
            .collect();
        // (re)start: the first tick, a respawn, an area change
        let respawn =
            self.started && !self.players.first().is_none_or(|p| p.alive) && world.player.alive();
        if world.area != self.area || respawn {
            self.music = None;
            self.voices = Voices::default();
            self.actors.clear();
            self.blocks.clear();
            self.coins.clear();
            if let Some(a) = self.areas.get_mut(world.area) {
                for s in &mut a.spots {
                    s.inside = 0;
                }
            }
            self.area = world.area;
        }
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

    fn seen(p: &crate::player::Player, fell: bool) -> PlayerSeen {
        PlayerSeen {
            alive: p.alive(),
            on_ground: p.on_ground,
            form: p.form as u8,
            riding: p.shoe.is_some() || p.yoshi.is_some() || p.clown.is_some() || p.car.is_some(),
            skid: p.pose == crate::player::Pose::Skid,
            fell,
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
                            || (hurt && p.alive() && (p.form as u8) < old.form)
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
            // the effect plays at the spot's bottom
            self.fire(world, s.id, s.x, s.y - 8.0, Source::Spot, true);
        }
        if let Some(a) = self.areas.get_mut(world.area) {
            a.spots = spots;
        }
    }

    /// The parts' actions since last tick.
    fn parts(&mut self, world: &World) {
        let Some(area) = self.areas.get(world.area) else {
            return;
        };
        let area = area.clone();
        let coins_up = world.coins > self.coin_count;
        // actors: defeated (alive → not), taken, gone in view while alive
        let mut now = BTreeMap::new();
        for a in &world.actors {
            let tag = self.actors.get(&a.id).map(|t| (t.0, t.1)).or_else(|| {
                let s = world.level.spawns.get(a.spawn?)?;
                area.actors.get(&spawn_key(s.x, s.y)).copied()
            });
            let Some((id, t)) = tag else { continue };
            let was = self.actors.get(&a.id).copied();
            match t {
                PartTrigger::Defeated => {
                    if was.is_some() && !a.alive() {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                        continue;
                    }
                }
                PartTrigger::Taken => {
                    if was.is_some_and(|w| w.4 == 0) && a.taken >= 2 {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                        continue;
                    }
                }
                PartTrigger::Bounced => {
                    let ridden = std::iter::once(&world.player)
                        .chain(&world.others)
                        .any(|p| p.ride == Some(a.id) && p.vy > 0.0 && !p.on_ground);
                    let was_ridden = was.is_some_and(|w| w.4 == 1);
                    if ridden && !was_ridden {
                        self.fire(world, id, a.x, a.y, Source::Part(t), false);
                    }
                    now.insert(a.id, (id, t, a.x, a.y, ridden as u8));
                    continue;
                }
                _ => {}
            }
            if a.alive() && a.taken < 2 {
                now.insert(a.id, (id, t, a.x, a.y, a.taken));
            }
        }
        let gone: Vec<(u8, PartTrigger, f32, f32)> = self
            .actors
            .iter()
            .filter(|(aid, _)| {
                !now.contains_key(aid) && !world.actors.iter().any(|a| a.id == **aid)
            })
            .map(|(_, &(id, t, x, y, _))| (id, t, x, y))
            .collect();
        for (id, t, x, y) in gone {
            // gone: a defeat in a puff in view, a coin / item taken
            let fire = match t {
                PartTrigger::Defeated => Self::view_has(world, x, y),
                PartTrigger::Taken => coins_up || Self::near_player(world, x, y),
                _ => false,
            };
            if fire {
                self.fire(world, id, x, y, Source::Part(t), false);
            }
        }
        self.actors = now;
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
        let present: BTreeSet<(i32, i32)> = world
            .level
            .coins
            .iter()
            .chain(&world.level.pink_coins)
            .map(|c| (c.x, c.y))
            .filter(|c| area.tiles.get(c).is_some_and(|t| t.1 == PartTrigger::Taken))
            .collect();
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

    fn near_player(world: &World, x: f32, y: f32) -> bool {
        std::iter::once(&world.player)
            .chain(&world.others)
            .any(|p| (p.x - x).abs() <= 24.0 && (p.y + 8.0 - y).abs() <= 32.0)
    }

    /// The music effect ends when the players are farther than its source's range.
    fn music_end(&mut self, world: &World) {
        let Some(m) = self.music else { return };
        let (lx, ly) = Self::listener(world);
        let d = ((m.x - lx).powi(2) + (m.y - ly).powi(2)).sqrt();
        if d > m.trigger.end_range() {
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
            self.coins = world
                .level
                .coins
                .iter()
                .chain(&world.level.pink_coins)
                .map(|c| (c.x, c.y))
                .filter(|c| area.tiles.get(c).is_some_and(|t| t.1 == PartTrigger::Taken))
                .collect();
        }
        self.coin_count = world.coins;
    }
}

#[cfg(test)]
mod tests;
