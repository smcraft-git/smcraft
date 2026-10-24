//! Super Worlds (Course World → Super Worlds; private spec `research/specs/super-worlds.md`):
//! - the planets screen `Nwk_MyWTitle_00`: ten planet buttons (`Nwk_MyWBtn_00`, TypeSkin = the
//!   planet) around the map info card (`Nwk_MyWMapInfo_00`: maker, worlds, courses, Play). The
//!   first planet is the test world assembled from the library (offline); the others are
//!   TGRCode's picked-up Super Worlds when Course World is on TGRCode (the `super_worlds` /
//!   `super_world` online calls; the host decrypts the map with the pack's world-map table);
//! - the title `MyWEdit_WorldPlayTitle_00` (New Game, Continue when a play is saved);
//! - the map: the 7 × 4 grid of the current world drawn flat (land, water, roads, the points,
//!   pipes and Toad Houses, Mario's icon) under the game's HUD `MyWEdit_PlayScene_00` (WORLD n,
//!   lives, coins) with the course-name balloon `MyWEdit_CourseBalloon_00` and the "Play" prompt
//!   `MyWEdit_PlayBalloonPos_00`. The game draws its maps in 3D (`MyWorld_Model.pack`); the flat
//!   map and its colours are ours [departure];
//! - a course: [`CoursePlay`] with the world's lives (the card's lives line, a death retries the
//!   course with one life less, the last one ends in the game over screen);
//! - the end: `MyW_Ending_00` after the last world's goal.
//!
//! Walking: a direction walks Mario along the road to the next point, fork or dead end; an
//! uncleared course blocks the way on; confirm plays the course under him, uses a pipe (to the
//! other pipe) or a Toad House (lives). Progress is saved after every change through
//! [`crate::Host::store_value`] under `superworld.<id>`.

use mmk_modes::superworld::{
    self as sw, ClearResult, EAST, NORTH, Progress, SOUTH, Spot, SuperWorld, SwCourse, WEST,
};
use mmk_ui::Quad;
use serde_json::json;

use crate::fade::FadeKind;
use crate::host::{CourseEntry, Source};
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, MenuCursor, Script, UiInput, hit};
use crate::play::{CoursePlay, PlayEvent, PlayInput, RunLink};
use crate::screens::{CardInfo, CourseScreens};
use crate::smm1::RunCardInfo;
use crate::sound::Event;
use crate::{Cx, DrawItem, Go, push_layout};

const PLANETS: &str = "Nwk_MyWTitle_00";
const TITLE: &str = "MyWEdit_WorldPlayTitle_00";
const HUD: &str = "MyWEdit_PlayScene_00";
const ENDING: &str = "MyW_Ending_00";
/// Planet buttons on the screen.
const PLANET_SLOTS: usize = 10;
/// Ticks Mario takes for one cell of road.
const WALK_TICKS: i32 = 10;

/// A planet to pick: the library's test world or a Super World on TGRCode.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    /// [`sw::LOCAL_ID`] or the TGRCode Super World ID.
    pub id: String,
    pub maker: Option<String>,
    pub worlds: usize,
    pub courses: usize,
    pub planet: u8,
}

/// What a step tells the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuperWorldEvent {
    /// Back to Course World.
    Exit,
}

/// Deferred steps (run when their fade has covered the screen).
#[derive(Clone, Debug, PartialEq)]
pub enum SuperWorldGo {
    Planets,
    Title,
    Map,
    Play,
    Ending,
}

// MARK: drawing helpers

fn solid(center: [f32; 2], size: [f32; 2], color: [u8; 4]) -> Quad {
    let (hx, hy) = (size[0] / 2.0, size[1] / 2.0);
    let [x, y] = center;
    Quad {
        pos: [
            [x - hx, y + hy],
            [x + hx, y + hy],
            [x - hx, y - hy],
            [x + hx, y - hy],
        ],
        color: [color; 4],
        black: [0, 0, 0, 0],
        white: [255; 4],
        tex0: mmk_ui::TEXTURE_NONE,
        tex1: mmk_ui::TEXTURE_NONE,
        flags: mmk_ui::BLEND_NORMAL
            | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_COLOR)
            | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_ALPHA),
        ..Default::default()
    }
}

/// A diamond (a square turned 45°).
fn diamond(center: [f32; 2], r: f32, color: [u8; 4]) -> Quad {
    let mut q = solid(center, [0.0, 0.0], color);
    let [x, y] = center;
    q.pos = [[x, y + r], [x + r, y], [x - r, y], [x, y - r]];
    q
}

/// A round-ish dot: a square and a diamond.
fn dot(out: &mut Vec<Quad>, center: [f32; 2], r: f32, color: [u8; 4]) {
    out.push(solid(center, [r * 1.6, r * 1.6], color));
    out.push(diamond(center, r * 1.1, color));
}

fn textured(center: [f32; 2], size: [f32; 2], tex: u32) -> Quad {
    let mut q = solid(center, size, [255; 4]);
    q.tex0 = tex;
    q.uv0 = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    q.flags = mmk_ui::BLEND_NORMAL | mmk_ui::FLAG_LINEAR;
    q
}

/// Per theme (in [`sw::THEMES`] order): land, the empty area (water or ground, as the theme's
/// `NothingAreaAttr`), road, backdrop [departure: our colours].
type Rgba = [u8; 4];

const PALETTE: [(Rgba, Rgba, Rgba, Rgba); 8] = [
    (
        [96, 184, 72, 255],
        [64, 136, 216, 255],
        [232, 208, 144, 255],
        [40, 96, 176, 255],
    ),
    (
        [120, 96, 80, 255],
        [64, 48, 40, 255],
        [216, 184, 136, 255],
        [32, 24, 24, 255],
    ),
    (
        [232, 200, 112, 255],
        [208, 160, 80, 255],
        [248, 232, 184, 255],
        [176, 120, 56, 255],
    ),
    (
        [232, 240, 248, 255],
        [88, 152, 216, 255],
        [176, 200, 224, 255],
        [48, 104, 168, 255],
    ),
    (
        [248, 248, 232, 255],
        [128, 192, 248, 255],
        [248, 216, 120, 255],
        [96, 168, 240, 255],
    ),
    (
        [48, 128, 64, 255],
        [24, 80, 40, 255],
        [200, 176, 112, 255],
        [16, 56, 32, 255],
    ),
    (
        [96, 64, 64, 255],
        [216, 72, 24, 255],
        [168, 128, 104, 255],
        [80, 24, 16, 255],
    ),
    (
        [72, 64, 128, 255],
        [24, 24, 64, 255],
        [160, 152, 216, 255],
        [8, 8, 32, 255],
    ),
];

/// Cell size of the drawn map (layout units) and the grid's centre.
const CELL: f32 = 128.0;
const GRID_CENTER: [f32; 2] = [0.0, -40.0];

fn cell_center(x: f32, y: f32) -> [f32; 2] {
    [
        GRID_CENTER[0] + (x - 3.0) * CELL,
        GRID_CENTER[1] - (y - 1.5) * CELL,
    ]
}

// MARK: the run (lives, clears) as a course sees it

struct SwRun {
    world: SuperWorld,
    progress: Progress,
    /// The course being played: world, order index.
    course: Option<(usize, usize)>,
    /// Set by a clear; applied when the course screen closes.
    cleared: Option<usize>,
    mode_line: String,
}

impl SwRun {
    fn persist(&self, cx: &mut Cx) {
        cx.host
            .store_value(&self.world.save_key(), &self.progress.to_json());
    }

