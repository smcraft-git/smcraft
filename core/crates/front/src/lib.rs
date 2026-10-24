//! The SMM2 front end, platform-neutral: the screens of the Swift app's SMM2 presentation
//! (opening, scene fades, title and Play menu, Course World over a local library and the course
//! server, 100 Mario Challenge in SMM2's Endless Challenge screens or SMM1's, the in-course HUD /
//! course card / pause / clear / give up / time up / game over screens, and the UI sounds) as
//! state machines over [`mmk_ui::LayoutPlayer`]s.
//!
//! A host (the portable shell, later the Swift app through the FFI) owns the window, the engine,
//! files and audio. Each 60 Hz tick it hands [`Front::tick`] an abstract input ([`FrontInput`]:
//! menu buttons, pause, skip, a click) and a [`Host`] for the course hooks, then draws
//! [`Front::frame`]: layouts in order (quads + layout size, see [`DrawItem`]), with the course
//! view where [`DrawItem::Course`] stands, and plays the frame's [`SoundCue`]s (WAVs of the SMM2
//! UI import's `ui/sound/`, or of the SMM1 import's for the SMM1 screens: [`SoundCue::ui`]).

pub mod account;
pub mod boot;
pub mod challenge;
pub mod challenge_modes;
pub mod coop;
pub mod coursebot;
pub mod coursepage;
pub mod endless;
pub mod endless_run;
pub mod fade;
pub mod host;
pub mod layout;
pub mod leaderboard;
pub mod maker;
pub mod menu;
pub mod minigame;
pub mod mods;
pub mod mods_page;
pub mod netplay;
pub mod online;
pub mod play;
pub mod screens;
pub mod search;
pub mod smm1;
pub mod smm1_sound;
pub mod sound;
pub mod superworld;
pub mod swmap;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_online;
#[cfg(test)]
mod tests_smm1_sound;
pub mod thumbs;
pub mod title;
pub mod world;

pub use host::{CourseEntry, CourseStatus, Host, Icon, Saves, Source, WorldCourse};
pub use layout::{Layout, Ui, UiKind};
pub use menu::{Buttons, UiInput};
pub use smm1_sound::Smm1Sounds;
pub use sound::{SoundCue, UiSounds};

use challenge::{ChallengeEvent, ChallengeFlow, ChallengeGo, Presentation};
use fade::{FadeKind, FadeStart, SceneFade};
use maker::{MakerEvent, MakerFlow, MakerInput, MakerStart};
use menu::Script;
use play::PlayInput;
use title::{Title, TitleChoice, TitleEvent};
use world::{ListKind, WorldEvent, WorldFlow, WorldGo};

/// One thing to draw, back to front.
#[derive(Clone, Debug)]
pub enum DrawItem {
    /// A layout's draw list: `size` layout units (origin centre, y up) mapped onto the 16:9
    /// view; texture ids are the [`Ui`]'s of `ui`.
    Layout {
        ui: UiKind,
        size: [f32; 2],
        quads: Vec<mmk_ui::Quad>,
    },
    /// The course view (the engine's picture, the whole 16:9 view).
    Course,
}

/// Append `l`'s draw list at `tick`.
pub fn push_layout(out: &mut Vec<DrawItem>, l: &mut Layout, tick: i32) {
    let mut quads = Vec::new();
    l.draw(tick, &mut quads);
    out.push(DrawItem::Layout {
        ui: l.ui().kind(),
        size: l.size(),
        quads,
    });
}

/// What to show now.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub items: Vec<DrawItem>,
    /// A short message to show over the screen (the Swift app's toasts and alerts).
    pub toast: Option<String>,
}

/// The input for one tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrontInput {
    /// Menu buttons held.
    pub menu: Buttons,
    /// Pause (START) held.
    pub start: bool,
    /// The skip key held (100 Mario).
    pub skip: bool,
    /// A click / tap this tick, layout units (1280 × 720 view, origin centre, y up).
    pub click: Option<[f32; 2]>,
    /// A key typed this tick while a text field is open ([`Front::typing`]): a character,
    /// '\u{8}' to erase the last one, '\n' to finish, '\u{1b}' to cancel. Hosts without a
    /// keyboard leave it None (one key per tick; queue the rest).
    pub text: Option<char>,
    /// The pointer (mouse) position this tick, layout units, and its buttons held (primary,
    /// secondary). Hosts without one (touch: taps come as `click`) leave it None.
    pub pointer: Option<[f32; 2]>,
    pub pointer_down: bool,
    pub pointer_alt: bool,
    /// Course Maker keys held ([`maker::keys`]).
    pub keys: u16,
    /// Co-op making's player 2: the engine buttons (`mmk_sim::input`) of the controller that is
    /// player 2's (as [`coop::assign_pads`] gives two players their controllers), while
    /// [`Front::coop_making`]; that controller is then left out of `menu` and `start`. 0 when
    /// there is none.
    pub pad2: u32,
}

/// A deferred scene change (run when its fade has covered the screen).
#[derive(Clone, Debug, PartialEq)]
pub enum Go {
    Title {
        start: title::State,
        select: Option<usize>,
    },
    Enter(TitleChoice),
    Maker(MakerStart),
    /// The Coursebot; `from_maker`: the maker's course goes with it (Save As / Load) and comes
    /// back to the maker.
    Coursebot {
        mode: coursebot::BotMode,
        from_maker: bool,
    },
    /// Back from the Coursebot to the maker it came from.
    BackToMaker,
    /// The Coursebot's list again after a play.
    CoursebotList,
    CourseWorld {
        selection: usize,
    },
    ShowChallenge {
        presentation: Option<Presentation>,
        back: Back,
    },
    World(WorldGo),
    Challenge(ChallengeGo),
    /// Course World's Super Worlds.
    ShowSuperWorlds,
    SuperWorld(superworld::SuperWorldGo),
    /// Course World's Challenge Modes (or SMM2's Endless Challenge alone).
    ShowModes,
    Modes(challenge_modes::ModesGo),
    /// Network Play's screens (from the Course World hub).
    ShowNetplay,
    Netplay(netplay::NetGo),
}

