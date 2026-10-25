//! 100 Mario Challenge screens drawn with the SMM1 layouts (the owner's SMM1 import; the
//! Swift app's `ChallengeScreens.swift`), for the "SMM1 layouts" presentation: difficulty tabs
//! over the world map, the walk and the intro card, game over, the ending and credits, and the
//! in-course overlays. Choreography the data does not pin down is marked [inf] (DECISIONS.md
//! "100 Mario screens: SMM1 layouts").
//!
//! Sounds: the SMM1 import's SLink keys and states ([`crate::smm1_sound`],
//! research/specs/smm1-sound.md), fired where the screens play the animations they go with.
//! Which key goes with which of our animations is matched by name where the data names one
//! (`In`, `Out`, `Decide`, state `1stDemoAnimB` ↔ `AnimB`, `1stDemoTelopIn` ↔ `Telop`,
//! `cState_Walk` ↔ `Walk`), else [inf].

use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, UiInput, hit};
use crate::smm1_beat::{BeatBounce, BeatClock, BeatEvents};
use crate::smm1_sound::Ctx;
use crate::smm1_window::{MessageWindow, WindowEvent, WindowKind};
use crate::sound::SoundCue;
use crate::{DrawItem, push_layout};

/// Sounds due at a later tick (the animation they go with starts then); dropped when the scene
/// skips ahead of them.
#[derive(Default)]
struct Pending(Vec<(i32, &'static str, Vec<SoundCue>)>);

impl Pending {
    fn at(&mut self, tick: i32, tag: &'static str, cues: Vec<SoundCue>) {
        if !cues.is_empty() {
            self.0.push((tick, tag, cues));
        }
    }

    fn drop_tag(&mut self, tag: &str) {
        self.0.retain(|e| e.1 != tag);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    /// Start the ones due at `tick`.
    fn fire(&mut self, ui: &Ui, tick: i32) {
        let (due, rest) = std::mem::take(&mut self.0)
            .into_iter()
            .partition::<Vec<_>, _>(|e| e.0 <= tick);
        self.0 = rest;
        for (_, _, cues) in due {
            ui.push_cues(cues);
        }
    }
}

/// Fire SMM1 key `key` on layout `l` (its part `part`) now.
fn key(l: &Layout, part: Option<&str>, key: &str, ctx: &[Ctx]) {
    let ui = l.ui();
    ui.push_cues(ui.smm1_key(&l.name, part, key, ctx));
}

/// Fire SMM1 key `key` (no switch values) on layout `l` (its part `part`) now.
pub(crate) fn fire_key(l: &Layout, part: Option<&str>, name: &str) {
    key(l, part, name, &[]);
}

/// The game style's enum value in the pause menu's switch (`CourseStatus::style` 0–3; the 3D
/// World style has none).
fn style_name(style: u32) -> &'static str {
    ["M1", "M3", "MW", "WU"]
        .get(style as usize)
        .copied()
        .unwrap_or("3W")
}

/// What the intro card shows.
#[derive(Clone, Debug)]
pub struct RunCardInfo {
    pub name: String,
    pub maker: String,
    pub code: Option<String>,
    pub lives: u32,
    pub number: u32,
    pub total: u32,
}

impl Default for RunCardInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            maker: String::new(),
            code: None,
            lives: 100,
            number: 1,
            total: 8,
        }
    }
}

pub const RUN_LENGTHS: [usize; 4] = [8, 16, 16, 6];

/// Fill a `Play_CourseIn100Mario_00` instance: texts, and one small Mario per life (the
/// formation is 100 Marios; the one that steps forward in AnimB is #99, so the hidden ones are
/// the first).
pub fn configure_card(card: &mut Layout, info: &RunCardInfo) {
    card.text("T_CourseName_00", &info.name);
    card.text("T_PlayerName_00", &info.maker);
    match &info.code {
        Some(code) => {
            card.text("T_IDNumber_00", code);
            card.visible("N_CourseID_00", None);
        }
        None => card.show("N_CourseID_00", false),
    }
    card.text("T_Zanki_00", &info.lives.to_string());
    card.text("T_CourseNum_00", &format!("{}/{}", info.number, info.total));
    // the maker's flag: SMM1 shows the course's country (0x0253d380); there is no country in
    // the course data here [departure]. The Mii face stays the dummy, as SMM1 leaves it when
    // the maker's face can't be fetched (0x02524f40) [obs; that its fallback is this picture
    // inf]
    card.show("L_NF_00", false);
    card.show("P_NFBase_00", false);
    // the maker group (face, flag, name) centred on its name's width: x = (group − (flag +
    // face + name)) × 0.5 + its own x (0x02524d0c, 0.5 at 0x100a1f9c) [obs]
    if let (Some(g), Some(w)) = (card.pane("N_Player_00"), card.text_width("T_PlayerName_00")) {
        let width = |p: &str| card.pane(p).map_or(0.0, |p| p.size[0]);
        let used = width("L_NF_00") + width("P_DummyMii_00") + w;
        let x = (g.size[0] - used) * 0.5 + g.translate[0];
        card.set_translate("N_Player_00", [x, g.translate[1]]);
    }
    let shown = info.lives.min(100) as usize;
    for i in 0..100 {
        card.visible(
            &format!("P_MarioSml_{i:02}"),
            if i >= 100 - shown { None } else { Some(false) },
        );
    }
}

// MARK: - world map

const SUFFIX: [&str; 4] = ["Esy", "Nml", "Exp", "SpExp"];
const MARIO: &str = "N_WorldMapMario_00";
const MARIO_PART: &str = "L_WorldMapMario_00";
const KURIBO_PART: &str = "L_WorldMapKuribo_00";
const CASTLES: [&str; 2] = ["L_Castle_00", "L_Castle_01"];

/// The map's animation slots (one layout group each: animations of a slot replace each other)
/// [obs: the anim / group tables 0x10230314 / 0x10230384, filled at 0x024f6f88].
const SLOT_COUNT: [&str; 2] = ["CountIn", "CountOut"];
const SLOT_INOUT: [&str; 6] = ["InL", "OutL", "InR", "OutR", "In", "Out"];
/// The Mario part's `G_Mario_00` animations (`In` / `Out` are its `G_All_00`).
const MARIO_SLOT: [&str; 5] = ["Walk", "Jump", "TwoJump", "Go", "Stand"];

/// Frames of `MoveMario` between two course dots, and its first dot's frame: 12 and 60
/// (constants 0x1009b314, 0x1009b428); Super Expert's step is (MoveMario − 48) / 6 = 19 and its
/// first dot is at step + 48 = 67 (0x1009b318, 0x1009b31c) [obs 0x024f4668].
const DOT_STEP: f32 = 12.0;
const FIRST_DOT: f32 = 60.0;
/// MoveMario's end of the jump out of the start castle (0x1009b318) [obs 0x024f65ec].
const CASTLE_JUMP: f32 = 48.0;

/// The Mario part's states (`Nwk_WorldMapMario_00`, class 0x0251e25c) [obs]: every one-shot
/// ends in Wait; Out ends hidden (Init).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarioState {
    Init,
    Jump,
    Wait,
    Out,
    TwoJump,
    Walk,
    Stand,
    Go,
}

/// A course dot's states (class 0x0251ba78): Init (hidden), In (`In`, 42 f), Wait, Out (8 f)
/// [obs].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotState {
    Init,
    In,
    Wait,
    Out,
}

/// One difficulty's world map (`Nwk_WorldMap{Esy,Nml,Exp,SpExp}_00`) with its parts' states,
/// driven the way the map class 0x024f2ae0 drives them.
pub struct WorldMap {
    pub layout: Layout,
    pub difficulty: usize,
    pub count: usize,
    step: f32,
    mario: MarioState,
    mario_until: i32,
    dots: Vec<(DotState, i32)>,
    castles: [i32; 2],
    /// Its beat bounces and the music's clock that drives them ([`crate::smm1_beat`]).
    beats: BeatBounce,
    pub clock: Option<BeatClock>,
}

impl WorldMap {
    pub fn new(ui: &Ui, d: usize) -> Option<Self> {
        let l = ui.layout(&format!("Nwk_WorldMap{}_00", SUFFIX[d]))?;
        let mut m = Self {
            layout: l,
            difficulty: d,
            count: RUN_LENGTHS[d],
            step: DOT_STEP,
            mario: MarioState::Init,
            mario_until: 0,
            dots: vec![(DotState::Init, 0); RUN_LENGTHS[d]],
            castles: [0; 2],
            beats: BeatBounce::default(),
            clock: None,
        };
        if d == 3 {
            let len = m.layout.length("MoveMario");
            if len > 0.0 {
                m.step = (len - CASTLE_JUMP) / (m.count as f32);
            }
        }
        let l = &mut m.layout;
        // the resting loops; the bounces (the balloon's `Beat_00_00`, the grass's `Beat_0x_01`,
        // the castles' `Beat_0x_00` and Bowser's `Anm_Koopa_00`) wait for the music's beats
        l.play_loop("Default", 0);
        for c in CASTLES {
            l.play_loop(&format!("{c}/Default"), 0);
        }
        m.beats = BeatBounce::new(&mut m.layout, &CASTLES.map(String::from));
        let l = &mut m.layout;
        // the LR slot at OutL frame 0: the fully shown pose [obs 0x024f4668]
        l.hold("OutL", 0.0);
        m.kuribo_init();
        m.mario_init();
        Some(m)
    }

    pub fn dot(&self, i: usize) -> String {
        format!("L_WorldMapCourse{}_{i:02}", SUFFIX[self.difficulty])
    }