    fn current(&self) -> Option<&SwCourse> {
        let (w, k) = self.course?;
        self.world.course(w, k)
    }
}

impl RunLink for SwRun {
    fn died(&mut self, cx: &mut Cx) -> bool {
        let alive = self.progress.died();
        self.persist(cx);
        if alive {
            self.apply_counters(cx);
        }
        alive
    }

    fn cleared(&mut self, score: u32, coins: u32) {
        self.progress.score = score;
        self.progress.coins = coins % 100;
        self.cleared = self.course.map(|c| c.1);
    }

    fn skipped(&mut self) {}

    fn save_and_quit(&mut self) {}

    fn save_and_quit_after_clear(&mut self) {}

    fn extra_lives(&mut self, n: u32) {
        self.progress.add_lives(n);
    }

    fn apply_counters(&mut self, cx: &mut Cx) {
        let p = &self.progress;
        cx.host.set_run_counters(p.score, p.coins, p.lives);
    }

    fn can_skip(&self) -> bool {
        false
    }

    fn pause_line(&self) -> String {
        self.mode_line.clone()
    }

    fn course_name(&self) -> String {
        self.current().map(|c| c.name.clone()).unwrap_or_default()
    }

    fn course_maker(&self) -> String {
        self.current()
            .and_then(|c| c.maker.clone())
            .unwrap_or_default()
    }

    fn clear_ends_run(&self) -> bool {
        false
    }

    fn world_record_ms(&self) -> Option<i64> {
        None
    }

    fn card_info(&self) -> RunCardInfo {
        RunCardInfo {
            name: self.course_name(),
            maker: self.course_maker(),
            lives: self.progress.lives,
            ..Default::default()
        }
    }
}

// MARK: planets

struct PlanetScene {
    ui: Ui,
    /// Course World's hub behind, turned to its Super World backdrop (InMyWTitle) [obs].
    bg: Option<Layout>,
    layout: Layout,
    cursor: MenuCursor,
    tick: i32,
    selection: usize,
    picks: Vec<Pick>,
    /// The pick to play at a tick, and whether together.
    pending: Option<(usize, i32, bool)>,
    /// The card's button with the cursor: Play (false) or Play Together (true).
    together_btn: bool,
}

enum PlanetEvent {
    /// Play the pick (`true`: Play Together).
    Play(Pick, bool),
    Back,
}

fn planet_bound(i: usize) -> String {
    format!("L_MyWBtn_{i:02}/B_Btn_00")
}

impl PlanetScene {
    fn new(ui: &Ui, picks: Vec<Pick>, selection: usize) -> Option<Self> {
        let mut l = ui.layout(PLANETS)?;
        l.play("In", 0);
        l.play("InContents", 0);
        // no shuffle (one page of picks) [departure]; Play Together (click it, or ZR)
        l.show("L_ShuffleBtn_00", false);
        l.show("L_MyWMapInfo_00/L_MiiBtn_00", false);
        l.show("L_MyWMapInfo_00/L_NationalFlag_00", false);
        l.show("L_MyWMapInfo_00/N_AlignGoodCount_00", false);
        for i in 0..PLANET_SLOTS {
            l.play_loop(&format!("L_MyWBtn_{i:02}/LoopRandom"), 0);
        }
        let mut bg = ui.layout("Nwk_Title_00");
        if let Some(b) = &mut bg {
            b.hold("RegionLanguageType", 1.0);
            b.play_loop("LoopRandom", 0);
            b.play("InMyWTitle", 0);
            b.show("N_Btn_00", false);
        }
        let mut s = Self {
            ui: ui.clone(),
            bg,
            layout: l,
            cursor: MenuCursor::new(ui),
            tick: 0,
            selection: 0,
            picks: Vec::new(),
            pending: None,
            together_btn: false,
        };
        s.set_picks(picks);
        s.select(selection.min(s.picks.len().saturating_sub(1)));
        Some(s)
    }

    fn set_picks(&mut self, picks: Vec<Pick>) {
        self.picks = picks;
        self.picks.truncate(PLANET_SLOTS);
        for i in 0..PLANET_SLOTS {
            let part = format!("L_MyWBtn_{i:02}");
            match self.picks.get(i) {
                Some(p) => {
                    self.layout.show(&part, true);
                    self.layout
                        .hold(&format!("{part}/TypeSkin"), p.planet.min(7) as f32);
                    self.layout
                        .hold(&format!("{part}/TypeSize"), if i == 0 { 1.0 } else { 0.0 });
                    self.layout.hold(&format!("{part}/TypeClear"), 0.0);
                    self.layout.show(&format!("{part}/N_Peach_00"), false);
                    self.layout.show(&format!("{part}/L_MyWProgress_00"), false);
                }
                None => self.layout.show(&part, false),
            }
        }
        self.select(self.selection.min(self.picks.len().saturating_sub(1)));
    }

    fn select(&mut self, i: usize) {
        self.selection = i;
        for k in 0..PLANET_SLOTS {
            let part = format!("L_MyWBtn_{k:02}");
            if k == i {
                self.layout.play(&format!("{part}/Check"), self.tick);
                self.layout
                    .play_loop(&format!("{part}/CheckLoop"), self.tick);
            } else {
                self.layout.hold_end(&format!("{part}/UnCheck"));
                self.layout.stop(&format!("{part}/CheckLoop"));
            }
        }
        let info = "L_MyWMapInfo_00";
        let Some(p) = self.picks.get(i).cloned() else {
            return;
        };
        let name = p.maker.clone().unwrap_or_else(|| self.library_name());
        let pane = format!("{info}/T_PlayerName_00");
        self.layout.text(&pane, &name);
        // "SUPER <name> WORLD": the two title pictures either side of the name [inf]
        if let Some(w) = self.layout.text_width(&pane) {
            let w = w.min(360.0);
            // (the text sits left-aligned in its 360-wide box: centre it)
            self.layout.set_translate(&pane, [180.0 - w / 2.0, 0.0]);
            self.layout
                .set_translate(&format!("{info}/P_Title_00"), [-w / 2.0 - 75.0, -3.0]);
            self.layout
                .set_translate(&format!("{info}/P_Title_01"), [w / 2.0 + 79.0, -3.0]);
        }
        self.layout.text(
            &format!("{info}/L_WorldCount_00/T_Count_00"),
            &p.worlds.to_string(),
        );
        self.layout.text(
            &format!("{info}/L_WCourseCount_00/T_Count_00"),
            &p.courses.to_string(),
        );
    }

    fn library_name(&self) -> String {
        // the test world is made of the player's own courses: Coursebot's "My Courses" [inf]
        self.ui
            .message("Coursebot", "MyCourses")
            .unwrap_or_else(|| "My Courses".into())
    }