/// Where the 100 Mario Challenge returns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    Title,
    CourseWorld,
}

/// What a screen's step may use: the host, the save store, the fade and the toast.
pub struct Cx<'a> {
    pub host: &'a mut dyn Host,
    pub saves: &'a mut Saves,
    /// The server account and the answers of server calls.
    pub online: &'a mut online::Online,
    fade: &'a mut SceneFade,
    pending: &'a mut Option<Go>,
    now: &'a mut Vec<Go>,
    toast: &'a mut Option<(String, i32)>,
}

impl Cx<'_> {
    /// Change the scene behind the fade `kind` (at once when a fade already runs).
    pub fn fade_to(&mut self, kind: FadeKind, go: Go) {
        match self.fade.run(kind, [0.0, 0.0]) {
            FadeStart::Started => *self.pending = Some(go),
            FadeStart::Now => self.now.push(go),
        }
    }

    /// Show a short message.
    pub fn toast(&mut self, text: &str) {
        *self.toast = Some((text.to_string(), TOAST_TICKS));
    }

    /// Play Together's controller step: SMM2 opens the system controller set-up for 2–4
    /// players, one per controller (local-coop.md §1.2); here the host's connected controllers
    /// are the players. None (with a message) when there's only one.
    pub fn together(&mut self) -> Option<u32> {
        let n = self.host.controllers().min(4);
        if n < 2 {
            self.toast(
                "Play Together: one controller per player (2–4). Connect another controller.",
            );
            return None;
        }
        self.toast(&format!("Play Together: {n} players"));
        Some(n)
    }
}

const TOAST_TICKS: i32 = 180;

/// Seconds since 1970 → (year, month, day, hour, minute), UTC.
pub fn civil(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Howard Hinnant's civil-from-days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (
        if m <= 2 { y + 1 } else { y },
        m,
        d,
        (rem / 3600) as u32,
        (rem / 60 % 60) as u32,
    )
}

/// How the front end starts.
#[derive(Clone, Debug)]
pub struct Options {
    /// Play the opening first.
    pub boot: bool,
    /// Force this opening (InDefault, InLink …).
    pub opening: Option<String>,
    /// The title screen's starting state when there is no opening.
    pub title_start: title::State,
    /// Start the Play menu on this button.
    pub title_select: Option<usize>,
    /// Start with this title demo course.
    pub title_course: Option<usize>,
    /// 100 Mario presentation.
    pub hundred: Presentation,
    /// Captures: scene changes happen at once (as the Swift app's frozen captures do).
    pub instant_fades: bool,
    /// Captures: menu presses at global UI ticks (`MMK_UI_SCRIPT`).
    pub script: String,
    /// Captures: title presses at title ticks (`MMK_UI_INPUT`).
    pub title_input: String,
    /// Captures of the maker: open this course file, else a new course in this style.
    pub maker_course: Option<String>,
    pub maker_style: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            boot: true,
            opening: None,
            title_start: title::State::Top,
            title_select: None,
            title_course: None,
            hundred: Presentation::Smm2,
            instant_fades: false,
            script: String::new(),
            title_input: String::new(),
            maker_course: None,
            maker_style: "M1".into(),
        }
    }
}

/// The Coursebot screen: its scene, a course played from it, the maker waiting on it.
pub struct CoursebotFlow {
    pub scene: coursebot::CoursebotScene,
    pub maker: Option<Box<MakerFlow>>,
    pub playing: Option<Box<play::CoursePlay>>,
    leaving: bool,
}