    /// MoveMario's frame with Mario on dot `d` [obs 0x024f66c0, 0x024f6d30].
    pub fn mario_frame(&self, d: usize) -> f32 {
        let d = d.min(self.count.saturating_sub(1)) as f32;
        if self.difficulty == 3 {
            d * self.step + self.step + CASTLE_JUMP
        } else {
            d * self.step + FIRST_DOT
        }
    }

    /// Play `anim` of a slot, stopping the slot's others.
    fn slot_play(l: &mut Layout, slot: &[&str], anim: &str, tick: i32) {
        for a in slot {
            if *a != anim {
                l.stop(a);
            }
        }
        l.play(anim, tick);
    }

    /// The entry slot's animation (`In`, `InL`, `InR`, `Out`, …) from `tick`.
    pub fn play_inout(&mut self, anim: &str, tick: i32) {
        Self::slot_play(&mut self.layout, &SLOT_INOUT, anim, tick);
    }

    pub fn count_in(&mut self, tick: i32) {
        Self::slot_play(&mut self.layout, &SLOT_COUNT, "CountIn", tick);
    }

    pub fn count_out(&mut self, tick: i32) {
        Self::slot_play(&mut self.layout, &SLOT_COUNT, "CountOut", tick);
    }

    pub fn set_counters(&mut self, lives: u32, cleared: usize) {
        self.layout.text("T_RestMarioNum_00", &lives.to_string());
        self.layout
            .text("T_ClearCourseNum_00", &format!("{cleared}/{}", self.count));
    }

    // MARK: Mario

    fn mario_anim(&mut self, anim: &str, tick: i32, looping: bool) {
        let l = &mut self.layout;
        for a in MARIO_SLOT {
            if a != anim {
                l.stop(&format!("{MARIO_PART}/{a}"));
            }
        }
        let p = format!("{MARIO_PART}/{anim}");
        if looping {
            l.play_loop(&p, tick);
        } else {
            l.play(&p, tick);
        }
    }

    fn mario_shown(&mut self, tick: Option<i32>) {
        let l = &mut self.layout;
        l.visible(MARIO_PART, None);
        l.stop(&format!("{MARIO_PART}/Out"));
        match tick {
            Some(t) => l.play(&format!("{MARIO_PART}/In"), t),
            None => l.hold_end(&format!("{MARIO_PART}/In")),
        }
    }

    /// Init: hidden (0x0251eea4 with 1) [obs].
    pub fn mario_init(&mut self) {
        self.mario = MarioState::Init;
        let l = &mut self.layout;
        for a in MARIO_SLOT {
            l.stop(&format!("{MARIO_PART}/{a}"));
        }
        l.stop(&format!("{MARIO_PART}/In"));
        l.stop(&format!("{MARIO_PART}/Out"));
        l.show(MARIO_PART, false);
    }

    /// Out (`Out`, 20 f) → Init; nothing when he is already hidden (0x0251eea4 with 0) [obs].
    pub fn mario_out(&mut self, tick: i32) {
        if self.mario == MarioState::Init {
            return;
        }
        self.layout.play(&format!("{MARIO_PART}/Out"), tick);
        self.mario = MarioState::Out;
        self.mario_until = tick + self.layout.ticks(&format!("{MARIO_PART}/Out"), 1);
    }

    /// Jump (`In` + `Jump` together, 20 f): out of the start castle or onto his dot
    /// (0x0251eebc); the part's `jump` key [inf: key ↔ animation, smm1-sound.md §4].
    pub fn mario_jump(&mut self, tick: i32) {
        self.mario_shown(Some(tick));
        self.mario_anim("Jump", tick, false);
        let l = &mut self.layout;
        let n = l
            .ticks(&format!("{MARIO_PART}/Jump"), 1)
            .max(l.ticks(&format!("{MARIO_PART}/In"), 1));
        self.mario = MarioState::Jump;
        self.mario_until = tick + n;
        key(&self.layout, Some(MARIO_PART), "jump", &[]);
    }

    /// TwoJump (36 f, 0x0251ef10): the state's two jump sounds.
    pub fn mario_two_jump(&mut self, tick: i32) {
        self.mario_shown(None);
        self.mario_anim("TwoJump", tick, false);
        self.mario = MarioState::TwoJump;
        self.mario_until = tick + self.layout.ticks(&format!("{MARIO_PART}/TwoJump"), 1);
        self.state_sounds(Some(MARIO_PART), "cState_TwoJump");
    }

    /// Walk (the `Walk` loop, 0x0251ef70): `M3_MoveMap` 5 frames in.
    pub fn mario_walk(&mut self, tick: i32) {
        self.mario_shown(None);
        self.mario_anim("Walk", tick, true);
        self.mario = MarioState::Walk;
        self.state_sounds(Some(MARIO_PART), "cState_Walk");
    }

    /// Stand (`Stand`, one frame → Wait, 0x0251efcc).
    pub fn mario_stand(&mut self, tick: i32) {
        self.mario_shown(None);
        self.mario_anim("Stand", tick, false);
        self.mario = MarioState::Stand;
        self.mario_until = tick + 1;
    }

    /// Go (`Go`, 128 f, 0x0251f028): he steps into the course; `M3_Wipe` 80 frames in.
    pub fn mario_go(&mut self, tick: i32) {
        self.mario_shown(None);
        self.mario_anim("Go", tick, false);
        self.mario = MarioState::Go;
        self.mario_until = tick + self.layout.ticks(&format!("{MARIO_PART}/Go"), 1);
        self.state_sounds(Some(MARIO_PART), "cState_Go");
    }

    pub fn mario_state(&self) -> MarioState {
        self.mario
    }

    /// "Is Wait" (0x0251f084).
    pub fn mario_waiting(&self) -> bool {
        self.mario == MarioState::Wait
    }

    /// Where Mario stands on the screen (layout units; the fade's circle closes there).
    pub fn mario_point(&mut self, tick: i32) -> [f32; 2] {
        self.layout.apply(tick);
        self.layout.rect(MARIO).map_or([0.0, 0.0], |r| r.center)
    }

    /// MoveMario held at `frame`.
    pub fn hold_mario(&mut self, frame: f32) {
        self.layout.hold("MoveMario", frame);
    }

    /// MoveMario from `from` on at speed 1, holding at `to`.
    pub fn move_mario(&mut self, from: f32, to: f32, tick: i32) {
        self.layout
            .play_ex("MoveMario", tick, from, Some(to), false, 1.0);
    }

    /// Mario standing on dot `i` (shown, Stand), or hidden in the start castle (`None`).
    pub fn place_mario(&mut self, i: Option<usize>) {
        match i {
            Some(i) => {
                let f = self.mario_frame(i);
                self.hold_mario(f);
                self.mario_shown(None);
                self.mario_anim("Stand", 0, false);
                self.mario = MarioState::Wait;
            }
            None => {
                self.hold_mario(0.0);
                self.mario_init();
            }
        }
    }

    // MARK: Goombas, castles

    pub fn kuribo_init(&mut self) {
        let l = &mut self.layout;
        l.stop(&format!("{KURIBO_PART}/Walk"));
        l.stop(&format!("{KURIBO_PART}/Default"));
        l.hold(&format!("{KURIBO_PART}/Stand"), 0.0);
    }

    /// Walk: Peach carried (`Default`) and the `Walk` loop (0x0251d7ec); their footsteps.
    pub fn kuribo_walk(&mut self, tick: i32) {
        let l = &mut self.layout;
        l.stop(&format!("{KURIBO_PART}/Stand"));
        l.play_loop(&format!("{KURIBO_PART}/Default"), tick);
        l.play_loop(&format!("{KURIBO_PART}/Walk"), tick);
        self.state_sounds(Some(KURIBO_PART), "cState_Walk");
    }

    /// Stand (0x0251d848).
    pub fn kuribo_stand(&mut self) {
        let l = &mut self.layout;
        l.stop(&format!("{KURIBO_PART}/Walk"));
        l.hold(&format!("{KURIBO_PART}/Stand"), 0.0);
    }

    /// A castle part's `Anim` (20 f, 0x0251b134).
    pub fn castle_anim(&mut self, c: usize, tick: i32) {
        let a = format!("{}/Anim", CASTLES[c]);
        self.layout.play(&a, tick);
        self.castles[c] = tick + self.layout.ticks(&a, 1);
    }

    /// The castle part is back in Wait (0x0251b188).
    pub fn castle_waiting(&self, c: usize, tick: i32) -> bool {
        tick >= self.castles[c]
    }

    // MARK: course dots

    /// Dot setup (0x0251c240): `pop` = type 1 (the stamp group hidden, `Type` frame 1);
    /// `stamp` = the clear stamp shown (`Clear` at its end); `instant` = no In animation: a
    /// type-1 dot is then hidden, a type-0 dot shown [obs; Type frame 0 shows `N_Clear_00`,
    /// the stamp is `Clear`'s scale 0 → 1, data].
    pub fn dot_setup(&mut self, i: usize, pop: bool, stamp: bool, instant: bool, tick: i32) {
        if i >= self.dots.len() {
            return;
        }
        let d = self.dot(i);
        let l = &mut self.layout;
        l.stop(&format!("{d}/Out"));
        l.hold(&format!("{d}/Type"), if pop { 1.0 } else { 0.0 });
        if instant {
            if pop {
                l.hold(&format!("{d}/In"), 0.0);
                l.hold(&format!("{d}/Clear"), 0.0);
            } else {
                l.hold_end(&format!("{d}/In"));
                if stamp {
                    l.hold_end(&format!("{d}/Clear"));
                } else {
                    l.hold(&format!("{d}/Clear"), 0.0);
                }
            }
            self.dots[i] = (DotState::Wait, tick);
        } else {
            l.hold(&format!("{d}/Clear"), 0.0);
            l.play(&format!("{d}/In"), tick);
            let n = l.ticks(&format!("{d}/In"), 1);
            self.dots[i] = (DotState::In, tick + n);
        }
    }