    fn step(&mut self, input: &UiInput, script: &mut Script) -> Option<PlanetEvent> {
        self.tick += 1;
        let mut pressed = input.pressed | script.step();
        if let Some((i, at, together)) = self.pending {
            if self.tick >= at {
                self.pending = None;
                return self
                    .picks
                    .get(i)
                    .cloned()
                    .map(|p| PlanetEvent::Play(p, together));
            }
            return None;
        }
        let mut together = pressed.has(Buttons::ZR);
        if let Some(p) = input.click {
            let mut panes: Vec<String> = (0..self.picks.len()).map(planet_bound).collect();
            panes.push("L_MyWMapInfo_00/L_PlayBtn_00/B_Btn_00".into());
            panes.push("L_MyWMapInfo_00/L_CoopModeBtn_00/B_Btn_00".into());
            match hit(&mut self.layout, self.tick, &panes, p) {
                Some(i) if i < self.picks.len() => {
                    if i == self.selection {
                        pressed = pressed | Buttons::CONFIRM;
                    } else {
                        self.ui.sound_button(PLANETS, "L_MyWBtn_00", Event::Select);
                        self.select(i);
                    }
                }
                Some(i) if i > self.picks.len() => together = true,
                Some(_) => pressed = pressed | Buttons::CONFIRM,
                None => {}
            }
        }
        if together && !self.picks.is_empty() {
            let b = "L_MyWMapInfo_00/L_CoopModeBtn_00";
            self.ui.sound_button(PLANETS, b, Event::Decide);
            let a = format!("{b}/Decide");
            self.layout.play(&a, self.tick);
            self.pending = Some((self.selection, self.tick + self.layout.ticks(&a, 1), true));
            return None;
        }
        // L / R (the card's arrows) change worlds; left / right move between Play Together and
        // Play
        let shoulder = if pressed.has(Buttons::L) {
            Some(-1)
        } else if pressed.has(Buttons::R) {
            Some(1)
        } else {
            None
        };
        if let Some(d) = shoulder
            && self.picks.len() > 1
        {
            let n = self.picks.len() as i32;
            self.ui.sound_button(PLANETS, "L_MyWBtn_00", Event::Select);
            self.select(((self.selection as i32 + d).rem_euclid(n)) as usize);
        } else if let Some(dir) = pressed.direction()
            && dir[0] != 0.0
        {
            let to = dir[0] < 0.0;
            if to != self.together_btn {
                self.together_btn = to;
                let b = if to {
                    "L_MyWMapInfo_00/L_CoopModeBtn_00"
                } else {
                    "L_MyWMapInfo_00/L_PlayBtn_00"
                };
                self.ui.sound_button(PLANETS, b, Event::Select);
            }
        } else if pressed.has(Buttons::CONFIRM) && self.together_btn && !self.picks.is_empty() {
            together = true;
            let b = "L_MyWMapInfo_00/L_CoopModeBtn_00";
            self.ui.sound_button(PLANETS, b, Event::Decide);
            let a = format!("{b}/Decide");
            self.layout.play(&a, self.tick);
            self.pending = Some((
                self.selection,
                self.tick + self.layout.ticks(&a, 1),
                together,
            ));
        } else if pressed.has(Buttons::CONFIRM) && !self.picks.is_empty() {
            let b = "L_MyWMapInfo_00/L_PlayBtn_00";
            self.ui.sound_button(PLANETS, b, Event::Decide);
            let a = format!("{b}/Decide");
            self.layout.play(&a, self.tick);
            self.pending = Some((self.selection, self.tick + self.layout.ticks(&a, 1), false));
        } else if pressed.has(Buttons::BACK) {
            self.ui.sound_back(PLANETS, "normal");
            return Some(PlanetEvent::Back);
        }
        let alpha = if self.pending.is_none() { 1.0 } else { 0.0 };
        let focus = if self.together_btn {
            "L_MyWMapInfo_00/L_CoopModeBtn_00/B_Btn_00"
        } else {
            "L_MyWMapInfo_00/L_PlayBtn_00/B_Btn_00"
        };
        self.cursor.place(&mut self.layout, focus, self.tick, alpha);
        None
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if let Some(b) = &mut self.bg {
            push_layout(out, b, self.tick);
        }
        push_layout(out, &mut self.layout, self.tick);
        self.cursor.draw(self.tick, out);
    }
}

// MARK: title

struct TitleScene {
    ui: Ui,
    layout: Layout,
    cursor: MenuCursor,
    tick: i32,
    can_continue: bool,
    selection: usize,
    pending: Option<(bool, i32)>,
}

const TITLE_BUTTONS: [&str; 2] = ["L_StartBtn_00", "L_StartBtn_01"];

impl TitleScene {
    fn new(ui: &Ui, maker: Option<&str>, can_continue: bool) -> Option<Self> {
        let mut l = ui.layout(TITLE)?;
        l.play("In", 0);
        // no Mii; the maker's name one letter to a plate (two rows of ten) [inf]
        l.hold("TypeMii", 0.0);
        l.show("N_MiiIcon_00", false);
        let name: Vec<char> = maker.unwrap_or_default().chars().take(20).collect();
        for i in 0..20 {
            let part = format!("L_PlayerName_{i:02}");
            match name.get(i) {
                Some(c) => l.text(&format!("{part}/T_PlayerName_00"), &c.to_string()),
                None => l.show(&part, false),
            }
        }
        l.show("N_PlayerName_01", name.len() > 10);
        l.show("N_PlayerName_00", !name.is_empty());
        l.show("L_StartBtn_01", can_continue);
        l.show("L_MyWProgress_00", false);
        // (the copyright line is for Nintendo's own worlds)
        l.show("P_Copyright_00", false);
        Some(Self {
            ui: ui.clone(),
            layout: l,
            cursor: MenuCursor::new(ui),
            tick: 0,
            can_continue,
            selection: if can_continue { 1 } else { 0 },
            pending: None,
        })
    }

    fn bound(i: usize) -> String {
        format!("{}/B_Btn_00", TITLE_BUTTONS[i])
    }

    /// Some(true): Continue, Some(false): New Game; Err(()) back.
    fn step(&mut self, input: &UiInput, script: &mut Script) -> Option<Result<bool, ()>> {
        self.tick += 1;
        let mut pressed = input.pressed | script.step();
        if let Some((resume, at)) = self.pending {
            if self.tick >= at {
                return Some(Ok(resume));
            }
            return None;
        }
        let n = if self.can_continue { 2 } else { 1 };
        if let Some(p) = input.click {
            let panes: Vec<String> = (0..n).map(Self::bound).collect();
            if let Some(i) = hit(&mut self.layout, self.tick, &panes, p) {
                self.selection = i;
                pressed = pressed | Buttons::CONFIRM;
            }
        }
        if pressed.any(Buttons::UP | Buttons::DOWN) && n > 1 {
            self.selection = 1 - self.selection;
            self.ui
                .sound_button(TITLE, TITLE_BUTTONS[self.selection], Event::Select);
        } else if pressed.has(Buttons::CONFIRM) {
            let b = TITLE_BUTTONS[self.selection];
            self.ui
                .sound_button(TITLE, TITLE_BUTTONS[self.selection], Event::Decide);
            let a = format!("{b}/Decide");
            self.layout.play(&a, self.tick);
            self.pending = Some((self.selection == 1, self.tick + self.layout.ticks(&a, 1)));
        } else if pressed.has(Buttons::BACK) {
            self.ui.sound_back(TITLE, "normal");
            return Some(Err(()));
        }
        let alpha = if self.pending.is_none() { 1.0 } else { 0.0 };
        self.cursor.place(
            &mut self.layout,
            &Self::bound(self.selection),
            self.tick,
            alpha,
        );
        None
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>) {
        push_layout(out, &mut self.layout, self.tick);
        self.cursor.draw(self.tick, out);
    }
}

// MARK: map

struct MapScene {
    ui: Ui,
    hud: Option<Layout>,
    name_balloon: Option<Layout>,
    prompt: Option<Layout>,
    tick: i32,
    /// The cells left to walk and the tick the current one started.
    walk: Vec<(i32, i32)>,
    walk_from: (f32, f32),
    walk_start: i32,
    mario_tex: Option<u32>,
}