impl CoursebotFlow {
    fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<(FadeKind, Go)> {
        use coursebot::BotEvent as E;
        if let Some(p) = &mut self.playing {
            if p.step(input, cx, None) == Some(play::PlayEvent::Exit) && !self.leaving {
                self.leaving = true;
                return Some((FadeKind::Black, Go::CoursebotList));
            }
            return None;
        }
        let back_to_maker = self.maker.is_some();
        let event = self.scene.step(&input.ui, cx)?;
        let together = matches!(event, E::Together(_));
        match event {
            E::Back if back_to_maker => Some((FadeKind::Black, Go::BackToMaker)),
            // back to the title's Play menu, on the Coursebot button [inf]
            E::Back => Some((
                FadeKind::Black,
                Go::Title {
                    start: title::State::Category,
                    select: Some(2),
                },
            )),
            E::SaveTo(slot) => {
                let m = self.maker.as_mut()?;
                match m.save_to_slot(cx.host, slot) {
                    Ok(()) => {
                        if let Some(t) = self.scene.frame.message("0003") {
                            cx.toast(&t);
                        }
                    }
                    Err(e) => cx.toast(&format!("Not saved: {e}")),
                }
                Some((FadeKind::Black, Go::BackToMaker))
            }
            E::Edit {
                source,
                entry,
                based_on,
            } => Some((
                FadeKind::Black,
                Go::Maker(MakerStart::Open {
                    source,
                    entry,
                    based_on,
                }),
            )),
            E::New => Some((
                FadeKind::Black,
                Go::Maker(MakerStart::New(
                    self.maker
                        .as_ref()
                        .map_or_else(|| "M1".to_string(), |m| m.style()),
                )),
            )),
            E::Play(e) | E::Together(e) => {
                let players = if together { cx.together()? } else { 1 };
                cx.host.set_players(players);
                match cx.host.load_course(&Source::Library(e.id.clone())) {
                    Ok(loaded) => {
                        let title = if loaded.name.is_empty() {
                            e.name.clone()
                        } else {
                            loaded.name.clone()
                        };
                        let key = loaded.key.clone().or(Some(e.key.clone()));
                        let info = screens::CardInfo {
                            name: title.clone(),
                            best_ticks: key.as_ref().and_then(|k| cx.saves.record(k).best_ticks),
                            mode_label: Some("T_Mode_CourseRobot".to_string()),
                            ..Default::default()
                        };
                        let s = screens::CourseScreens::new(&self.scene.ui());
                        self.playing = Some(Box::new(play::CoursePlay::new(
                            cx, title, key, s, None, info, true, true, false,
                        )));
                    }
                    Err(err) => cx.toast(&format!("Couldn't start {}: {err}", e.name)),
                }
                None
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>) {
        match &mut self.playing {
            Some(p) => p.draw(out),
            None => self.scene.draw(out),
        }
    }
}

pub enum Screen {
    Boot(Box<boot::BootScene>),
    Title(Box<Title>),
    World(Box<WorldFlow>),
    Challenge(Box<ChallengeFlow>, Back),
    SuperWorld(Box<superworld::SuperWorldFlow>),
    /// Course World's Endless button: Challenge Modes (`challenge_modes`).
    Modes(Box<challenge_modes::ChallengeModes>),
    /// One course with the SMM2 screens (the Swift app's library play / `MMK_COURSE`).
    Course(Box<play::CoursePlay>),
    /// The Course Maker.
    Maker(Box<MakerFlow>),
    /// The Coursebot (and a course played from it, the maker waiting on it).
    Coursebot(Box<CoursebotFlow>),
    /// Network Play: the mode select, matchmaking, the lobby, the match, the result.
    Netplay(Box<netplay::NetplayFlow>),
    None,
}

/// The whole front end.
pub struct Front {
    pub smm2: Ui,
    pub smm1: Option<Ui>,
    pub screen: Screen,
    pub fade: SceneFade,
    pending: Option<Go>,
    now: Vec<Go>,
    pub saves: Saves,
    pub online: online::Online,
    toast: Option<(String, i32)>,
    held: Buttons,
    pub opts: Options,
    /// Global UI ticks.
    pub tick: i64,
    /// Set when the player left a direct course play (`Screen::Course`).
    pub course_done: bool,
    /// The Mods page and the title's button to it.
    pub mods: Option<mods_page::ModsUi>,
}

impl Front {
    pub fn new(smm2: Ui, smm1: Option<Ui>, mut opts: Options, host: &mut dyn Host) -> Self {
        // the Mods page's options: the engine's now, the 100 Mario screens for the runs
        mods::apply_engine(host);
        if let Some(p) = mods::hundred_screens(host) {
            opts.hundred = p;
        }
        let mods = Some(mods_page::ModsUi::new(&smm2, smm1.is_some()));
        let mut fade = SceneFade::new(Some(&smm2));
        fade.instant = opts.instant_fades;
        let mut f = Self {
            saves: Saves::load(host),
            online: online::Online::load(host),
            smm2,
            smm1,
            screen: Screen::None,
            fade,
            pending: None,
            now: Vec::new(),
            toast: None,
            held: Buttons::NONE,
            opts,
            tick: 0,
            course_done: false,
            mods,
        };
        let boot = if f.opts.boot {
            boot::BootScene::new(&f.smm2, f.opts.opening.as_deref(), host)
        } else {
            None
        };
        match boot {
            Some(b) => f.screen = Screen::Boot(Box::new(b)),
            None => {
                let (start, select) = (f.opts.title_start, f.opts.title_select);
                f.show_title(host, start, select);
            }
        }
        f
    }

    fn cx<'a>(&'a mut self, host: &'a mut dyn Host) -> (Cx<'a>, &'a mut Screen) {
        (
            Cx {
                host,
                saves: &mut self.saves,
                online: &mut self.online,
                fade: &mut self.fade,
                pending: &mut self.pending,
                now: &mut self.now,
                toast: &mut self.toast,
            },
            &mut self.screen,
        )
    }

    /// Run `f` with a context (for hosts and captures that drive a screen directly).
    pub fn with_cx<R>(
        &mut self,
        host: &mut dyn Host,
        f: impl FnOnce(&mut Cx, &mut Screen) -> R,
    ) -> R {
        let (mut cx, screen) = self.cx(host);
        let r = f(&mut cx, screen);
        self.run_now(host);
        r
    }

    fn show_title(&mut self, host: &mut dyn Host, start: title::State, select: Option<usize>) {
        host.unload_course();
        match Title::new(&self.smm2, start, self.opts.title_course, host) {
            Some(mut t) => {
                if let Some(i) = select {
                    t.scene.select_category_now(i);
                }
                t.scene.script = Script::parse(&self.opts.title_input);
                self.screen = Screen::Title(Box::new(t));
            }
            None => self.screen = Screen::None,
        }
    }

    fn show_world(&mut self, host: &mut dyn Host, kind: ListKind, selection: usize) {
        // (the title's demo course stops with the title)
        host.unload_course();
        let lib = host.library();
        let smm2 = self.smm2.clone();
        let script = self.opts.script.clone();
        let relabel = mods::challenge_modes(host);
        let (mut cx, screen) = self.cx(host);
        *screen = match WorldFlow::new(&smm2, kind, lib, selection, &mut cx) {
            Some(mut f) => {
                f.script = Script::parse(&script);
                if relabel {
                    f.set_endless_label(challenge_modes::BUTTON_LABEL);
                }
                Screen::World(Box::new(f))
            }
            None => Screen::None,
        };
    }

    fn show_maker(&mut self, host: &mut dyn Host, start: MakerStart) {
        host.unload_course();
        let smm2 = self.smm2.clone();
        let (mut cx, screen) = self.cx(host);
        let mut m = MakerFlow::new(&smm2, &mut cx, start);
        m.load_icons(cx.host);
        *screen = Screen::Maker(Box::new(m));
    }

    /// The Coursebot: `from_maker` takes the maker along (it comes back after Save As / Load).
    fn show_coursebot(&mut self, host: &mut dyn Host, mode: coursebot::BotMode, from_maker: bool) {
        let maker = match std::mem::replace(&mut self.screen, Screen::None) {
            Screen::Maker(m) if from_maker => Some(m),
            s => {
                self.screen = s;
                None
            }
        };
        if maker.is_none() {
            host.unload_course();
        }
        let smm2 = self.smm2.clone();
        let (mut cx, screen) = self.cx(host);
        *screen = match coursebot::CoursebotScene::new(&smm2, mode, &mut cx) {
            Some(scene) => Screen::Coursebot(Box::new(CoursebotFlow {
                scene,
                maker,
                playing: None,
                leaving: false,
            })),
            None => match maker {
                Some(m) => Screen::Maker(m),
                None => Screen::None,
            },
        };
    }

    fn show_challenge(
        &mut self,
        host: &mut dyn Host,
        presentation: Option<Presentation>,
        back: Back,
    ) {
        host.unload_course();
        // the library, then the Course World courses the host keeps for the runs
        let mut lib = host.library();
        lib.extend(host.challenge_courses());
        let (smm2, smm1) = (self.smm2.clone(), self.smm1.clone());
        let wanted = presentation.unwrap_or(self.opts.hundred);
        let (mut cx, screen) = self.cx(host);
        let flow = ChallengeFlow::new(&mut cx, &smm2, smm1.as_ref(), &lib, wanted);
        *screen = Screen::Challenge(Box::new(flow), back);
    }

    fn show_netplay(&mut self, host: &mut dyn Host) {
        host.unload_course();
        let lib = host.library();
        let smm2 = self.smm2.clone();
        let script = self.opts.script.clone();
        let (mut cx, screen) = self.cx(host);
        if let Some(mut f) = netplay::NetplayFlow::new(&smm2, &lib, &mut cx) {
            f.script = Script::parse(&script);
            *screen = Screen::Netplay(Box::new(f));
        }
    }

    fn show_super_worlds(&mut self, host: &mut dyn Host) {
        host.unload_course();
        let lib = host.library();
        let smm2 = self.smm2.clone();
        let script = self.opts.script.clone();
        let (mut cx, screen) = self.cx(host);
        let mut f = superworld::SuperWorldFlow::new(&smm2, lib, &mut cx);
        f.script = Script::parse(&script);
        *screen = Screen::SuperWorld(Box::new(f));
    }

    /// Challenge Modes, or (`switching` false) SMM2's Endless Challenge alone; None: as the
    /// Mods page says.
    fn show_modes(&mut self, host: &mut dyn Host, switching: Option<bool>) {
        host.unload_course();
        let mut lib = host.library();
        lib.extend(host.challenge_courses());
        let switching = switching.unwrap_or_else(|| mods::challenge_modes(host));
        let (smm2, smm1) = (self.smm2.clone(), self.smm1.clone());
        let hundred = self.opts.hundred;
        let script = Script::parse(&self.opts.script);
        let (mut cx, screen) = self.cx(host);
        let m = challenge_modes::ChallengeModes::new(
            &mut cx,
            &smm2,
            smm1.as_ref(),
            lib,
            hundred,
            switching,
        );
        let mut m = Box::new(m);
        m.script = script;
        *screen = Screen::Modes(m);
    }

    /// Play one course with the SMM2 screens (already loaded by the host): `intro` the card
    /// first. Unlimited retries, as library play.
    pub fn play_course(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        key: Option<String>,
        intro: bool,
    ) {
        let info = screens::CardInfo {
            name: name.to_string(),
            best_ticks: key.as_ref().and_then(|k| self.saves.record(k).best_ticks),
            ..Default::default()
        };
        let s = screens::CourseScreens::new(&self.smm2);
        let (mut cx, screen) = self.cx(host);
        let p = play::CoursePlay::new(
            &mut cx,
            name.to_string(),
            key,
            s,
            None,
            info,
            intro,
            true,
            false,
        );
        *screen = Screen::Course(Box::new(p));
    }

    fn apply(&mut self, host: &mut dyn Host, g: Go) {
        // the maker's music stops when another screen comes (it starts again on the way back)
        if let Screen::Maker(m) = &mut self.screen {
            m.leave(host);
        }
        match g {
            Go::Title { start, select } => self.show_title(host, start, select),
            Go::Enter(c) => match c {
                TitleChoice::StoryMode => self.show_world(host, ListKind::Story, 0),
                TitleChoice::CourseWorld => self.show_world(host, ListKind::World, 0),
                TitleChoice::Coursebot => {
                    self.show_coursebot(host, coursebot::BotMode::Browse, false)
                }
                TitleChoice::HundredMario => self.show_challenge(host, None, Back::Title),
                TitleChoice::Make => self.show_maker(host, MakerStart::Resume),
            },
            Go::Maker(start) => self.show_maker(host, start),
            Go::Coursebot { mode, from_maker } => self.show_coursebot(host, mode, from_maker),
            Go::BackToMaker => {
                let screen = std::mem::replace(&mut self.screen, Screen::None);
                self.screen = match screen {
                    Screen::Coursebot(mut f) => match f.maker.take() {
                        Some(mut m) => {
                            m.redraw();
                            Screen::Maker(m)
                        }
                        None => Screen::Coursebot(f),
                    },
                    s => s,
                };
                let (mut cx, screen) = self.cx(host);
                if let Screen::Maker(m) = screen {
                    m.show_now(&mut cx);
                }
            }
            Go::CoursebotList => {
                host.unload_course();
                let (mut cx, screen) = self.cx(host);
                if let Screen::Coursebot(f) = screen {
                    f.playing = None;
                    f.leaving = false;
                    f.scene.reload(&mut cx);
                    f.scene.fill(&mut cx);
                }
            }
            Go::CourseWorld { selection } => self.show_world(host, ListKind::World, selection),
            Go::ShowChallenge { presentation, back } => {
                self.show_challenge(host, presentation, back)
            }
            Go::World(w) => {
                let (mut cx, screen) = self.cx(host);
                if let Screen::World(f) = screen {
                    f.go(&mut cx, w);
                }
            }
            Go::Challenge(c) => {
                let (mut cx, screen) = self.cx(host);
                match screen {
                    Screen::Challenge(f, _) => f.go(&mut cx, c),
                    Screen::Modes(m) => m.go(&mut cx, c),
                    _ => {}
                }
            }
            Go::ShowModes => self.show_modes(host, None),
            Go::Modes(g) => {
                let (mut cx, screen) = self.cx(host);
                if let Screen::Modes(m) = screen {
                    match g {
                        challenge_modes::ModesGo::Switch => m.switch(&mut cx),
                    }
                }
            }
            Go::ShowSuperWorlds => self.show_super_worlds(host),
            Go::SuperWorld(g) => {
                let (mut cx, screen) = self.cx(host);
                if let Screen::SuperWorld(f) = screen {
                    f.go(&mut cx, g);
                }
            }
            Go::ShowNetplay => self.show_netplay(host),
            Go::Netplay(n) => {
                let (mut cx, screen) = self.cx(host);
                if let Screen::Netplay(f) = screen {
                    f.go(&mut cx, n);
                }
            }
        }
    }

    fn run_now(&mut self, host: &mut dyn Host) {
        while !self.now.is_empty() {
            let g = self.now.remove(0);
            self.apply(host, g);
        }
    }

    fn fade_to(&mut self, host: &mut dyn Host, kind: FadeKind, go: Go) {
        let (mut cx, _) = self.cx(host);
        cx.fade_to(kind, go);
        self.run_now(host);
    }

    /// One 60 Hz tick.
    pub fn tick(&mut self, input: &FrontInput, host: &mut dyn Host) {
        self.tick += 1;
        let pressed = input.menu.minus(self.held);
        self.held = input.menu;
        let ui = UiInput {
            pressed,
            held: input.menu,
            skip: input.skip,
            click: input.click,
            text: input.text,
        };
        let play = PlayInput {
            ui,
            start: input.start,
        };
        self.online.poll(host);
        if let Some((_, t)) = &mut self.toast {
            *t -= 1;
            if *t <= 0 {
                self.toast = None;
            }
        }
        let mut after: Option<(FadeKind, Go)> = None;
        let mut mods = self.mods.take();
        let mut hundred = None;
        let (mut cx, screen) = self.cx(host);
        match screen {
            Screen::Boot(b) => {
                if b.step() {
                    // the boot sequence changes to the title with the black circle [obs]
                    after = Some((
                        FadeKind::CircleBlack,
                        Go::Title {
                            start: title::State::Top,
                            select: None,
                        },
                    ));
                }
            }
            Screen::Title(t) => {
                // the Mods page's button and window take what they use first
                let mut title_ui = ui;
                if let Some(m) = &mut mods {
                    let (rest, ev) = m.title_step(&ui, t.scene.state, &mut cx);
                    title_ui = rest;
                    if let Some(mods_page::ModsEvent::Closed { hundred: Some(p) }) = ev {
                        hundred = Some(p);
                    }
                }
                let ev = t.step(&title_ui, cx.host);
                if mods.as_ref().is_some_and(|m| m.has_focus())
                    && let Some(c) = &mut t.scene.cursor
                {
                    c.set_alpha("N_All_00", 0.0);
                }
                match ev {
                    Some(TitleEvent::Choice(c)) => after = Some((FadeKind::Black, Go::Enter(c))),
                    None => {}
                }
            }
            Screen::World(f) => match f.step(&play, &mut cx) {
                Some(WorldEvent::Exit) => {
                    // back to the title's Play menu on the button that led here [inf]
                    let select = match f.kind {
                        ListKind::Story => 0,
                        ListKind::World => 1,
                        ListKind::Coursebot => 2,
                    };
                    after = Some((
                        FadeKind::Black,
                        Go::Title {
                            start: title::State::Category,
                            select: Some(select),
                        },
                    ));
                }
                Some(WorldEvent::NetworkPlay) => after = Some((FadeKind::Black, Go::ShowNetplay)),
                Some(WorldEvent::SuperWorlds) => {
                    after = Some((FadeKind::Black, Go::ShowSuperWorlds));
                }
                Some(WorldEvent::Edit {
                    source,
                    entry,
                    based_on,
                }) => {
                    after = Some((
                        FadeKind::Black,
                        Go::Maker(MakerStart::Open {
                            source,
                            entry,
                            based_on,
                        }),
                    ))
                }
                Some(WorldEvent::Endless) => after = Some((FadeKind::Black, Go::ShowModes)),
                None => {}
            },
            Screen::Challenge(f, back) => {
                if f.step(&play, &mut cx) == Some(ChallengeEvent::Exit) {
                    after = Some((
                        FadeKind::Black,
                        match back {
                            // back to the title's Play menu, on the 100 Mario button [inf]
                            Back::Title => Go::Title {
                                start: title::State::Category,
                                select: Some(3),
                            },
                            Back::CourseWorld => Go::CourseWorld { selection: 3 },
                        },
                    ));
                }
            }
            Screen::SuperWorld(f) => {
                if f.step(&play, &mut cx) == Some(superworld::SuperWorldEvent::Exit) {
                    // back to Course World's hub on its Super Worlds button [inf]
                    after = Some((FadeKind::Black, Go::CourseWorld { selection: 5 }));
                }
            }
            Screen::Modes(m) => {
                if m.step(&play, &mut cx) == Some(ChallengeEvent::Exit) {
                    // back to Course World, on its Endless button [inf]
                    after = Some((FadeKind::Black, Go::CourseWorld { selection: 3 }));
                }
            }
            Screen::Course(p) => {
                if p.step(&play, &mut cx, None) == Some(play::PlayEvent::Exit) {
                    self.course_done = true;
                }
            }
            Screen::Maker(m) => {
                let mi = MakerInput {
                    ui,
                    start: input.start,
                    play: input.skip,
                    pointer: input.pointer,
                    down: input.pointer_down,
                    alt: input.pointer_alt,
                    keys: input.keys,
                    pad2: input.pad2,
                };
                let ev = m.step(&mi, &mut cx);
                m.load_icons(cx.host);
                after = ev.map(|e| {
                    (
                        FadeKind::Black,
                        match e {
                            MakerEvent::Title => Go::Title {
                                start: title::State::Main,
                                select: None,
                            },
                            MakerEvent::StoryMode => Go::Enter(TitleChoice::StoryMode),
                            MakerEvent::CourseWorld => Go::Enter(TitleChoice::CourseWorld),
                            MakerEvent::Coursebot => Go::Enter(TitleChoice::Coursebot),
                            MakerEvent::SaveAs => Go::Coursebot {
                                mode: coursebot::BotMode::Save,
                                from_maker: true,
                            },
                            MakerEvent::Load => Go::Coursebot {
                                mode: coursebot::BotMode::Load,
                                from_maker: true,
                            },
                        },
                    )
                });
            }
            Screen::Coursebot(f) => after = f.step(&play, &mut cx),
            Screen::Netplay(f) => {
                if f.step(&play, &mut cx) == Some(netplay::NetplayEvent::Exit) {
                    // back to the hub, on the Network Play button
                    after = Some((FadeKind::Black, Go::CourseWorld { selection: 2 }));
                }
            }
            Screen::None => {}
        }
        self.mods = mods;
        if let Some(p) = hundred {
            self.opts.hundred = p;
        }
        self.run_now(host);
        if let Some((k, g)) = after {
            self.fade_to(host, k, g);
        }
        if self.fade.step()
            && let Some(g) = self.pending.take()
        {
            self.apply(host, g);
        }
    }

    /// Co-op making is on (`Edit_MultiEditWindow_00`, research/specs/maker.md C16): two players
    /// share the maker, each with a controller. The host hands player 2's controller over as
    /// [`FrontInput::pad2`] and leaves it out of the menu buttons.
    pub fn coop_making(&self) -> bool {
        matches!(&self.screen, Screen::Maker(m) if m.coop_on())
    }

    /// A text field is open: the host should send typed keys ([`FrontInput::text`]) and keep
    /// the keyboard's letter keys from moving the menus.
    pub fn typing(&self) -> bool {
        match &self.screen {
            Screen::World(f) => f.typing(),
            Screen::Coursebot(f) => f.playing.is_none() && f.scene.typing(),
            _ => false,
        }
    }

    /// What to draw and play now.
    pub fn frame(&mut self) -> Frame {
        let mut items = Vec::new();
        match &mut self.screen {
            Screen::Boot(b) => b.draw(&mut items),
            Screen::Title(t) => {
                t.draw(&mut items);
                if let Some(m) = &mut self.mods {
                    m.draw(&mut items);
                }
            }
            Screen::World(f) => f.draw(&mut items),
            Screen::Challenge(f, _) => f.draw(&mut items),
            Screen::SuperWorld(f) => f.draw(&mut items),
            Screen::Modes(m) => m.draw(&mut items),
            Screen::Course(p) => p.draw(&mut items),
            Screen::Maker(m) => m.draw(&mut items),
            Screen::Coursebot(f) => f.draw(&mut items),
            Screen::Netplay(f) => f.draw(&mut items),
            Screen::None => {}
        }
        self.fade.draw(&mut items);
        // the glyphs drawn for the first time this frame go onto their sheets before the host
        // uploads the textures
        self.smm2.cache_mut().update_glyphs();
        if let Some(u) = &self.smm1 {
            u.cache_mut().update_glyphs();
        }
        Frame {
            items,
            toast: self.toast.as_ref().map(|t| t.0.clone()),
        }
    }

    /// The player is controlling a course right now (not a menu, card, pause or result):
    /// touch screens show their on-screen controls then.
    pub fn in_play(&mut self) -> bool {
        if let Screen::Netplay(f) = &self.screen {
            return f.in_play();
        }
        let p = match &mut self.screen {
            Screen::Course(p) => Some(&mut **p),
            Screen::World(w) => w.playing(),
            Screen::Challenge(c, _) => c.playing.as_mut(),
            Screen::SuperWorld(f) => f.playing(),
            Screen::Modes(m) => m.playing(),
            _ => None,
        };
        p.is_some_and(|p| p.phase == play::Phase::Playing)
    }

    /// The UI sounds started since the last call (take them every tick).
    pub fn take_sounds(&mut self) -> Vec<SoundCue> {
        let mut sounds = self.smm2.take_cues();
        if let Some(u) = &self.smm1 {
            sounds.extend(u.take_cues());
        }
        sounds
    }

    /// Captures: jump to a screen the way the Swift app's `MMK_UI_SCREEN` does: boot, title,
    /// title-main, title-play, mods, mods-2 (the Mods page's pages), challenge-modes
    /// (-hundred, -endless: Course World's Challenge Modes), endless, endless-map (SMM2's own
    /// Endless Challenge), courseworld, courseworld-list, the online screens (account,
    /// courseworld-leaderboards, courseworld-course-page / -comments / -post / -maker / -play /
    /// -pause / -clear of the list's first server course), challenge, challenge-map,
    /// challenge-walk, challenge-gameover, challenge-ending (`difficulty`, `cleared` for the
    /// walk), superworld, superworld-title / -map / -walk / -play / -ending (the library's test
    /// world; `cleared` for the map). Returns false for an unknown name.
    pub fn capture(
        &mut self,
        host: &mut dyn Host,
        screen: &str,
        difficulty: usize,
        cleared: usize,
    ) -> bool {
        match screen {
            "boot" => {
                if let Some(b) =
                    boot::BootScene::new(&self.smm2, self.opts.opening.as_deref(), host)
                {
                    self.screen = Screen::Boot(Box::new(b));
                }
            }
            "title" | "title-main" | "title-play" => {
                let start = match screen {
                    "title-main" => title::State::Main,
                    "title-play" => title::State::Category,
                    _ => title::State::Top,
                };
                let sel = self.opts.title_select;
                self.show_title(host, start, sel);
            }
            "courseworld" => self.show_world(host, ListKind::World, 0),
            "courseworld-list" | "courseworld-list-popular" | "courseworld-list-new" => {
                self.show_world(host, ListKind::World, 0);
                self.apply(host, Go::World(WorldGo::List));
                let (mut cx, scr) = self.cx(host);
                let Screen::World(f) = scr else {
                    return false;
                };
                let tab = match screen {
                    "courseworld-list-popular" => world::Tab::Popular,
                    "courseworld-list-new" => world::Tab::New,
                    _ => world::Tab::Hot,
                };
                f.debug_tab(tab);
                f.wait_server(&mut cx, 6000);
                f.wait_details(&mut cx, 6000);
            }
            // the ID search's window, and its answer for an ID ("courseworld-id:<ID>")
            "courseworld-search" => {
                self.show_world(host, ListKind::World, 7);
                let (mut cx, scr) = self.cx(host);
                let Screen::World(f) = scr else {
                    return false;
                };
                if !f.debug_open(&mut cx, "search") {
                    return false;
                }
            }
            id if id.starts_with("courseworld-id:") => {
                self.show_world(host, ListKind::World, 7);
                let (mut cx, scr) = self.cx(host);
                let Screen::World(f) = scr else {
                    return false;
                };
                if !f.debug_open(&mut cx, &id["courseworld-".len()..]) {
                    return false;
                }
                f.wait_details(&mut cx, 6000);
            }
            // the online screens (a server: --server / MMK_SERVER; MMK_FRONT_REALTIME=1 gives
            // its answers time to arrive)
            "account" | "courseworld-leaderboards" => {
                self.show_world(
                    host,
                    ListKind::World,
                    if screen == "account" { 6 } else { 1 },
                );
                let what = if screen == "account" {
                    "account"
                } else {
                    "leaderboards"
                };
                let (mut cx, scr) = self.cx(host);
                let Screen::World(f) = scr else {
                    return false;
                };
                if !f.debug_open(&mut cx, what) {
                    return false;
                }
                self.run_now(host);
            }
            "courseworld-course-page"
            | "courseworld-comments"
            | "courseworld-post"
            | "courseworld-maker"
            | "courseworld-play"
            | "courseworld-pause"
            | "courseworld-clear" => {
                self.show_world(host, ListKind::World, 0);
                self.apply(host, Go::World(WorldGo::List));
                let what = &screen["courseworld-".len()..];
                let what = if what == "course-page" { "page" } else { what };
                let (mut cx, scr) = self.cx(host);
                let Screen::World(f) = scr else {
                    return false;
                };
                f.wait_server(&mut cx, 6000);
                if !f.debug_open(&mut cx, what) {
                    return false;
                }
                if matches!(what, "page" | "maker") {
                    f.wait_details(&mut cx, 6000);
                }
                self.run_now(host);
            }
            "storymode" => self.show_world(host, ListKind::Story, 0),
            // Network Play: the mode select; netplay-wait / -lobby / -loading / -play / -menu /
            // -clear / -result with a made-up match (versus with a `-versus` suffix)
            s if s.starts_with("netplay") => {
                self.show_netplay(host);
                let what = s.trim_start_matches("netplay").trim_start_matches('-');
                let versus = what.ends_with("versus");
                let what = what.trim_end_matches("versus").trim_end_matches('-');
                if !what.is_empty() {
                    let (mut cx, scr) = self.cx(host);
                    let Screen::Netplay(f) = scr else {
                        return false;
                    };
                    if !f.debug(&mut cx, what, versus) {
                        return false;
                    }
                }
                self.run_now(host);
            }
            // Super Worlds over the library's test world: the planets, its title, the map
            // (`cleared` courses of world 1 cleared, Mario on the next), its first course, the end
            // a Super World from a file (`superworld-json:<path>`: the hosts' `super_world`
            // answer, its map as hex), on its map
            s if s.starts_with("superworld-json:") => {
                let Some(v) = std::fs::read_to_string(&s["superworld-json:".len()..])
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                else {
                    return false;
                };
                self.show_super_worlds(host);
                let (mut cx, scr) = self.cx(host);
                let Screen::SuperWorld(f) = scr else {
                    return false;
                };
                if !f.debug_open_json(&mut cx, &v, cleared) {
                    return false;
                }
                self.run_now(host);
            }
            s if s.starts_with("superworld") => {
                self.show_super_worlds(host);
                if s != "superworld" {
                    let what = &s["superworld-".len().min(s.len())..];
                    let (mut cx, scr) = self.cx(host);
                    let Screen::SuperWorld(f) = scr else {
                        return false;
                    };
                    if !f.debug_open(&mut cx, what, cleared) {
                        return false;
                    }
                }
                self.run_now(host);
            }
            "coursebot" | "coursebot-save" | "coursebot-load" | "coursebot-window" => {
                let mode = match screen {
                    "coursebot-save" => coursebot::BotMode::Save,
                    "coursebot-load" => coursebot::BotMode::Load,
                    _ => coursebot::BotMode::Browse,
                };
                self.show_coursebot(host, mode, false);
                if screen == "coursebot-window" {
                    let (mut cx, scr) = self.cx(host);
                    if let Screen::Coursebot(f) = scr {
                        f.scene.debug_window(&mut cx);
                    }
                }
            }
            // the Course Maker: a new course (maker), its windows, test play
            s if s == "maker" || s.starts_with("maker-") => {
                // (a course to open: `maker_course`; a new one in `maker_style`)
                let start = match self.opts.maker_course.clone() {
                    Some(path) => MakerStart::Open {
                        source: Source::Library(path),
                        entry: None,
                        based_on: None,
                    },
                    None => MakerStart::New(self.opts.maker_style.clone()),
                };
                self.show_maker(host, start);
                let script = self.opts.script.clone();
                if let Screen::Maker(m) = &mut self.screen {
                    m.set_script(&script);
                }
                if let Some(what) = s.strip_prefix("maker-") {
                    let (mut cx, scr) = self.cx(host);
                    let Screen::Maker(m) = scr else {
                        return false;
                    };
                    if !m.debug_open(&mut cx, what) {
                        return false;
                    }
                    m.load_icons(cx.host);
                    self.run_now(host);
                }
            }
            // Course World's Challenge Modes on the mode used last, on one of them, SMM2's
            // Endless Challenge alone (endless; endless-map: a run's map)
            "challenge-modes" | "challenge-modes-hundred" | "challenge-modes-endless" => {
                let mode = match screen {
                    "challenge-modes-hundred" => Some("hundred"),
                    "challenge-modes-endless" => Some("endless"),
                    _ => None,
                };
                if let Some(m) = mode {
                    host.store_value(mods::CHALLENGE_MODE_KEY, m);
                }
                self.show_modes(host, Some(true));
            }
            "endless" | "endless-map" | "endless-record" => {
                self.show_modes(host, Some(false));
                let (mut cx, scr) = self.cx(host);
                let Screen::Modes(m) = scr else {
                    return false;
                };
                let challenge_modes::Flow::Endless(f) = &mut m.flow else {
                    return false;
                };
                f.debug_select(difficulty);
                if screen != "endless" {
                    f.start(&mut cx, difficulty, false);
                }
                if screen == "endless-record" {
                    f.debug_high_score(12);
                }
                self.run_now(host);
            }
            // the Mods page over the title (mods-2: its second page)
            "mods" | "mods-2" => {
                self.show_title(host, title::State::Main, None);
                let mut m = self.mods.take();
                let (mut cx, _) = self.cx(host);
                if let Some(m) = &mut m {
                    m.open(&mut cx);
                    if let (Some(p), true) = (&mut m.page, screen == "mods-2") {
                        p.turn(1, &mut cx);
                    }
                }
                self.mods = m;
            }
            s if s.starts_with("challenge") => {
                self.show_challenge(host, None, Back::Title);
                let (mut cx, scr) = self.cx(host);
                let Screen::Challenge(f, _) = scr else {
                    return false;
                };
                match s {
                    "challenge" => f.debug_select(difficulty),
                    "challenge-map" | "challenge-intro" => f.start(&mut cx, difficulty, false),
                    "challenge-walk" => f.debug_map(difficulty, false, cleared),
                    "challenge-gameover" => f.debug_game_over(),
                    "challenge-ending" => {
                        let lib = cx.host.library();
                        f.debug_ending(difficulty, &lib);
                    }
                    "challenge-pause" | "challenge-clear" | "challenge-skip" | "challenge-card" => {
                        f.start(&mut cx, difficulty, false);
                        f.play(&mut cx);
                        let what = &s["challenge-".len()..];
                        f.debug_play(what, &mut cx);
                    }
                    _ => return false,
                }
                self.run_now(host);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0));
        assert_eq!(civil(1_700_000_000), (2023, 11, 14, 22, 13));
    }
}