    /// Hide a dot (0x0251c3d0): one popping in goes back to In frame 0, a shown one plays Out
    /// (8 f) [obs].
    pub fn dot_hide(&mut self, i: usize, tick: i32) {
        if i >= self.dots.len() {
            return;
        }
        let d = self.dot(i);
        let l = &mut self.layout;
        match self.dots[i].0 {
            DotState::Init => {}
            DotState::In => {
                l.hold(&format!("{d}/In"), 0.0);
                self.dots[i] = (DotState::Init, tick);
            }
            _ => {
                l.play(&format!("{d}/Out"), tick);
                let n = l.ticks(&format!("{d}/Out"), 1);
                self.dots[i] = (DotState::Out, tick + n);
            }
        }
    }

    /// A dot that is hidden from the start.
    fn dot_hidden(&mut self, i: usize) {
        let d = self.dot(i);
        let l = &mut self.layout;
        l.stop(&format!("{d}/Out"));
        l.hold(&format!("{d}/In"), 0.0);
        l.hold(&format!("{d}/Clear"), 0.0);
        self.dots[i] = (DotState::Init, 0);
    }

    fn dot_waiting(&self, i: usize) -> bool {
        self.dots.get(i).is_some_and(|d| d.0 == DotState::Wait)
    }

    fn dot_hidden_now(&self, i: usize) -> bool {
        self.dots.get(i).is_none_or(|d| d.0 == DotState::Init)
    }

    /// The dots when the map is shown (vtable +0x8c = 0x024f2214) [obs]: k = dot (+1 with an
    /// advance pending); the dots before k cleared (stamp), dot k the next course when a run is
    /// in progress (hidden while its advance is pending: it pops in later), the rest hidden.
    pub fn show_dots(&mut self, dot: usize, advance: bool, in_progress: bool) {
        let k = (dot + advance as usize) % self.count.max(1);
        for i in 0..self.count {
            if i < k {
                self.dot_setup(i, false, true, true, 0);
            } else if i == k && in_progress && !advance {
                self.dot_setup(i, false, false, true, 0);
            } else {
                self.dot_hidden(i);
            }
        }
    }

    /// One tick of the parts' one-shots.
    pub fn update(&mut self, tick: i32) {
        if matches!(
            self.mario,
            MarioState::Jump
                | MarioState::Out
                | MarioState::TwoJump
                | MarioState::Stand
                | MarioState::Go
        ) && tick >= self.mario_until
        {
            if self.mario == MarioState::Out {
                self.mario_init();
            } else {
                self.mario = MarioState::Wait;
            }
        }
        for d in &mut self.dots {
            match d.0 {
                DotState::In if tick >= d.1 => d.0 = DotState::Wait,
                DotState::Out if tick >= d.1 => d.0 = DotState::Init,
                _ => {}
            }
        }
    }

    fn state_sounds(&self, part: Option<&str>, state: &str) {
        let ui = self.layout.ui();
        ui.push_cues(ui.smm1_state(&self.layout.name, part, state, &[]));
    }

    /// The music's beats of this tick start the bounces they arm.
    pub fn beat(&mut self, ev: &BeatEvents, tick: i32) {
        self.beats.step(&mut self.layout, ev, tick);
    }

    /// One tick of the map's own music clock (the scene's tune), if it has one.
    pub fn run_clock(&mut self, tick: i32) {
        if let Some(c) = &mut self.clock {
            let ev = c.step();
            self.beats.step(&mut self.layout, &ev, tick);
        }
    }
}

// MARK: - music

/// The select's music: Course World's stream with the difficulty's track (Easy and Normal 2,
/// Expert 3, Super Expert 6; mask 1 << (value − 1)) [obs 0x02580cb0, 0x02581200].
pub fn select_music(d: usize) -> String {
    let t = [2, 2, 3, 6][d.min(3)];
    format!("BGM_NetworkMainScene_{t}")
}

/// The between-course map's music by difficulty (0x0270864c) [obs].
pub fn map_music(d: usize) -> &'static str {
    [
        "BGM_100MarioMap",
        "BGM_100MarioMap",
        "BGM_100MarioMapExpert",
        "BGM_100MarioMapSuperExpert",
    ][d.min(3)]
}

/// Start the menu music `name` of the SMM1 import.
pub fn play_music(ui: &Ui, name: &str) {
    ui.push_cues(vec![SoundCue::music(name, ui.kind())]);
}

/// What the run's data says about a difficulty (asked by the select screen).
pub trait RunQuery {
    fn unlocked(&self, d: usize) -> bool;
    fn can_continue(&self, d: usize) -> bool;
    /// The Continue record (lives, courses cleared).
    fn continue_info(&self, d: usize) -> Option<(u32, usize)>;
    /// Courses in a run (the map's dots); 100 Mario's fixed lengths.
    fn run_length(&self, d: usize) -> usize {
        RUN_LENGTHS[d.min(3)]
    }
    /// Lives a New Game starts with.
    fn new_game_lives(&self, _d: usize) -> u32 {
        100
    }
    /// The rule line under New Game (None: "N courses, 100 lives").
    fn rule_text(&self, _d: usize) -> Option<String> {
        None
    }
    /// The map dot of the course after `cleared` clears (an endless run goes round the map).
    fn map_dot(&self, cleared: usize) -> usize {
        cleared
    }
    /// The map has shown the last clear of difficulty `d`'s run (the save flag 0x02084060; a
    /// run saved at the course clear has not): its next map shows Mario walking on.
    fn clear_shown(&self, _d: usize) -> bool {
        true
    }
}

/// What the SMM1 screens ask of the run.
#[derive(Debug)]
pub enum Smm1Event {
    Exit,
    /// Start a run (New Game / Continue), handing the map and title bar on.
    Start {
        difficulty: usize,
        resume: bool,
    },
    Locked(usize),
    BackToSelect,
    Play,
}

// MARK: - difficulty select

pub const TABS: [&str; 4] = [
    "L_Sub_00/L_ListSelectBtnSub_10",
    "L_Sub_00/L_ListSelectBtnSub_00",
    "L_Sub_00/L_ListSelectBtnSub_01",
    "L_Sub_00/L_ListSelectBtnSub_08",
];

/// The map's start buttons by focus: L_StartBtn_01 = Continue (upper), L_StartBtn_00 = New Game.
const START_BUTTONS: [&str; 2] = ["L_StartBtn_01", "L_StartBtn_00"];

/// `Nwk_ListTitle_00` (difficulty tabs) over the selected world map in its pre-MapIn state,
/// with New Game / Continue.
pub struct ChallengeMenuScene {
    ui: Ui,
    pub title: Layout,
    maps: Vec<Option<WorldMap>>,
    leaving: Option<(WorldMap, i32)>,
    pub difficulty: usize,
    /// 0 Continue (upper button), 1 New Game.
    focus: usize,
    pub tick: i32,
    /// Input waits until the title bar's and the map's entry animations are over
    /// (`cAppearRecommend` waits for both, 0x02580e78) and while the maps slide after a tab
    /// change (64 frames) [obs; the slide's wait inf].
    ready_at: i32,
    /// Course World's music's beats (its tracks share them).
    clock: Option<BeatClock>,
}

impl ChallengeMenuScene {
    pub fn new(ui: &Ui, difficulty: usize, q: &dyn RunQuery) -> Option<Self> {
        let mut t = ui.layout("Nwk_ListTitle_00")?;
        t.play("In", 0);
        t.play_loop("Default", 0);
        t.hold("MapIn", 0.0);
        t.hold("L_Sub_00/ListTitleType", 0.0); // frame 0: the 100 Mario list
        // OpenType 0/1/2 shows 2/3/4 tabs; all four are shown, locked ones greyed
        t.hold("L_Sub_00/OpenType", 2.0);
        let mut s = Self {
            ui: ui.clone(),
            title: t,
            maps: (0..4).map(|_| None).collect(),
            leaving: None,
            difficulty,
            focus: 1,
            tick: 0,
            ready_at: 0,
            clock: BeatClock::for_music(ui, &select_music(difficulty)),
        };
        s.prepare(difficulty, q)?;
        if let Some(m) = &mut s.maps[difficulty] {
            // entering from the top: the title bar's In (40) and the map's In (48, entry type
            // 3) [obs 0x02580e78]
            m.play_inout("In", 0);
            s.ready_at = m.layout.ticks("In", 0);
        }
        s.ready_at = s.ready_at.max(s.title.ticks("In", 0));
        s.update_tabs(q);
        play_music(ui, &select_music(difficulty));
        Some(s)
    }

    /// The current map and the title bar, for the MapIn that follows (it continues them), and
    /// whether the map showed a Continue (its counter leaves in the New Game demo).
    pub fn into_handoff(mut self, q: &dyn RunQuery) -> Option<(WorldMap, Layout, bool)> {
        let had = q.can_continue(self.difficulty);
        let mut m = self.maps[self.difficulty].take()?;
        // the music plays on into the map's demo
        m.clock = self.clock.take();
        Some((m, self.title, had))
    }

    /// Input is taken (the entry and tab animations are over).
    pub fn ready(&self) -> bool {
        self.tick >= self.ready_at
    }

    fn prepare(&mut self, d: usize, q: &dyn RunQuery) -> Option<()> {
        if self.maps[d].is_none() {
            let m = WorldMap::new(&self.ui, d)?;
            self.maps[d] = Some(m);
            self.refresh(d, q);
        }
        Some(())
    }