enum MapEvent {
    Play,
    Pipe,
    ToadHouse(usize),
    Quit,
}

impl MapScene {
    fn new(ui: &Ui) -> Self {
        let mut hud = ui.layout(HUD);
        if let Some(h) = &mut hud {
            h.play("In", 0);
            h.hold("Type", 0.0);
            h.hold("TypeMember", 0.0);
            h.hold("TypeCharacter", 0.0);
            h.show("N_WorldChange_00", false);
            h.show("L_GhostBtn_00", false);
        }
        let mut name_balloon = ui.layout("MyWEdit_CourseBalloon_00");
        if let Some(b) = &mut name_balloon {
            b.hold_end("In");
        }
        let mut prompt = ui.layout("MyWEdit_PlayBalloonPos_00");
        if let Some(b) = &mut prompt {
            b.hold_end("In");
            b.hold("L_Balloon_00/Type", 0.0);
            b.show("L_Balloon_00/N_DirectionlClear_00", false);
        }
        let mario_tex = ui
            .cache_mut()
            .texture_file(HUD, "CharacterIconS_MW_P1^u")
            .map(|t| t.id);
        Self {
            ui: ui.clone(),
            hud,
            name_balloon,
            prompt,
            tick: 0,
            walk: Vec::new(),
            walk_from: (0.0, 0.0),
            walk_start: 0,
            mario_tex,
        }
    }

    fn walking(&self) -> bool {
        !self.walk.is_empty()
    }

    /// Mario's drawn position (cells).
    fn mario_at(&self, p: &Progress) -> (f32, f32) {
        let Some(&(nx, ny)) = self.walk.first() else {
            return (p.x as f32, p.y as f32);
        };
        let t = ((self.tick - self.walk_start) as f32 / WALK_TICKS as f32).clamp(0.0, 1.0);
        let (fx, fy) = self.walk_from;
        (fx + (nx as f32 - fx) * t, fy + (ny as f32 - fy) * t)
    }

    fn step(
        &mut self,
        input: &UiInput,
        script: &mut Script,
        world: &SuperWorld,
        p: &mut Progress,
    ) -> Option<MapEvent> {
        self.tick += 1;
        let pressed = input.pressed | script.step();
        let Some(w) = p.world_map(world) else {
            return Some(MapEvent::Quit);
        };
        if let Some(&(nx, ny)) = self.walk.first() {
            if self.tick - self.walk_start >= WALK_TICKS {
                self.walk_from = (nx as f32, ny as f32);
                (p.x, p.y) = (nx as u8, ny as u8);
                self.walk.remove(0);
                self.walk_start = self.tick;
            }
            return None;
        }
        let dir = if pressed.has(Buttons::RIGHT) {
            Some(EAST)
        } else if pressed.has(Buttons::LEFT) {
            Some(WEST)
        } else if pressed.has(Buttons::UP) {
            Some(NORTH)
        } else if pressed.has(Buttons::DOWN) {
            Some(SOUTH)
        } else {
            None
        };
        if let Some(d) = dir {
            let cleared = p.clears.get(p.world).copied().unwrap_or(0);
            let route = sw::route(w, cleared, p.x as i32, p.y as i32, d);
            if !route.is_empty() {
                self.walk = route;
                self.walk_from = (p.x as f32, p.y as f32);
                self.walk_start = self.tick;
            }
            return None;
        }
        if pressed.has(Buttons::CONFIRM) {
            return match sw::spot(w, p.x as i32, p.y as i32) {
                Spot::Course { .. } => {
                    self.ui.sound_button(HUD, "L_EndBtn_00", Event::Decide);
                    Some(MapEvent::Play)
                }
                Spot::Pipe(_) => Some(MapEvent::Pipe),
                Spot::ToadHouse(k) => Some(MapEvent::ToadHouse(k)),
                _ => None,
            };
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(HUD, "normal");
            return Some(MapEvent::Quit);
        }
        None
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>, world: &SuperWorld, p: &Progress) {
        let Some(w) = p.world_map(world) else {
            return;
        };
        let theme = (w.theme as usize).min(PALETTE.len() - 1);
        let (land, empty, road, sky) = PALETTE[theme];
        let mut q = vec![solid([0.0, 0.0], [1280.0, 720.0], sky)];
        let cleared = p.clears.get(p.world).copied().unwrap_or(0);
        let reach = sw::reachable(w, cleared);
        // the grid: land under roads and points, the theme's empty area elsewhere
        for y in 0..mmk_formats::smm2::world::GRID_H as i32 {
            for x in 0..mmk_formats::smm2::world::GRID_W as i32 {
                let c = w.cell(x, y).copied().unwrap_or_default();
                let used = c.kind != 0 || w.occupied(x, y);
                let col = if used { land } else { empty };
                q.push(solid(
                    cell_center(x as f32, y as f32),
                    [CELL + 1.0, CELL + 1.0],
                    col,
                ));
            }
        }
        // roads: a bar from each cell's centre to every side it links to (paler when out of reach)
        for y in 0..4 {
            for x in 0..7 {
                let links = w.links(x, y);
                if links == 0 {
                    continue;
                }
                let lit = reach & (1 << (y * 7 + x)) != 0;
                let col = if lit {
                    road
                } else {
                    [
                        road[0] / 2 + land[0] / 2,
                        road[1] / 2 + land[1] / 2,
                        road[2] / 2 + land[2] / 2,
                        255,
                    ]
                };
                let c = cell_center(x as f32, y as f32);
                q.push(solid(c, [30.0, 30.0], col));
                for (d, dx, dy) in [
                    (EAST, 1.0, 0.0),
                    (WEST, -1.0, 0.0),
                    (SOUTH, 0.0, -1.0),
                    (NORTH, 0.0, 1.0),
                ] {
                    if links & d != 0 {
                        let half = CELL / 4.0;
                        let center = [c[0] + dx * half, c[1] + dy * half];
                        let size = if dx != 0.0 {
                            [CELL / 2.0, 30.0]
                        } else {
                            [30.0, CELL / 2.0]
                        };
                        q.push(solid(center, size, col));
                    }
                }
            }
        }
        // points, pipes, Toad Houses
        for y in 0..4 {
            for x in 0..7 {
                let c = cell_center(x as f32, y as f32);
                match sw::spot(w, x, y) {
                    Spot::Start => {
                        q.push(solid(c, [52.0, 40.0], [40, 40, 40, 255]));
                        q.push(solid(c, [44.0, 32.0], [248, 248, 248, 255]));
                    }
                    Spot::Course { order, goal } => {
                        let done = cleared & (1 << order) != 0;
                        if goal {
                            let stone = if done {
                                [168, 168, 184, 255]
                            } else {
                                [120, 120, 136, 255]
                            };
                            q.push(solid([c[0], c[1] + 4.0], [64.0, 56.0], [40, 40, 48, 255]));
                            q.push(solid([c[0], c[1] + 4.0], [56.0, 48.0], stone));
                            for k in [-1.0, 0.0, 1.0] {
                                q.push(solid([c[0] + k * 20.0, c[1] + 36.0], [12.0, 12.0], stone));
                            }
                            q.push(solid([c[0], c[1] - 6.0], [16.0, 24.0], [40, 40, 48, 255]));
                            if done {
                                q.push(solid(
                                    [c[0] + 18.0, c[1] + 54.0],
                                    [20.0, 12.0],
                                    [232, 48, 40, 255],
                                ));
                            }
                        } else {
                            dot(&mut q, c, 22.0, [40, 32, 24, 255]);
                            let col = if done {
                                [56, 112, 232, 255]
                            } else {
                                [248, 200, 40, 255]
                            };
                            dot(&mut q, c, 18.0, col);
                        }
                    }
                    Spot::Pipe(_) => {
                        q.push(solid([c[0], c[1] + 14.0], [58.0, 18.0], [24, 96, 32, 255]));
                        q.push(solid([c[0], c[1] - 6.0], [46.0, 30.0], [48, 168, 64, 255]));
                        q.push(solid([c[0], c[1] + 14.0], [52.0, 12.0], [72, 200, 88, 255]));
                    }
                    Spot::ToadHouse(k) => {
                        let used = p.toads.get(p.world).is_some_and(|t| t & (1 << k) != 0);
                        q.push(solid(
                            [c[0], c[1] - 10.0],
                            [44.0, 28.0],
                            [248, 240, 224, 255],
                        ));
                        let roof = if used {
                            [160, 120, 120, 255]
                        } else {
                            [232, 48, 40, 255]
                        };
                        let mut tri = solid([c[0], c[1] + 14.0], [0.0, 0.0], roof);
                        tri.pos = [
                            [c[0] - 2.0, c[1] + 40.0],
                            [c[0] + 2.0, c[1] + 40.0],
                            [c[0] - 34.0, c[1] + 4.0],
                            [c[0] + 34.0, c[1] + 4.0],
                        ];
                        q.push(tri);
                        q.push(solid([c[0], c[1] - 16.0], [12.0, 16.0], [96, 64, 32, 255]));
                    }
                    Spot::Road => {}
                }
            }
        }
        // Mario
        let (mx, my) = self.mario_at(p);
        let at = cell_center(mx, my);
        let bob = if self.walking() {
            ((self.tick / 4) % 2) as f32 * 4.0
        } else {
            0.0
        };
        let pos = [at[0], at[1] + 30.0 + bob];
        match self.mario_tex {
            Some(t) => q.push(textured(pos, [76.0, 76.0], t)),
            None => q.push(solid(pos, [40.0, 52.0], [232, 40, 32, 255])),
        }
        out.push(DrawItem::Layout {
            ui: self.ui.kind(),
            size: [1280.0, 720.0],
            quads: q,
        });
        // the course's name above it, and the prompt, when Mario stands on a course
        let here = (!self.walking()).then(|| sw::spot(w, p.x as i32, p.y as i32));
        let course = match here {
            Some(Spot::Course { order, .. }) => world.course(p.world, order),
            _ => None,
        };
        let c = cell_center(p.x as f32, p.y as f32);
        if let (Some(course), Some(b)) = (course, &mut self.name_balloon) {
            // above the point; below it on the top row (under the HUD) [inf]
            let below = p.y == 0;
            b.hold("Direction", if below { 1.0 } else { 0.0 });
            let dy = if below { -150.0 } else { 70.0 };
            b.set_translate("N_Pos_00", [c[0], c[1] + dy]);
            b.text("T_Title_00", &course.name);
            push_layout(out, b, self.tick);
        }
        if matches!(
            here,
            Some(Spot::Course { .. } | Spot::Pipe(_) | Spot::ToadHouse(_))
        ) && let Some(b) = &mut self.prompt
        {
            let label = if matches!(here, Some(Spot::Course { .. })) {
                "Challenge_00"
            } else {
                "Enter_00"
            };
            if let Some(t) = self.ui.message("MyWEdit_PlayBalloonPos_00", label) {
                b.text("L_Balloon_00/T_Challenge_00", &t);
            }
            b.set_translate("N_Pos_00", [c[0] + 110.0, c[1] - 40.0]);
            push_layout(out, b, self.tick);
        }
        if let Some(h) = &mut self.hud {
            let world_no = self
                .ui
                .format(HUD, "WorldNo", &[(p.world + 1).to_string()])
                .unwrap_or_else(|| format!("World {}", p.world + 1));
            for t in ["T_WorldNum_00", "T_WorldNumSh_00"] {
                h.text(t, &world_no);
            }
            for t in ["T_MW_Zanki_00", "T_MW_ZankiSh_00"] {
                h.text(t, &p.lives.to_string());
            }
            for t in ["T_MW_Coin_00", "T_MW_CoinSh_00"] {
                h.text(t, &p.coins.to_string());
            }
            push_layout(out, h, self.tick);
        }
    }
}

// MARK: ending

struct EndingScene {
    layout: Layout,
    tick: i32,
    length: i32,
}

impl EndingScene {
    fn new(ui: &Ui, character: usize) -> Option<Self> {
        let mut l = ui.layout(ENDING)?;
        l.hold("TypeMember", 0.0);
        l.hold("TypeCharacter", character.min(2) as f32);
        l.play("In", 0);
        let length = l.ticks("In", 1);
        Some(Self {
            layout: l,
            tick: 0,
            length,
        })
    }

    fn step(&mut self, input: &UiInput) -> bool {
        self.tick += 1;
        self.tick >= self.length
            || (self.tick > 120 && input.pressed.any(Buttons::CONFIRM | Buttons::BACK))
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>) {
        push_layout(out, &mut self.layout, self.tick);
    }
}

// MARK: the flow

enum Stage {
    Planets(Box<PlanetScene>),
    /// Waiting for a Super World from the server (the planets stay up).
    Loading(Box<PlanetScene>, u32),
    Title(Box<TitleScene>),
    Map(Box<MapScene>),
    Playing(Box<CoursePlay>),
    Ending(Box<EndingScene>),
    /// No layouts to show (an import without them): the next step leaves.
    None,
}

/// The Super World screens from Course World's hub; owns the world being played.
pub struct SuperWorldFlow {
    ui: Ui,
    stage: Stage,
    library: Vec<CourseEntry>,
    picks: Vec<Pick>,
    /// The picks call to the server (TGRCode's picked-up worlds).
    picks_call: Option<u32>,
    run: Option<SwRun>,
    /// Players of this run (Play Together: 2–4).
    players: u32,
    leaving: bool,
    /// Captures: menu presses at global UI ticks.
    pub script: Script,
}

/// The pause menu's mode line: "Super World - World n".
fn mode_line(ui: &Ui, w: usize) -> String {
    let title = ui
        .message("MyWorld", "WorldMap")
        .unwrap_or_else(|| "Super World".into());
    let world = ui
        .message("MyWorld", "World")
        .unwrap_or_else(|| "World".into());
    format!("{title} - {world} {}", w + 1)
}

/// The library's playable courses for the test world (the player's own first, else the
/// built-in ones).
pub fn library_courses(library: &[CourseEntry]) -> Vec<SwCourse> {
    let to = |e: &CourseEntry| SwCourse {
        id: e.id.clone(),
        online: false,
        name: e.name.clone(),
        maker: e.meta.maker.clone(),
        style: e.style.clone(),
    };
    let own: Vec<SwCourse> = library
        .iter()
        .filter(|e| e.playable && !e.built_in)
        .map(to)
        .collect();
    if !own.is_empty() {
        return own;
    }
    library.iter().filter(|e| e.playable).map(to).collect()
}