    /// Selection state of a map (0x024f4668 / 0x024f555c): MapIn frame 0; with a Continue
    /// Peach on the castle (AnimB at its end), the counters, Mario on his dot (one back when
    /// the last clear was not shown) and the cleared dots stamped; without one 0 and 100,
    /// Mario in the castle and no dots [obs].
    fn refresh(&mut self, d: usize, q: &dyn RunQuery) {
        let cont = q.continue_info(d);
        let shown = q.clear_shown(d);
        let Some(m) = &mut self.maps[d] else { return };
        m.layout.hold("MapIn", 0.0);
        m.set_counters(cont.map_or(100, |c| c.0), cont.map_or(0, |c| c.1));
        match cont {
            Some((_, n)) => {
                let mut dot = q.map_dot(n);
                let advance = !shown && dot > 0;
                if advance {
                    dot -= 1;
                }
                m.layout.hold_end("AnimB");
                m.place_mario(Some(dot));
                m.show_dots(dot, advance, dot > 0);
            }
            None => {
                m.layout.hold("AnimB", 0.0);
                m.place_mario(None);
                m.show_dots(0, false, false);
            }
        }
        self.update_buttons(q);
    }

    fn state(l: &mut Layout, tick: i32, part: &str, anim: &str, frame: Option<f32>) {
        for a in ["OnOff", "Valid", "Invalid", "Decide", "Cancel"] {
            if a != anim {
                l.stop(&format!("{part}/{a}"));
            }
        }
        let p = format!("{part}/{anim}");
        match frame {
            Some(f) => l.hold(&p, f),
            None => l.play(&p, tick),
        }
    }

    fn update_tabs(&mut self, q: &dyn RunQuery) {
        for (d, tab) in TABS.iter().enumerate() {
            if d == self.difficulty {
                let f = self.title.length(&format!("{tab}/Decide"));
                Self::state(&mut self.title, self.tick, tab, "Decide", Some(f));
            } else {
                let a = if q.unlocked(d) { "Valid" } else { "Invalid" };
                Self::state(&mut self.title, self.tick, tab, a, Some(1.0));
            }
        }
    }

    fn update_buttons(&mut self, q: &dyn RunQuery) {
        let d = self.difficulty;
        let open = q.unlocked(d);
        let cont = open && q.can_continue(d);
        if !cont && self.focus == 0 {
            self.focus = 1;
        }
        let (focus, tick) = (self.focus, self.tick);
        let Some(m) = &mut self.maps[d] else { return };
        for (i, part) in START_BUTTONS.iter().enumerate() {
            let enabled = open && (i == 1 || cont);
            if !enabled {
                Self::state(&mut m.layout, tick, part, "Invalid", Some(1.0));
            } else if i == focus {
                Self::state(&mut m.layout, tick, part, "OnOff", Some(3.0));
            } else {
                Self::state(&mut m.layout, tick, part, "Valid", Some(1.0));
            }
        }
    }

    pub fn step(&mut self, input: &UiInput, q: &dyn RunQuery) -> Option<Smm1Event> {
        self.tick += 1;
        if self.leaving.as_ref().is_some_and(|l| self.tick >= l.1) {
            self.leaving = None;
        }
        if let Some(c) = &mut self.clock {
            let ev = c.step();
            let t = self.tick;
            for m in self.maps.iter_mut().flatten() {
                m.beat(&ev, t);
            }
            if let Some((m, _)) = &mut self.leaving {
                m.beat(&ev, t);
            }
        }
        self.maps[self.difficulty].as_ref()?;
        if !self.ready() {
            return None;
        }
        let mut p = input.pressed;
        if let Some(c) = input.click {
            p = p | self.click(c, q);
        }
        // L / R: the difficulty tabs too
        if p.has(Buttons::L) {
            p = p | Buttons::LEFT;
        }
        if p.has(Buttons::R) {
            p = p | Buttons::RIGHT;
        }
        if p.has(Buttons::LEFT) || p.has(Buttons::RIGHT) {
            let right = p.has(Buttons::RIGHT);
            let d = if right {
                (self.difficulty + 1).min(3)
            } else {
                self.difficulty.saturating_sub(1)
            };
            self.switch_to(d, q);
            return None;
        }
        let d = self.difficulty;
        let can_continue = q.can_continue(d);
        let was = self.focus;
        if p.has(Buttons::UP) && can_continue {
            self.focus = 0;
            self.update_buttons(q);
        }
        if p.has(Buttons::DOWN) {
            self.focus = 1;
            self.update_buttons(q);
        }
        if self.focus != was
            && let Some(m) = &self.maps[d]
        {
            // [inf] the cursor moving onto a button plays its touch-select sound
            key(
                &m.layout,
                Some(START_BUTTONS[self.focus]),
                "TouchSelect",
                &[],
            );
        }
        if p.has(Buttons::BACK) {
            // [inf] Course World's back button (the select is a Course World page)
            let ui = &self.ui;
            ui.push_cues(ui.smm1_key("Nwk_ListFrame_01", Some("L_BackBtn_00"), "Decide", &[]));
            return Some(Smm1Event::Exit);
        }
        if p.has(Buttons::CONFIRM) {
            if !q.unlocked(d) {
                return Some(Smm1Event::Locked(d));
            }
            let resume = self.focus == 0 && can_continue;
            if let Some(m) = &mut self.maps[d] {
                m.layout.hold("OutL", 0.0);
                key(&m.layout, Some(START_BUTTONS[self.focus]), "Decide", &[]);
            }
            return Some(Smm1Event::Start {
                difficulty: d,
                resume,
            });
        }
        None
    }

    /// Tests: where start button `part` of the current map is drawn.
    #[cfg(test)]
    pub(crate) fn start_button_center(&mut self, part: &str) -> Option<[f32; 2]> {
        let m = self.maps[self.difficulty].as_mut()?;
        m.layout.apply(self.tick);
        m.layout.rect(&format!("{part}/B_Btn_00")).map(|r| r.center)
    }

    /// Slide over to difficulty `d`'s map: to Easy, or one step down, the old map plays OutR
    /// and the new one InR; otherwise OutL / InL (64 frames each) [obs 0x02581200].
    fn switch_to(&mut self, d: usize, q: &dyn RunQuery) {
        if d == self.difficulty || self.prepare(d, q).is_none() {
            return;
        }
        let old = self.difficulty;
        let r = d == 0 || (d == 1 && old == 2) || (d == 2 && old == 3);
        let tick = self.tick;
        // the tab's Decide: SE_NetworkLeft and the curtain closing and opening
        key(&self.title, Some(TABS[d]), "Decide", &[]);
        if let Some(mut m) = self.maps[old].take() {
            m.play_inout(if r { "OutR" } else { "OutL" }, tick);
            let n = m.layout.ticks(if r { "OutR" } else { "OutL" }, 1);
            self.leaving = Some((m, tick + n));
        }
        self.difficulty = d;
        self.refresh(d, q);
        if let Some(n) = &mut self.maps[d] {
            let a = if r { "InR" } else { "InL" };
            n.play_inout(a, tick);
            self.ready_at = tick + n.layout.ticks(a, 0);
        }
        self.update_tabs(q);
        // the stream's other track, at once [obs 0x02581200]
        let mut cue = SoundCue::music(&select_music(d), self.ui.kind());
        cue.kind = crate::sound::CueKind::MusicTrack;
        self.ui.push_cues(vec![cue]);
    }

    /// A click: a difficulty tab (its map slides in), Continue / New Game (focuses and
    /// decides it). The buttons to add as if pressed.
    fn click(&mut self, c: [f32; 2], q: &dyn RunQuery) -> Buttons {
        let tick = self.tick;
        let tabs: Vec<String> = TABS.iter().map(|t| format!("{t}/B_Btn_00")).collect();
        if let Some(d) = hit(&mut self.title, tick, &tabs, c) {
            self.switch_to(d, q);
            return Buttons::NONE;
        }
        let d = self.difficulty;
        let starts = START_BUTTONS.map(|b| format!("{b}/B_Btn_00"));
        let Some(m) = &mut self.maps[d] else {
            return Buttons::NONE;
        };
        match hit(&mut m.layout, tick, &starts, c) {
            Some(0) if q.can_continue(d) => {
                self.focus = 0;
                self.update_buttons(q);
                Buttons::CONFIRM
            }
            Some(1) => {
                self.focus = 1;
                self.update_buttons(q);
                Buttons::CONFIRM
            }
            _ => Buttons::NONE,
        }
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let t = self.tick;
        if let Some((m, _)) = &mut self.leaving {
            push_layout(out, &mut m.layout, t);
        }
        if let Some(m) = &mut self.maps[self.difficulty] {
            push_layout(out, &mut m.layout, t);
        }
        push_layout(out, &mut self.title, t);
    }
}

// MARK: - world map demos

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Course World's New Game (state 0x43 → 0x44 with "first").
    NewGame,
    /// Course World's Continue.
    Resume,
    /// NetWorldMapScene after a clear.
    AfterClear,
    /// Straight to the next course (after a skip: no map) [inf].
    Direct,
}

/// The map's state machine (field +0x1e8; ids as the game registers them) [obs, state table
/// of class 0x024f2ae0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapState {
    Init,
    FadeWait,
    Appear,
    Wait,
    DemoMapIn,
    TelopIn,
    AnimA,
    MoveKuribo,
    AnimCastle,
    AnimB,
    AppearMarioFirst,
    AppearMario,
    AppearTwoJump,
    NextMapPoint,
    WalkAndStand,
    WalkMario,
    DisappearMario,
    Cancel,
    ChangeScene,
}

/// Who shows the map: Course World (its states 0x43 `cMarioClubModePlayDemo0` and 0x44
/// `…Demo1`) or the between-course NetWorldMapScene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shower {
    CourseWorldMapIn,
    CourseWorldDemo,
    NetMap,
}