impl SuperWorldFlow {
    pub fn new(ui: &Ui, library: Vec<CourseEntry>, cx: &mut Cx) -> Self {
        let mut f = Self {
            ui: ui.clone(),
            stage: Stage::None,
            library,
            picks: Vec::new(),
            picks_call: None,
            run: None,
            players: 1,
            leaving: false,
            script: Script::default(),
        };
        f.picks = f.local_pick().into_iter().collect();
        f.picks_call = cx.online.call(cx.host, "super_worlds", json!({}));
        f.show_planets(0);
        f
    }

    fn local_world(&self) -> Option<SuperWorld> {
        SuperWorld::test_world(&library_courses(&self.library))
    }

    fn local_pick(&self) -> Option<Pick> {
        let w = self.local_world()?;
        Some(Pick {
            id: w.id.clone(),
            maker: None,
            worlds: w.world_count(),
            courses: w.course_count(),
            planet: 0,
        })
    }

    fn show_planets(&mut self, selection: usize) {
        if let Some(p) = PlanetScene::new(&self.ui, self.picks.clone(), selection) {
            self.stage = Stage::Planets(Box::new(p));
        }
    }

    /// A Super World to play is in: its title (New Game / Continue).
    fn open_world(&mut self, cx: &mut Cx, world: SuperWorld) {
        let saved = cx
            .host
            .load_value(&world.save_key())
            .and_then(|s| Progress::from_json(&s, &world))
            .filter(|p| !p.finished);
        let can_continue = saved.is_some();
        let progress = saved.unwrap_or_else(|| Progress::new(&world));
        let maker = world.maker.clone();
        self.run = Some(SwRun {
            mode_line: mode_line(&self.ui, progress.world),
            world,
            progress,
            course: None,
            cleared: None,
        });
        if let Some(t) = TitleScene::new(&self.ui, maker.as_deref(), can_continue) {
            self.stage = Stage::Title(Box::new(t));
        }
    }

    fn pick(&mut self, cx: &mut Cx, p: Pick, scene: Box<PlanetScene>) {
        if p.id == sw::LOCAL_ID {
            match self.local_world() {
                Some(w) => self.open_world(cx, w),
                None => {
                    cx.toast("There are no courses in your library to build a Super World from.");
                    self.stage = Stage::Planets(scene);
                }
            }
            return;
        }
        match cx
            .online
            .call(cx.host, "super_world", json!({ "id": p.id }))
        {
            Some(id) => self.stage = Stage::Loading(scene, id),
            None => {
                cx.toast("Super Worlds from Course World need TGRCode (Course World's source).");
                self.stage = Stage::Planets(scene);
            }
        }
    }

    pub fn go(&mut self, cx: &mut Cx, g: SuperWorldGo) {
        if matches!(self.stage, Stage::Playing(_)) {
            cx.host.unload_course();
        }
        self.leaving = false;
        match g {
            SuperWorldGo::Planets => {
                self.run = None;
                self.show_planets(0);
            }
            SuperWorldGo::Title => {
                if let Some(w) = self.run.take().map(|r| r.world) {
                    self.open_world(cx, w);
                }
            }
            SuperWorldGo::Map => self.stage = Stage::Map(Box::new(MapScene::new(&self.ui))),
            SuperWorldGo::Play => self.play(cx),
            SuperWorldGo::Ending => {
                let c = cx
                    .host
                    .load_value("player.character")
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0usize);
                match EndingScene::new(&self.ui, c) {
                    Some(e) => self.stage = Stage::Ending(Box::new(e)),
                    None => self.show_planets(0),
                }
            }
        }
    }

    /// Play the course under Mario.
    fn play(&mut self, cx: &mut Cx) {
        let ui = self.ui.clone();
        cx.host.set_players(self.players.max(1));
        let Some(run) = &mut self.run else {
            return;
        };
        let Some((order, _)) = run.progress.course_here(&run.world) else {
            self.stage = Stage::Map(Box::new(MapScene::new(&self.ui)));
            return;
        };
        let w = run.progress.world;
        let Some(course) = run.world.course(w, order).cloned() else {
            cx.toast("This course isn't in the Super World any more.");
            self.stage = Stage::Map(Box::new(MapScene::new(&self.ui)));
            return;
        };
        let source = if course.online {
            Source::Server(course.id.clone())
        } else {
            Source::Library(course.id.clone())
        };
        match cx.host.load_course(&source) {
            Ok(loaded) => {
                run.course = Some((w, order));
                run.cleared = None;
                run.mode_line = mode_line(&ui, w);
                run.apply_counters(cx);
                let key = loaded.key.clone();
                let info = CardInfo {
                    name: course.name.clone(),
                    maker: course.maker.clone(),
                    lives: true,
                    super_world: true,
                    world_no: Some(w as u32 + 1),
                    mode_text: Some(run.mode_line.clone()),
                    best_ticks: key.as_ref().and_then(|k| cx.saves.record(k).best_ticks),
                    server_code: course.online.then(|| course.id.clone()),
                    ..Default::default()
                };
                let screens = CourseScreens::new(&self.ui);
                let p = CoursePlay::new(
                    cx,
                    course.name.clone(),
                    key,
                    screens,
                    None,
                    info,
                    true,
                    false,
                    true,
                );
                self.stage = Stage::Playing(Box::new(p));
            }
            Err(e) => {
                cx.toast(&format!("Couldn't start {}: {e}", course.name));
                self.stage = Stage::Map(Box::new(MapScene::new(&self.ui)));
            }
        }
    }

    /// The course screen closed: apply its clear (or a game over) and go back to the map.
    fn course_ended(&mut self, cx: &mut Cx) {
        let Some(run) = &mut self.run else {
            return;
        };
        let mut next = SuperWorldGo::Map;
        if let Some(order) = run.cleared.take() {
            match run.progress.clear(&run.world, order) {
                ClearResult::Map => {}
                ClearResult::WorldClear => {
                    cx.toast(&format!(
                        "{} {}!",
                        self.ui
                            .message("MyWorld", "World")
                            .unwrap_or_else(|| "World".into()),
                        run.progress.world + 1
                    ));
                }
                ClearResult::Finished => next = SuperWorldGo::Ending,
            }
        } else if run.progress.lives == 0 {
            run.progress.game_over(&run.world);
        }
        run.course = None;
        run.persist(cx);
        self.leaving = true;
        cx.fade_to(FadeKind::Black, Go::SuperWorld(next));
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<SuperWorldEvent> {
        if matches!(self.stage, Stage::None) {
            return Some(SuperWorldEvent::Exit);
        }
        if self.leaving && !matches!(self.stage, Stage::Playing(_)) {
            // (the fade is covering the screen; its scene change comes next)
            return None;
        }
        // TGRCode's picks join the planets when they arrive
        if let Some(id) = self.picks_call
            && let Some(a) = cx.online.take(id)
        {
            self.picks_call = None;
            if let Ok(v) = a {
                let online = v.as_array().into_iter().flatten().filter_map(|w| {
                    Some(Pick {
                        id: w["id"].as_str()?.to_string(),
                        maker: w["maker"]["name"].as_str().map(str::to_string),
                        worlds: w["worlds"].as_u64().unwrap_or(0) as usize,
                        courses: w["levels"].as_u64().unwrap_or(0) as usize,
                        planet: w["planet_type"].as_u64().unwrap_or(0).min(7) as u8,
                    })
                });
                self.picks
                    .extend(online.take(PLANET_SLOTS.saturating_sub(self.picks.len())));
                if let Stage::Planets(p) = &mut self.stage {
                    p.set_picks(self.picks.clone());
                }
            }
        }
        let ui = &input.ui;
        match &mut self.stage {
            Stage::Planets(p) => match p.step(ui, &mut self.script)? {
                PlanetEvent::Back => return Some(SuperWorldEvent::Exit),
                PlanetEvent::Play(pick, together) => {
                    // Play Together: one player per controller, one shared life counter
                    // (local-coop.md §1.1, the Super World's co-op)
                    self.players = if together { cx.together()? } else { 1 };
                    if let Stage::Planets(scene) = std::mem::replace(&mut self.stage, Stage::None) {
                        self.pick(cx, pick, scene);
                    }
                }
            },
            Stage::Loading(p, id) => {
                p.tick += 1;
                if let Some(a) = cx.online.take(*id) {
                    let Stage::Loading(scene, _) = std::mem::replace(&mut self.stage, Stage::None)
                    else {
                        unreachable!("matched above")
                    };
                    match a.and_then(|v| SuperWorld::from_json(&v)) {
                        Ok(w) => self.open_world(cx, w),
                        Err(e) => {
                            cx.toast(&format!("The Super World couldn't be loaded: {e}"));
                            self.stage = Stage::Planets(scene);
                        }
                    }
                }
            }
            Stage::Title(t) => match t.step(ui, &mut self.script)? {
                Ok(resume) => {
                    if let Some(run) = &mut self.run
                        && !resume
                    {
                        run.progress = Progress::new(&run.world);
                        run.persist(cx);
                    }
                    self.leaving = true;
                    cx.fade_to(FadeKind::Black, Go::SuperWorld(SuperWorldGo::Map));
                }
                Err(()) => {
                    self.leaving = true;
                    cx.fade_to(FadeKind::Black, Go::SuperWorld(SuperWorldGo::Planets));
                }
            },
            Stage::Map(m) => {
                let run = self.run.as_mut()?;
                let ev = m.step(ui, &mut self.script, &run.world, &mut run.progress);
                if !m.walking() && ev.is_none() {
                    return None;
                }
                match ev {
                    Some(MapEvent::Play) => {
                        run.persist(cx);
                        self.leaving = true;
                        cx.fade_to(FadeKind::Black, Go::SuperWorld(SuperWorldGo::Play));
                    }
                    Some(MapEvent::Pipe) => {
                        if let Some(w) = run.progress.world_map(&run.world)
                            && let Spot::Pipe(k) =
                                sw::spot(w, run.progress.x as i32, run.progress.y as i32)
                            && let Some((x, y)) = w.pipes[1 - k]
                        {
                            (run.progress.x, run.progress.y) = (x, y);
                            run.persist(cx);
                        }
                    }
                    Some(MapEvent::ToadHouse(k)) => {
                        let n = run.progress.toad_house(k);
                        run.persist(cx);
                        if n > 0 {
                            cx.toast(&format!("+{n} lives"));
                        }
                    }
                    Some(MapEvent::Quit) => {
                        run.persist(cx);
                        self.leaving = true;
                        cx.fade_to(FadeKind::Black, Go::SuperWorld(SuperWorldGo::Title));
                    }
                    None => {}
                }
            }
            Stage::Playing(p) => {
                let run = self.run.as_mut()?;
                match p.step(input, cx, Some(run)) {
                    _ if self.leaving => {}
                    Some(PlayEvent::Exit) => self.course_ended(cx),
                    Some(PlayEvent::Cleared) | None => {}
                }
            }
            Stage::Ending(e) => {
                if e.step(ui) {
                    self.leaving = true;
                    cx.fade_to(FadeKind::Black, Go::SuperWorld(SuperWorldGo::Planets));
                }
            }
            Stage::None => {}
        }
        None
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        match &mut self.stage {
            Stage::Planets(p) | Stage::Loading(p, _) => p.draw(out),
            Stage::Title(t) => t.draw(out),
            Stage::Map(m) => {
                if let Some(run) = &self.run {
                    m.draw(out, &run.world, &run.progress);
                }
            }
            Stage::Playing(p) => p.draw(out),
            Stage::Ending(e) => e.draw(out),
            Stage::None => {}
        }
    }

    pub fn playing(&mut self) -> Option<&mut CoursePlay> {
        match &mut self.stage {
            Stage::Playing(p) => Some(p),
            _ => None,
        }
    }

    /// Captures and tests: the stage shown ("planets", "loading", "title", "map", "playing",
    /// "ending").
    pub fn stage_name(&self) -> &'static str {
        match &self.stage {
            Stage::Planets(_) => "planets",
            Stage::Loading(..) => "loading",
            Stage::Title(_) => "title",
            Stage::Map(_) => "map",
            Stage::Playing(_) => "playing",
            Stage::Ending(_) => "ending",
            Stage::None => "none",
        }
    }

    /// The world being played and its progress.
    pub fn progress(&self) -> Option<(&SuperWorld, &Progress)> {
        self.run.as_ref().map(|r| (&r.world, &r.progress))
    }

    /// Captures: open the library's test world at `what` ("title", "map", "walk": the map with
    /// `cleared` courses of world 1 cleared and Mario on the next, "play": its first course,
    /// "ending").
    pub fn debug_open(&mut self, cx: &mut Cx, what: &str, cleared: usize) -> bool {
        let Some(w) = self.local_world() else {
            return false;
        };
        self.open_world(cx, w);
        if what == "title" {
            return true;
        }
        let run = self.run.as_mut().expect("opened");
        run.progress = Progress::new(&run.world);
        for k in 0..cleared.min(run.world.courses[0].len().saturating_sub(1)) {
            run.progress.clear(&run.world, k);
        }
        // Mario on the next course (or the start)
        if let Some(wm) = run.progress.world_map(&run.world) {
            let order = wm.course_order();
            if let Some(p) = order
                .get(cleared.min(order.len().saturating_sub(1)))
                .and_then(|&i| wm.points[i])
                && cleared > 0
            {
                (run.progress.x, run.progress.y) = (p.x, p.y);
            }
        }
        match what {
            "map" | "walk" => self.stage = Stage::Map(Box::new(MapScene::new(&self.ui))),
            "play" => {
                let run = self.run.as_mut().expect("opened");
                if let Some(wm) = run.progress.world_map(&run.world)
                    && let Some(p) = wm.course_order().first().and_then(|&i| wm.points[i])
                {
                    (run.progress.x, run.progress.y) = (p.x, p.y);
                }
                self.play(cx);
            }
            "ending" => self.go(cx, SuperWorldGo::Ending),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::host::CourseMeta;
    use crate::play::tests::MockHost;
    use crate::{Front, FrontInput, Options, Screen};

    fn library(n: usize) -> Vec<CourseEntry> {
        (0..n)
            .map(|i| CourseEntry {
                id: format!("/lib/{i}.course"),
                name: format!("Course {i}"),
                style: "M1".into(),
                playable: true,
                key: format!("k{i}"),
                meta: CourseMeta::default(),
                ..Default::default()
            })
            .collect()
    }

    fn front(host: &mut MockHost) -> Option<Front> {
        let ui = crate::testing::smm2()?;
        let opts = Options {
            boot: false,
            instant_fades: true,
            ..Default::default()
        };
        Some(Front::new(ui, None, opts, host))
    }

    fn press(f: &mut Front, host: &mut MockHost, b: Buttons) {
        f.tick(
            &FrontInput {
                menu: b,
                ..Default::default()
            },
            host,
        );
        f.tick(&FrontInput::default(), host);
    }

    fn idle(f: &mut Front, host: &mut MockHost, n: usize) {
        for _ in 0..n {
            f.tick(&FrontInput::default(), host);
        }
    }

    fn flow(f: &mut Front) -> &mut SuperWorldFlow {
        match &mut f.screen {
            Screen::SuperWorld(s) => s,
            _ => panic!("not on the Super Worlds"),
        }
    }

    fn saved(host: &MockHost) -> Value {
        serde_json::from_str(&host.values["superworld.local"]).unwrap()
    }

    #[test]
    fn walk_play_clear_and_save() {
        let mut host = MockHost {
            library: library(7),
            ..Default::default()
        };
        let Some(mut f) = front(&mut host) else {
            return;
        };
        assert!(f.capture(&mut host, "superworld", 0, 0));
        assert_eq!(flow(&mut f).stage_name(), "planets");
        // the library's world: planets → title (New Game) → the map
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 40);
        assert_eq!(flow(&mut f).stage_name(), "title");
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 40);
        assert_eq!(flow(&mut f).stage_name(), "map");
        // an uncleared course can't be passed; walk to the first one and play it
        press(&mut f, &mut host, Buttons::RIGHT);
        idle(&mut f, &mut host, 20);
        let (_, p) = flow(&mut f).progress().unwrap();
        assert_eq!((p.x, p.y), (1, 0));
        press(&mut f, &mut host, Buttons::RIGHT);
        idle(&mut f, &mut host, 20);
        assert_eq!(flow(&mut f).progress().unwrap().1.x, 1);
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 5);
        assert_eq!(flow(&mut f).stage_name(), "playing");
        assert_eq!(
            host.loaded.last(),
            Some(&Source::Library("/lib/0.course".into()))
        );
        // the clear, then the clear screen closes back onto the map
        host.status.cleared = true;
        host.status.goal_event = true;
        for _ in 0..600 {
            f.tick(&FrontInput::default(), &mut host);
            f.tick(
                &FrontInput {
                    menu: Buttons::CONFIRM,
                    ..Default::default()
                },
                &mut host,
            );
            if flow(&mut f).stage_name() == "map" {
                break;
            }
        }
        assert_eq!(flow(&mut f).stage_name(), "map");
        assert_eq!(saved(&host)["clears"][0], 1);
        // now the way on is open
        host.status = Default::default();
        press(&mut f, &mut host, Buttons::RIGHT);
        idle(&mut f, &mut host, 40);
        assert_eq!(flow(&mut f).progress().unwrap().1.x, 3);
    }

    #[test]
    fn play_together_plays_the_worlds_courses_with_every_controller() {
        let mut host = MockHost {
            library: library(7),
            controllers: 2,
            ..Default::default()
        };
        let Some(mut f) = front(&mut host) else {
            return;
        };
        assert!(f.capture(&mut host, "superworld", 0, 0));
        // left onto Play Together, confirm
        press(&mut f, &mut host, Buttons::LEFT);
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 40);
        assert_eq!(flow(&mut f).stage_name(), "title");
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 40);
        press(&mut f, &mut host, Buttons::RIGHT);
        idle(&mut f, &mut host, 20);
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 5);
        assert_eq!(flow(&mut f).stage_name(), "playing");
        assert_eq!(host.players.last(), Some(&2), "both controllers play");
    }

    #[test]
    fn the_hub_button_opens_them_and_back_returns_to_it() {
        let mut host = MockHost {
            library: library(3),
            ..Default::default()
        };
        let Some(mut f) = front(&mut host) else {
            return;
        };
        f.apply(&mut host, Go::CourseWorld { selection: 5 });
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 60);
        assert_eq!(flow(&mut f).stage_name(), "planets");
        press(&mut f, &mut host, Buttons::BACK);
        idle(&mut f, &mut host, 10);
        match &f.screen {
            Screen::World(w) => assert_eq!(w.hub_selection(), 5),
            _ => panic!("not back on Course World"),
        }
    }

    #[test]
    fn deaths_cost_lives_and_a_game_over_refills_them() {
        let mut host = MockHost {
            library: library(5),
            death_clock: true,
            ..Default::default()
        };
        let Some(mut f) = front(&mut host) else {
            return;
        };
        assert!(f.capture(&mut host, "superworld-play", 0, 0));
        assert_eq!(flow(&mut f).stage_name(), "playing");
        {
            let run = flow(&mut f).run.as_mut().unwrap();
            run.progress.lives = 2;
        }
        let die = |f: &mut Front, host: &mut MockHost| {
            // through the card into play, then a death sequence
            idle(f, host, 300);
            host.status.dying = true;
            host.status.restart_in = 30;
            idle(f, host, 60);
        };
        die(&mut f, &mut host);
        assert_eq!(flow(&mut f).progress().unwrap().1.lives, 1);
        die(&mut f, &mut host);
        // the game over screen, then the map with the lives refilled
        for _ in 0..600 {
            press(&mut f, &mut host, Buttons::CONFIRM);
            if flow(&mut f).stage_name() == "map" {
                break;
            }
        }
        assert_eq!(flow(&mut f).stage_name(), "map");
        let (w, p) = flow(&mut f).progress().unwrap();
        assert_eq!(p.lives, w.map.lives as u32);
        assert_eq!((p.x, p.y), (0, 0));
    }

    #[test]
    fn a_downloaded_world_opens_on_its_title() {
        let mut host = MockHost {
            server: true,
            ..Default::default()
        };
        let Some(mut f) = front(&mut host) else {
            return;
        };
        assert!(f.capture(&mut host, "superworld", 0, 0));
        let (picks_id, ..) = host.calls.last().cloned().unwrap();
        assert_eq!(host.calls.last().unwrap().1, "super_worlds");
        let id = "0123456789abcdef_20260101000000000000";
        host.answers.push((
            picks_id,
            Ok(
                serde_json::json!([{ "id": id, "worlds": 1, "levels": 3, "planet_type": 2,
                "maker": {"name": "Ada"} }]),
            ),
        ));
        idle(&mut f, &mut host, 3);
        assert_eq!(flow(&mut f).picks.len(), 1);
        press(&mut f, &mut host, Buttons::CONFIRM);
        idle(&mut f, &mut host, 40);
        assert_eq!(flow(&mut f).stage_name(), "loading");
        let (call, action, args, _) = host.calls.last().cloned().unwrap();
        assert_eq!(
            (action.as_str(), args["id"].as_str()),
            ("super_world", Some(id))
        );
        let src: Vec<SwCourse> = (0..3)
            .map(|i| SwCourse {
                id: format!("x{i}"),
                ..Default::default()
            })
            .collect();
        let map = SuperWorld::test_world(&src).unwrap().map;
        let courses: Vec<Value> = (0..3)
            .map(|i| serde_json::json!({"code": format!("AAA-BBB-CC{i}"), "name": format!("N{i}")}))
            .collect();
        host.answers.push((
            call,
            Ok(
                serde_json::json!({"id": id, "maker": {"name": "Ada"}, "courses": courses,
                "map": sw::to_hex(&map.to_bytes())}),
            ),
        ));
        idle(&mut f, &mut host, 3);
        assert_eq!(flow(&mut f).stage_name(), "title");
        let (w, _) = flow(&mut f).progress().unwrap();
        assert_eq!(w.maker.as_deref(), Some("Ada"));
        assert_eq!(w.courses[0][0].id, "AAA-BBB-CC0");
        assert!(w.courses[0][0].online);
    }
}