/// The minimum time Mario's Go holds the map before the scene changes (+0x274 := 60,
/// 0x024f6be0) [obs].
const GO_MIN: i32 = 60;

/// Between the select and a course: the world map's demos the way the map class runs them
/// (research/specs/smm1-100mario-screens.md §2): New Game (MapIn with the title bar, the
/// banner, the Goombas carrying Peach to the castle, Mario jumping out of the start castle to
/// the first dot), Continue (MapIn, Mario's two jumps, a walk when a clear was not shown yet),
/// after a clear (Mario walks on to the next dot); each ends with Mario's Go. A skips to the
/// Go. The course is reached through a fade the flow runs ([`MapIntroScene::leave_fade`]).
pub struct MapIntroScene {
    ui: Ui,
    pub map: WorldMap,
    title: Option<Layout>,
    shower: Shower,
    state: MapState,
    state_start: i32,
    fresh: bool,
    pub tick: i32,
    /// +0x280 Mario's dot, +0x290 a Continue exists, +0x28e an advance is pending (the last
    /// clear not shown yet), +0x28d the New Game demo ("first"), +0x28f the demo flag (shown
    /// at MapIn's end), +0x295 skipped, +0x28c done.
    dot: usize,
    cont: bool,
    advance: bool,
    /// +0x296: the pending advance has not moved Mario's dot yet.
    advance_todo: bool,
    first: bool,
    demo: bool,
    cancel: bool,
    done: bool,
    go_timer: i32,
    lives: u32,
    /// The map has just shown the run's last clear (the game writes its save flag then,
    /// 0x020844e0): the flow records it.
    pub clear_shown: bool,
    finished: bool,
}

impl MapIntroScene {
    /// `cleared`: courses cleared in the run (the saved count); `shown`: whether the map has
    /// already shown the last of them (the "clear shown" save flag, 0x02084060); `had_continue`:
    /// the select showed a Continue before New Game (its counter leaves then).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ui: &Ui,
        entry: Entry,
        difficulty: usize,
        lives: u32,
        cleared: usize,
        shown: bool,
        had_continue: bool,
        handoff: Option<(WorldMap, Layout)>,
    ) -> Option<Self> {
        let usable = handoff.filter(|h| h.0.difficulty == difficulty);
        let (map, title) = match (entry, usable) {
            (Entry::NewGame | Entry::Resume, Some((m, t))) => (m, Some(t)),
            _ => (WorldMap::new(ui, difficulty)?, None),
        };
        let mut s = Self {
            ui: ui.clone(),
            map,
            title,
            shower: Shower::NetMap,
            state: MapState::Init,
            state_start: 0,
            fresh: false,
            tick: 0,
            dot: 0,
            cont: false,
            advance: false,
            advance_todo: false,
            first: false,
            demo: false,
            cancel: false,
            done: false,
            go_timer: 0,
            lives,
            clear_shown: false,
            finished: false,
        };
        let count = s.map.count;
        match entry {
            Entry::NewGame | Entry::Resume => {
                // the select's map as it was shown (0x024f4668 / 0x024f555c)
                s.shower = Shower::CourseWorldMapIn;
                let resume = entry == Entry::Resume;
                s.cont = if resume { true } else { had_continue };
                s.dot = if s.cont { cleared } else { 0 };
                if s.cont && !shown && s.dot > 0 {
                    s.dot -= 1;
                    s.advance = true;
                    s.advance_todo = true;
                }
                if s.title.is_none() {
                    s.show_select_pose(cleared);
                }
                s.first = !resume;
                s.set(MapState::DemoMapIn);
            }
            Entry::AfterClear | Entry::Direct => {
                // NetWorldMapScene: built with the clear not shown, shown at MapIn's end with
                // the demo flag (0x024f27d0) [obs]
                s.cont = cleared > 0 && cleared < count;
                s.dot = cleared;
                if !shown && s.dot > 0 {
                    s.dot -= 1;
                    s.advance = true;
                    s.advance_todo = true;
                }
                s.demo = true;
                s.show_select_pose(cleared);
                s.set(MapState::FadeWait);
                // the scene's Wait starts the map's music (0x0270864c) [obs]; its streams have
                // no beat end and no loop, so its maps don't bounce (0x02089898) [obs]
                play_music(ui, map_music(difficulty));
                s.map.clock = BeatClock::for_music(ui, map_music(difficulty));
            }
        }
        Some(s)
    }

    /// The map's shown state (0x024f555c + the dots, vtable +0x8c).
    fn show_select_pose(&mut self, cleared: usize) {
        let m = &mut self.map;
        if self.demo {
            m.layout.hold_end("MapIn");
        } else {
            m.layout.hold("MapIn", 0.0);
        }
        if self.cont {
            m.layout.hold_end("AnimB");
        } else {
            m.layout.hold("AnimB", 0.0);
        }
        let (lives, n) = if self.cont {
            (self.lives, cleared)
        } else {
            (100, 0)
        };
        m.set_counters(lives, n);
        if self.cont {
            m.place_mario(Some(self.dot));
        } else {
            m.place_mario(None);
        }
        let in_progress = self.cont && self.dot > 0;
        m.show_dots(self.dot, self.advance, in_progress);
    }

    /// Change state: the new state's first frame is the next one ("frames in state" 0,
    /// field +8, through the change-state helper 0x024f7a14) [obs].
    fn set(&mut self, s: MapState) {
        self.state = s;
        self.fresh = true;
    }

    pub fn state(&self) -> MapState {
        self.state
    }

    /// The fade to the course and where its circle closes: from Course World every scene
    /// request's own type 1 (the map's type 7 is set before the request, which resets it,
    /// 0x0258eb34 → 0x02766e1c → 0x0282dce0); from NetWorldMapScene type 7 on Mario
    /// (0x027089d8) [obs].
    pub fn leave_fade(&mut self) -> (crate::smm1_fade::Smm1FadeKind, [f32; 2]) {
        use crate::smm1_fade::Smm1FadeKind;
        let p = self.map.mario_point(self.tick);
        match self.shower {
            Shower::NetMap => (Smm1FadeKind::MAP_TO_COURSE, p),
            _ => (Smm1FadeKind::SCENE, p),
        }
    }

    /// `faded_in`: the scene fade has finished revealing the map (FadeWait waits for it).
    pub fn step(&mut self, input: &UiInput, faded_in: bool) -> Option<Smm1Event> {
        self.tick += 1;
        if self.finished {
            return None;
        }
        let tick = self.tick;
        self.map.update(tick);
        self.map.run_clock(tick);
        // A: the map's ShortCutA (accepted once the next course is ready: at once here)
        // [obs 0x0256fff4 / 0x025a05f0; a click / tap does the same, inf]
        let a = input.pressed.has(Buttons::CONFIRM) || input.click.is_some();
        if a && !self.cancel && self.state != MapState::FadeWait {
            self.cancel = true;
        }
        self.exec(faded_in);
        match self.shower {
            Shower::CourseWorldMapIn => {
                let title_done = self
                    .title
                    .as_mut()
                    .is_none_or(|t| !t.is_playing("MapIn") || t.finished("MapIn", tick));
                if self.state == MapState::Wait && title_done {
                    // 0x44: New Game reloads the dot (0) and the Continue flag (0x024f28b0),
                    // Continue keeps them (0x024f2924) [obs]
                    if self.first {
                        self.dot = 0;
                        self.cont = false;
                        self.advance = false;
                        self.advance_todo = false;
                    }
                    self.shower = Shower::CourseWorldDemo;
                    self.set(MapState::AnimA);
                }
            }
            Shower::CourseWorldDemo | Shower::NetMap => {
                if self.done {
                    self.finished = true;
                    return Some(Smm1Event::Play);
                }
            }
        }
        None
    }

    /// One frame of the current state (the functions 0x024f573c … 0x024f6f24).
    fn exec(&mut self, faded_in: bool) {
        let tick = self.tick;
        let enter = std::mem::take(&mut self.fresh);
        if enter {
            self.state_start = tick;
        }
        let cancel = self.cancel;
        match self.state {
            MapState::Init | MapState::Wait | MapState::ChangeScene => {}
            MapState::FadeWait => {
                if faded_in {
                    self.set(MapState::Appear);
                }
            }
            MapState::Appear => {
                // entry type 0: shown at once (OutL frame 0), then the demo [obs 0x024f5a3c]
                self.map.layout.hold("OutL", 0.0);
                key(&self.map.layout, None, "In", &[]);
                self.set(if self.demo {
                    MapState::AppearMario
                } else {
                    MapState::Wait
                });
            }
            MapState::DemoMapIn => self.demo_map_in(enter, cancel),
            MapState::AnimA => {
                let m = &mut self.map;
                if enter && self.first {
                    // 0x024f51c0: the counters (with no Continue: 0 and 100); CountIn only
                    // with a Continue flag [obs]
                    let (lives, n) = if self.cont {
                        (self.lives, self.dot)
                    } else {
                        (100, 0)
                    };
                    m.set_counters(lives, n);
                    if self.cont {
                        m.count_in(tick);
                    }
                }
                if cancel {
                    if self.first && self.cont {
                        m.layout.hold_end("CountIn");
                    }
                    self.begin_cancel();
                } else if !self.first {
                    self.set(MapState::AppearTwoJump);
                } else if !m.layout.is_playing("CountIn") || m.layout.finished("CountIn", tick) {
                    self.set(MapState::TelopIn);
                }
            }
            MapState::TelopIn => {
                if enter {
                    let m = &mut self.map;
                    for i in 0..m.count {
                        m.dot_setup(i, true, false, true, tick);
                    }
                    m.mario_init();
                    m.hold_mario(0.0);
                    m.layout.play("Telop", tick);
                    self.state_sounds("c1stDemoTelopIn");
                }
                if cancel {
                    self.map.layout.hold_end("Telop");
                    self.begin_cancel();
                } else if self.map.layout.finished("Telop", tick) {
                    self.set(MapState::MoveKuribo);
                }
            }
            MapState::MoveKuribo => {
                let m = &mut self.map;
                if enter {
                    m.kuribo_stand();
                    m.castle_anim(0, tick);
                    m.layout.play("MoveKuribo", tick);
                }
                if cancel {
                    m.kuribo_init();
                    m.layout.hold_end("MoveKuribo");
                    self.begin_cancel();
                    return;
                }
                // the Goombas walk from MoveKuribo frame 60 and stand at 156 (Easy), 268
                // (Normal / Expert) or 174 (Super Expert) [obs 0x024f6220, constants
                // 0x1009b428 / 0x1009b430 / 0x1009b438 / 0x1009b434]
                let f = (tick - self.state_start) as f32;
                let stand = match m.difficulty {
                    0 => 156.0,
                    3 => 174.0,
                    _ => 268.0,
                };
                if f == FIRST_DOT {
                    m.kuribo_walk(tick);
                } else if f == stand {
                    m.kuribo_stand();
                }
                if m.layout.finished("MoveKuribo", tick) {
                    m.kuribo_init();
                    self.set(MapState::AnimCastle);
                }
            }
            MapState::AnimCastle => {
                if enter {
                    self.map.castle_anim(1, tick);
                }
                if cancel {
                    self.begin_cancel();
                } else if self.map.castle_waiting(1, tick) {
                    self.set(MapState::AnimB);
                }
            }
            MapState::AnimB => {
                if enter {
                    self.map.layout.play("AnimB", tick);
                    self.state_sounds("c1stDemoAnimB");
                }
                if cancel {
                    self.map.layout.hold_end("AnimB");
                    self.begin_cancel();
                } else if self.map.layout.finished("AnimB", tick) {
                    self.set(MapState::AppearMarioFirst);
                }
            }
            MapState::AppearMarioFirst => {
                let m = &mut self.map;
                if enter {
                    m.castle_anim(0, tick);
                    m.mario_jump(tick);
                    m.move_mario(0.0, CASTLE_JUMP, tick);
                }
                if cancel {
                    self.begin_cancel();
                } else if tick - self.state_start >= CASTLE_JUMP as i32 && m.mario_waiting() {
                    self.set(MapState::NextMapPoint);
                }
            }
            MapState::AppearMario | MapState::AppearTwoJump => {
                let m = &mut self.map;
                if enter {
                    if self.state == MapState::AppearTwoJump {
                        m.mario_two_jump(tick);
                    } else if m.mario_state() == MarioState::Init {
                        m.mario_jump(tick);
                    }
                    let f = m.mario_frame(self.dot);
                    m.hold_mario(f);
                }
                if cancel {
                    self.begin_cancel();
                } else if m.mario_waiting() {
                    if self.advance {
                        self.advance_todo = false;
                        self.dot += 1;
                        self.set(MapState::NextMapPoint);
                    } else {
                        self.set(MapState::DisappearMario);
                    }
                }
            }
            MapState::NextMapPoint => {
                if enter {
                    self.map.dot_setup(self.dot, true, false, false, tick);
                }
                if cancel {
                    self.begin_cancel();
                } else if self.map.dot_waiting(self.dot) {
                    self.set(if self.first {
                        MapState::WalkAndStand
                    } else {
                        MapState::WalkMario
                    });
                }
            }
            MapState::WalkAndStand | MapState::WalkMario => {
                let m = &mut self.map;
                let from = if self.state == MapState::WalkAndStand {
                    CASTLE_JUMP
                } else {
                    m.mario_frame(self.dot.saturating_sub(1))
                };
                let to = m.mario_frame(self.dot);
                if enter {
                    m.mario_walk(tick);
                    m.move_mario(from, to, tick);
                }
                if cancel {
                    self.begin_cancel();
                    return;
                }
                if from + (tick - self.state_start) as f32 >= to {
                    if m.mario_state() == MarioState::Walk {
                        m.mario_stand(tick);
                        m.hold_mario(to);
                    }
                    if m.mario_waiting() {
                        self.set(MapState::DisappearMario);
                    }
                }
            }
            MapState::Cancel => {
                // the end poses [obs 0x024f6d30]: an advance not yet made is made (the next
                // dot pops in); the New Game demo's first dot pops in and Peach sits on the
                // castle; MoveMario on Mario's dot
                let m = &mut self.map;
                if self.advance_todo {
                    self.dot = (self.dot + 1).min(m.count.saturating_sub(1));
                    self.advance_todo = false;
                    m.dot_setup(self.dot, true, false, false, tick);
                } else if self.first {
                    m.dot_setup(self.dot, true, false, false, tick);
                    m.layout.hold_end("AnimB");
                }
                let f = m.mario_frame(self.dot);
                m.hold_mario(f);
                self.state_sounds("cDemoCancel");
                self.cancel = false;
                self.set(MapState::DisappearMario);
            }
            MapState::DisappearMario => {
                let m = &mut self.map;
                if enter {
                    m.mario_go(tick);
                    self.go_timer = GO_MIN;
                    // the clear is shown now (its save flag, 0x020844e0) [obs]
                    if self.advance {
                        self.clear_shown = true;
                    }
                }
                self.go_timer -= 1;
                if self.go_timer <= 0 && m.mario_waiting() {
                    m.mario_init();
                    self.done = true;
                    self.set(MapState::ChangeScene);
                }
            }
        }
    }

    /// DemoMapIn (0x024f5e28): the New Game demo first hides the dots, sends Mario and (with a
    /// Continue shown) the counter out and takes Peach off the castle; then MapIn [obs].
    fn demo_map_in(&mut self, enter: bool, cancel: bool) {
        let tick = self.tick;
        let m = &mut self.map;
        if enter {
            if self.first {
                for i in 0..m.count {
                    m.dot_hide(i, tick);
                }
                m.mario_out(tick);
                if self.cont {
                    m.count_out(tick);
                }
                m.layout.hold("AnimB", 0.0);
            }
            m.layout.play("MapIn", tick);
            // Course World 0x43: the title bar's MapIn starts on the same frame [obs]
            if let Some(t) = &mut self.title {
                t.play("MapIn", tick);
            }
        }
        if cancel {
            // A: MapIn to its end (the title bar's too: its skip flag +0x249, 0x024c415c
            // [inf: who sets it])
            m.layout.hold_end("MapIn");
            if let Some(t) = &mut self.title {
                t.hold_end("MapIn");
            }
            self.set(MapState::Wait);
            return;
        }
        if self.first {
            let hidden = (0..m.count).all(|i| m.dot_hidden_now(i));
            let count_done =
                !m.layout.is_playing("CountOut") || m.layout.finished("CountOut", tick);
            if !(hidden && count_done && m.mario_state() == MarioState::Init) {
                return;
            }
        }
        if m.layout.finished("MapIn", tick) {
            self.set(MapState::Wait);
        }
    }

    fn begin_cancel(&mut self) {
        self.set(MapState::Cancel);
    }

    fn state_sounds(&self, state: &str) {
        let ui = &self.ui;
        ui.push_cues(ui.smm1_state(&self.map.layout.name, None, state, &[]));
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let t = self.tick;
        push_layout(out, &mut self.map.layout, t);
        if let Some(l) = &mut self.title {
            push_layout(out, l, t);
        }
    }
}

// MARK: - game over, ending

/// GameOverScene: `Cmn_GameOver_00`'s `OneTime` (320 frames at 1.0, 0x024e9f84), then back
/// to Course World's select (0x02700d44 → NetworkMainScene2, whose return state shows the
/// 100 Mario select again, 0x0257f7a4). It reads no buttons [obs: the scene, its manager
/// 0x02599554 and the layout states].
pub struct GameOverScene {
    layout: Layout,
    pub tick: i32,
    done: bool,
}

impl GameOverScene {
    pub fn new(ui: &Ui) -> Option<Self> {
        let mut l = ui.layout("Cmn_GameOver_00")?;
        l.play("OneTime", 0);
        // GameOverScene's bank: M1_GameOver [obs data]
        play_music(ui, "M1_GameOver");
        Some(Self {
            layout: l,
            tick: 0,
            done: false,
        })
    }

    pub fn step(&mut self, _input: &UiInput) -> Option<Smm1Event> {
        self.tick += 1;
        if self.done {
            return None;
        }
        if self.tick >= self.layout.ticks("OneTime", 60) {
            self.done = true;
            return Some(Smm1Event::BackToSelect);
        }
        None
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        push_layout(out, &mut self.layout, self.tick);
    }
}

// MARK: - in-course overlays

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOverlayMode {
    None,
    Pause,
    Clear,
    Card,
    Skip,
}

/// Ticks the skip key must be held: the key icon's In (16 frames), then its three countdown
/// steps (32 each) until `cCountDownEnd`, which fires the skip (0x0250357c, 0x02503610,
/// 0x025460cc) [obs].
pub const SKIP_HOLD: i32 = 112;

/// How a course's intro card `Play_CourseIn100Mario_00` starts (its Init, 0x024e3590) [obs].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardStart {
    /// The first course of a new run: AnimA (the 100 Marios run in, 215 f), then AnimB.
    NewRun,
    /// The first course after Continue: AnimA at its end, then AnimB (70 f).
    Resume,
    /// The course again after a death or Start Over: AnimA at its end, AnimB, and `OutMiss`
    /// (60 f) instead of `Out`.
    Retry,
    /// Every other course: AnimB at its end and no In animation.
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardPhase {
    None,
    /// AnimA (`InAnim`), from the tick.
    AnimA(i32),
    /// AnimB (`InAnimAction`).
    AnimB(i32),
    /// Out / OutMiss (`OutAnim`), until the tick.
    Out(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Pause,
    Skip,
}

/// Layouts over a 100 Mario course: pause menu, course clear, the skip countdown and the intro
/// card after a death.
pub struct RunOverlay {
    ui: Ui,
    pub tick: i32,
    pub mode: RunOverlayMode,
    mode_start: i32,
    pause_l: Option<Layout>,
    clear_l: Option<Layout>,
    skip_l: Option<Layout>,
    card_l: Option<Layout>,
    closing: Option<(Which, i32)>,
    pub clear_focus: usize,
    clear_last: bool,
    pause_focus: usize,
    /// The course's game style (`CourseStatus::style`): the pause menu's In / Out sound.
    pub style: Option<u32>,
    sounds: Pending,
    card: CardPhase,
    card_retry: bool,
    card_lives: u32,
    /// The scene fader over the course (a death's and Start Over's fade to the card).
    pub fade: crate::smm1_fade::Smm1Fade,
    /// The course's stars and players for the pause menu, when known.
    pub counts: (Option<i64>, Option<i64>),
    /// The course clear is leaving: its Out and the button's Decide play until this tick.
    pub clear_leave_at: Option<i32>,
    /// Save & Quit's question over the pause menu.
    quit: Option<MessageWindow>,
}

pub const PAUSE_PARTS: [&str; 3] = ["L_CloseBtn_00", "L_RetryBtn_00", "L_ExitBtn_00"];
/// Course clear: Save & Quit, Next.
const CLEAR_PARTS: [&str; 2] = ["L_ExitBtn_00", "L_NextBtn_00"];

impl RunOverlay {
    pub fn new(ui: &Ui) -> Self {
        Self {
            ui: ui.clone(),
            tick: 0,
            mode: RunOverlayMode::None,
            mode_start: 0,
            pause_l: None,
            clear_l: None,
            skip_l: None,
            card_l: None,
            closing: None,
            clear_focus: 1,
            clear_last: false,
            pause_focus: 0,
            style: None,
            sounds: Pending::default(),
            card: CardPhase::None,
            card_retry: false,
            card_lives: 100,
            fade: crate::smm1_fade::Smm1Fade::new(ui),
            counts: (None, None),
            clear_leave_at: None,
            quit: None,
        }
    }

    pub fn since_mode(&self) -> i32 {
        self.tick - self.mode_start
    }

    /// One tick; true on the tick the fader has covered the screen.
    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.sounds.fire(&self.ui, self.tick);
        self.card_step();
        self.fade.step()
    }

    fn style_ctx(&self) -> Vec<Ctx<'static>> {
        self.style
            .map(|s| Ctx::Enum(style_name(s)))
            .into_iter()
            .collect()
    }

    fn set_mode(&mut self, m: RunOverlayMode) {
        self.mode = m;
        self.mode_start = self.tick;
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let t = self.tick;
        if let Some((w, until)) = self.closing
            && t < until
        {
            let l = match w {
                Which::Pause => self.pause_l.as_mut(),
                Which::Skip => self.skip_l.as_mut(),
            };
            if let Some(l) = l {
                push_layout(out, l, t);
            }
        }
        let l = match self.mode {
            RunOverlayMode::None => None,
            RunOverlayMode::Pause => self.pause_l.as_mut(),
            RunOverlayMode::Clear => self.clear_l.as_mut(),
            RunOverlayMode::Card => self.card_l.as_mut(),
            RunOverlayMode::Skip => self.skip_l.as_mut(),
        };
        if let Some(l) = l {
            push_layout(out, l, t);
        }
        if let Some(q) = &mut self.quit {
            q.draw(out);
        }
        self.fade.draw(out);
    }

    /// The pause's Save & Quit asks first (0x02413274): the message window with the pause
    /// menu's `Msg_0006` ("Exit game? (Your progress will be saved.)", message 0xb) and No / Yes,
    /// kept up after a Yes (window +0x285 := 1). False without the window layout.
    pub fn ask_quit(&mut self) -> bool {
        let msg = |l: &str| self.ui.message("Cmn_PauseMenu", l);
        let (Some(text), no, yes) = (
            msg("Msg_0006"),
            msg("BtnL_0006").unwrap_or_else(|| "No".into()),
            msg("BtnR_0006").unwrap_or_else(|| "Yes".into()),
        ) else {
            return false;
        };
        self.quit = MessageWindow::open(&self.ui, WindowKind::YesNo, &text, &[&no, &yes], true);
        self.quit.is_some()
    }

    /// The question is up.
    pub fn asking_quit(&self) -> bool {
        self.quit.is_some()
    }

    /// One tick of the question: Some(true) on Yes (result 4: the pause leaves at once, the
    /// window staying until the fade covers it, 0x024132f0 → state 0x26), Some(false) once a
    /// No has closed it (back to the pause menu, state 0x2c) [obs].
    pub fn quit_step(&mut self, input: &UiInput) -> Option<bool> {
        let q = self.quit.as_mut()?;
        match q.step(input) {
            Some(WindowEvent::Decided(true)) => Some(true),
            Some(WindowEvent::Closed) => {
                self.quit = None;
                Some(false)
            }
            _ => None,
        }
    }

    pub fn show_pause(&mut self, mode_line: &str, course: &str, maker: &str, selection: usize) {
        if self.pause_l.is_none()
            && let Some(mut p) = self.ui.layout("Cmn_PauseMenu_00")
        {
            // SMM1's 100 Mario pause is type 2: Type 0 (the network buttons — Star, comments —
            // over Start Over / Save & Quit) and TypeCourseInfo 1 (the mode icon, the stars
            // and players, the maker's Mii, name and flag) [obs 0x0240da10, table
            // 0x1008130c]. There is no star or comment service for these courses, so the
            // network buttons are left out and Type 6 places the play buttons as they stand
            // without them [departure]; the exit button reads "Save & Quit" (T_ExitBtn_01)
            p.hold("Type", 6.0);
            p.text_from("L_ExitBtn_00/T_Btn_00", "T_ExitBtn_01");
            p.hold("TypeCourseInfo", 1.0);
            // no country for the flag; the options are not a 100 Mario button [departure]
            for n in ["N_NetworkBtn_00", "L_NationalFlag_00", "L_OptionBtn_00"] {
                p.show(n, false);
            }
            self.pause_l = Some(p);
        }
        let t = self.tick;
        let ctx = self.style_ctx();
        let (stars, players) = self.counts;
        let Some(p) = &mut self.pause_l else { return };
        // the stars and players of the course data (shown as SMM1 shows the server's)
        for (icon, text, n) in [
            ("P_YeahCountIcon_00", "T_YeahCount_00", stars),
            ("P_PlayerCountIcon_00", "T_PlayerCount_00", players),
        ] {
            p.visible(icon, if n.is_some() { None } else { Some(false) });
            if let Some(n) = n {
                p.text(text, &n.to_string());
            }
        }
        p.text("T_Mode_00", mode_line);
        p.text("T_CourseName_00", course);
        p.text("T_MiiName_00", maker);
        p.stop("Out");
        p.play("In", t);
        // In: the style's pause sound (M1_Pose, MW_Pose, WU_SE_SYS_PAUSE_)
        key(p, None, "In", &ctx);
        self.closing = None;
        self.set_mode(RunOverlayMode::Pause);
        self.focus_pause(selection);
    }

    /// Tests: where pause pane `name` is drawn.
    #[cfg(test)]
    pub(crate) fn pause_rect(&mut self, name: &str) -> Option<crate::layout::Rect> {
        let t = self.tick;
        let p = self.pause_l.as_mut()?;
        p.apply(t);
        p.rect(name)
    }

    /// The cursor moved to pause button `i` (its touch-select sound [inf]).
    pub fn pause_select(&mut self, i: usize) {
        if i != self.pause_focus
            && let (Some(p), Some(part)) = (&self.pause_l, PAUSE_PARTS.get(i))
        {
            key(p, Some(part), "TouchSelect", &[]);
        }
        self.focus_pause(i);
    }

    /// Pause button `i` decided (Close: SE_BongoClose; Start Over, Save & Quit: SE_chukettei3).
    pub fn pause_decide(&mut self, i: usize) {
        if let (Some(p), Some(part)) = (&self.pause_l, PAUSE_PARTS.get(i)) {
            key(p, Some(part), "Decide", &[]);
        }
    }

    fn focus_pause(&mut self, i: usize) {
        self.pause_focus = i;
        let t = self.tick;
        let Some(p) = &mut self.pause_l else { return };
        for (k, part) in PAUSE_PARTS.iter().enumerate() {
            let a = format!("{part}/OnOff");
            if k == i {
                p.play(&a, t);
            } else {
                p.hold(&a, 0.0);
            }
        }
    }

    pub fn hide_pause(&mut self) {
        if self.mode != RunOverlayMode::Pause {
            return;
        }
        let t = self.tick;
        let ctx = self.style_ctx();
        let Some(p) = &mut self.pause_l else { return };
        p.stop("In");
        p.play("Out", t);
        key(p, None, "Out", &ctx);
        self.closing = Some((Which::Pause, t + p.ticks("Out", 1)));
        self.set_mode(RunOverlayMode::None);
    }

    pub fn show_clear(
        &mut self,
        course: &str,
        maker: &str,
        ticks: Option<i64>,
        world_record_ms: Option<i64>,
        last: bool,
    ) {
        if self.clear_l.is_none() {
            self.clear_l = self.ui.layout("Play_CourseClear_00");
        }
        let t = self.tick;
        self.clear_leave_at = None;
        let Some(c) = &mut self.clear_l else { return };
        self.clear_last = last;
        // 100 Mario variant: Save & Quit + Next; ClearType centres Next alone (the run's last
        // course) [inf]
        if last {
            c.hold("ClearType", 1.0);
        } else {
            c.hold("ModeType", 0.0);
        }
        for n in [
            "N_MiiverseComment_01",
            "N_RcmText_00",
            "N_RcmCourseBtn_02",
            "N_CommentGoodBtn_00",
        ] {
            c.show(n, false);
        }
        c.show("N_Player_00/L_NF_00", false);
        c.show("L_NF_00", false);
        c.text("T_CourseName_00", course);
        c.text("T_PlayerName_00", maker);
        let put = |c: &mut Layout, main: &str, frac: &str, ms: Option<i64>| match ms {
            None => {
                c.text(main, "--’--”");
                c.text(frac, "---");
            }
            Some(ms) => {
                c.text(main, &format!("{:02}’{:02}”", ms / 60000, ms / 1000 % 60));
                c.text(frac, &format!("{:03}", ms % 1000));
            }
        };
        put(c, "T_Time_02", "T_Time_03", ticks.map(|t| t * 1000 / 60));
        put(c, "T_Time_00", "T_Time_01", world_record_ms);
        c.play("In", t);
        c.play_loop("Default", t);
        key(c, None, "In", &[]); // SE_CourseClearFilter
        self.set_mode(RunOverlayMode::Clear);
        self.clear_focus = 1;
        self.clear_focus_to(1);
    }

    /// The cursor moved to course-clear button `i` (its touch-select sound [inf]).
    pub fn clear_select(&mut self, i: usize) {
        let was = self.clear_focus;
        self.clear_focus_to(i);
        if self.clear_focus != was
            && let Some(c) = &self.clear_l
        {
            key(c, Some(CLEAR_PARTS[self.clear_focus]), "TouchSelect", &[]);
        }
    }

    fn clear_focus_to(&mut self, i: usize) {
        self.clear_focus = if self.clear_last { 1 } else { i.min(1) };
        let (f, t) = (self.clear_focus, self.tick);
        let Some(c) = &mut self.clear_l else { return };
        for (k, part) in CLEAR_PARTS.iter().enumerate() {
            let a = format!("{part}/OnOff");
            if k == f {
                c.play(&a, t);
            } else {
                c.hold(&a, 0.0);
            }
        }
    }

    /// The buttons take input once In has ended (`cIn` waits for slot 0, 180 frames, then
    /// `cLoop` reads the buttons, 0x024dc88c / 0x024dcf18) [obs].
    pub fn clear_ready(&mut self) -> bool {
        let n = self.clear_l.as_mut().map_or(0, |c| c.ticks("In", 0));
        self.mode == RunOverlayMode::Clear
            && self.since_mode() >= n
            && self.clear_leave_at.is_none()
    }

    /// The focused button decided: its Decide, then the layout's Out (16 f, from the current
    /// frame) once the buttons are done (`cOut`, 0x024dded8 / 0x024ddf68) [obs]. The leave
    /// tick is in [`RunOverlay::clear_leave_at`].
    pub fn press_clear(&mut self) {
        let part = CLEAR_PARTS[self.clear_focus.min(1)];
        let t = self.tick;
        if let Some(c) = &mut self.clear_l {
            let decide = format!("{part}/Decide");
            c.play(&decide, t);
            key(c, Some(part), "Decide", &[]); // SE_PlayExit / SE_PlayNext
            let d = c.ticks(&decide, 0);
            c.play("Out", t + d);
            key(c, None, "Out", &[]);
            self.clear_leave_at = Some(t + d + c.ticks("Out", 0));
        }
    }

    pub fn skip_begin(&mut self) {
        if self.skip_l.is_none() {
            self.skip_l = self.ui.layout("Play_SlidePassIconKey_00");
        }
        let t = self.tick;
        let Some(s) = &mut self.skip_l else { return };
        self.closing = None;
        for a in [
            "Out",
            "Pass",
            "CountDown_00",
            "CountDown_01",
            "CountDown_02",
        ] {
            s.stop(a);
        }
        s.play_loop("Default", t);
        s.play("In", t);
        key(s, None, "In", &[]);
        // after In: CountDown_00 / _01 / _02 with the keys Countdown3 / 2 / 1 (SLink ids
        // 0x18 / 0x19 / 0x1a), each when the step before has ended [obs 0x025035c0,
        // 0x02503610]
        let i = s.ticks("In", 0);
        let step = s.ticks("CountDown_00", 1);
        let (ui, name) = (&self.ui, s.name.clone());
        self.sounds.drop_tag("skip");
        for (k, (anim, cue)) in [
            ("CountDown_00", "Countdown3"),
            ("CountDown_01", "Countdown2"),
            ("CountDown_02", "Countdown1"),
        ]
        .iter()
        .enumerate()
        {
            let at = t + i + step * k as i32;
            s.play(anim, at);
            let cues = ui.smm1_key(&name, None, cue, &[]);
            self.sounds.at(at, "skip", cues);
        }
        self.set_mode(RunOverlayMode::Skip);
    }

    pub fn skip_cancel(&mut self) {
        if self.mode != RunOverlayMode::Skip {
            return;
        }
        let t = self.tick;
        self.sounds.drop_tag("skip");
        let Some(s) = &mut self.skip_l else { return };
        s.play("Out", t);
        self.closing = Some((Which::Skip, t + s.ticks("Out", 1)));
        self.set_mode(RunOverlayMode::None);
    }

    /// The hold is complete: play Pass. Returns the tick count it takes.
    pub fn skip_pass(&mut self) -> i32 {
        let t = self.tick;
        let ui = &self.ui;
        // the course slides away: Play_SlidePass_00's SlidePass, upwards [inf]
        ui.push_cues(ui.smm1_key("Play_SlidePass_00", None, "SlidePass", &[Ctx::Enum("cUp")]));
        let Some(s) = &mut self.skip_l else { return 1 };
        s.play("Pass", t);
        s.ticks("Pass", 1)
    }

    /// The course's intro card the way `start` says (its Init, 0x024e3590), over the
    /// course: In (AnimA, key `CouseInWalk100`) and InAction (AnimB, `CouseInJump100` with the
    /// lives) as they apply, then — the course being loaded already — Out (150 f) or, after a
    /// death, OutMiss (60 f), with the layout's `Out` key [obs 0x024e3bd4 … 0x024e3e04,
    /// 0x0254a160]. The course starts when it has ended.
    pub fn show_card(&mut self, info: &RunCardInfo, start: CardStart) {
        if self.card_l.is_none() {
            self.card_l = self.ui.layout("Play_CourseIn100Mario_00");
        }
        let t = self.tick;
        let Some(c) = &mut self.card_l else { return };
        configure_card(c, info);
        for a in ["AnimA", "AnimB", "Out", "OutMiss"] {
            c.stop(a);
        }
        c.hold("Type", if info.number >= info.total { 1.0 } else { 0.0 });
        self.card_retry = start == CardStart::Retry;
        self.card_lives = info.lives;
        self.sounds.drop_tag("card");
        match start {
            CardStart::NewRun => {
                c.play("AnimA", t);
                key(c, None, "CouseInWalk100", &[Ctx::Int(info.lives as i32)]);
                self.card = CardPhase::AnimA(t);
            }
            CardStart::Resume | CardStart::Retry => {
                c.hold_end("AnimA");
                self.card_anim_b(t);
            }
            CardStart::Next => {
                c.hold_end("AnimB");
                self.card_out(t);
            }
        }
        self.set_mode(RunOverlayMode::Card);
    }

    fn card_anim_b(&mut self, t: i32) {
        let Some(c) = &mut self.card_l else { return };
        c.play("AnimB", t);
        key(
            c,
            None,
            "CouseInJump100",
            &[Ctx::Int(self.card_lives as i32)],
        );
        self.card = CardPhase::AnimB(t);
    }

    fn card_out(&mut self, t: i32) {
        let Some(c) = &mut self.card_l else { return };
        let a = if self.card_retry { "OutMiss" } else { "Out" };
        c.play(a, t);
        key(c, None, "Out", &[]);
        self.card = CardPhase::Out(t + c.ticks(a, 1));
    }

    fn card_step(&mut self) {
        let t = self.tick;
        let Some(c) = &mut self.card_l else { return };
        match self.card {
            CardPhase::AnimA(_) if c.finished("AnimA", t) => self.card_anim_b(t),
            CardPhase::AnimB(_) if c.finished("AnimB", t) => self.card_out(t),
            _ => {}
        }
    }

    /// The card has played out (or was cut with A).
    pub fn card_done(&mut self) -> bool {
        if self.mode != RunOverlayMode::Card {
            return true;
        }
        matches!(self.card, CardPhase::Out(end) if self.tick >= end)
    }

    /// A on the card (the course being loaded): the course-in controller goes straight to its
    /// Out state and the course starts (0x0254862c) [obs; the card cut at once inf].
    pub fn card_skip(&mut self) {
        if self.mode == RunOverlayMode::Card {
            self.sounds.drop_tag("card");
            self.card = CardPhase::Out(self.tick);
        }
    }

    pub fn hide_card(&mut self) {
        if self.mode == RunOverlayMode::Card {
            self.card = CardPhase::None;
            self.set_mode(RunOverlayMode::None);
        }
    }

    pub fn clear_all(&mut self) {
        self.closing = None;
        self.sounds.clear();
        self.card = CardPhase::None;
        self.clear_leave_at = None;
        self.set_mode(RunOverlayMode::None);
    }
}
