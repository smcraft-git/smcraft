//! SMM2's own Endless Challenge on its own screens: the rules of `mmk_modes::endless` (a New
//! Game's 5 lives, as many courses as the lives last, 1-Ups paid at the goal, skips, Continue)
//! over the library and the Course World courses the host keeps for the runs, played offline
//! through [`Session`] (the record is updated exactly as our server would). The screens are the
//! Endless menu and map of `endless` (shared with the 100 Mario Challenge's SMM2 look) and the
//! SMM2 course screens; the run is saved under the host value `endless`.
//!
//! The map has 32 dots: an endless run goes round it (the course after n clears is dot n mod 32)
//! [inf, the original's map position rule §4.4 counts history entries the same way]. The
//! difficulty bands come from the courses' clear rates, else their difficulty values
//! (`mmk_modes::endless::pick`). Not here yet: the server's runs and the world ranking, the
//! goal time bonus (the course screens don't hand the timer on), the high score panel.

use std::collections::HashMap;

use mmk_modes::endless::{Candidate, Difficulty, Outcome, Session, Step};

use crate::challenge::{ChallengeEvent, ChallengeGo};
use crate::coop::{Asked, PlayersAsk};
use crate::endless::{EndlessBoard, EndlessEvent, EndlessMapScene, EndlessMenuScene, MapEntry};
use crate::fade::FadeKind;
use crate::host::{CourseEntry, Source};
use crate::layout::Ui;
use crate::play::{CoursePlay, PlayEvent, PlayInput, RunLink};
use crate::screens::{CardInfo, CourseScreens};
use crate::smm1::{RunCardInfo, RunQuery};
use crate::{Cx, DrawItem, Go};

/// The save's host value.
pub const SAVE_KEY: &str = "endless";
/// The header of SMM2's own Endless Challenge.
pub const TITLE: &str = "Endless Challenge";
/// Dots on the map.
const MAP_DOTS: usize = 32;

enum Scene {
    Menu(Box<EndlessMenuScene>),
    Map(Box<EndlessMapScene>),
    None,
}

/// What comes after the course screen closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After {
    /// The map, on to the next course.
    Next,
    GameOver {
        clears: u32,
        best: u32,
    },
    /// The library has no course left for this run (it is kept).
    OutOfCourses,
    /// Quit (Save & Quit, or the clear screen's Quit): the menu.
    Quit,
}

/// The run, apart from the screens (the course play talks to it as a [`RunLink`]).
pub struct EndlessRun {
    pub session: Session,
    /// The library's courses by id (path), for names, makers, themes, codes.
    entries: HashMap<String, CourseEntry>,
    pub difficulty: usize,
    /// The course being played (the session moves on as soon as it is reported).
    course: Option<Candidate>,
    /// The counters the course started with (score, HUD coins).
    start: (u32, u32),
    after: Option<After>,
}

impl RunQuery for EndlessRun {
    fn unlocked(&self, _d: usize) -> bool {
        true
    }

    fn can_continue(&self, d: usize) -> bool {
        Difficulty::from_index(d).is_some_and(|d| self.session.save.can_continue(d))
    }

    fn continue_info(&self, d: usize) -> Option<(u32, usize)> {
        if !self.can_continue(d) {
            return None;
        }
        let r = self.session.save.run(Difficulty::from_index(d)?)?;
        Some((r.status.lives, r.status.clears as usize))
    }

    fn run_length(&self, _d: usize) -> usize {
        MAP_DOTS
    }

    fn new_game_lives(&self, d: usize) -> u32 {
        Difficulty::from_index(d).map_or(5, Difficulty::start_lives)
    }

    fn rule_text(&self, d: usize) -> Option<String> {
        let best = self.session.save.best[d.min(3)];
        let lives = self.new_game_lives(d);
        Some(if best > 0 {
            format!("{lives} lives, as many courses as you can. Best: {best}")
        } else {
            format!("{lives} lives, as many courses as you can")
        })
    }

    fn map_dot(&self, cleared: usize) -> usize {
        cleared % MAP_DOTS
    }
}

impl EndlessRun {
    fn persist(&self, cx: &mut Cx) {
        cx.host.store_value(SAVE_KEY, &self.session.save_json());
    }

    fn apply(&self, cx: &mut Cx) {
        let s = self.session.status();
        cx.host.set_run_counters(s.score, s.coins, s.lives);
    }

    fn step_to_after(&mut self, step: Step) {
        self.after = Some(match step {
            Step::Play | Step::Retry => After::Next,
            Step::GameOver(f) => After::GameOver {
                clears: f.clears,
                best: f.best,
            },
            Step::OutOfCourses => After::OutOfCourses,
        });
    }

    fn entry(&self) -> Option<&CourseEntry> {
        self.entries.get(&self.course.as_ref()?.id)
    }
}

impl RunLink for EndlessRun {
    fn died(&mut self, cx: &mut Cx) -> bool {
        let outcome = if cx.host.course_status().time_up {
            Outcome::TimeUp
        } else {
            Outcome::Death
        };
        let step = self.session.report(outcome, 0);
        self.persist(cx);
        if step == Step::Retry {
            self.apply(cx);
            let s = self.session.status();
            self.start = (s.score, s.coins);
            return true;
        }
        self.step_to_after(step);
        false
    }

    fn cleared(&mut self, score: u32, coins: u32) {
        // what the course added on the HUD (its coin counter wraps at 100)
        let (s0, c0) = self.start;
        let coins_gained = if coins >= c0 {
            coins - c0
        } else {
            coins + 100 - c0
        };
        if let Some(r) = self.session.run_mut() {
            r.current.coins = r.current.coins.saturating_add(coins_gained);
            r.add_score(score.saturating_sub(s0));
        }
        let step = self.session.report(Outcome::Clear, 0);
        self.step_to_after(step);
    }

    fn skipped(&mut self) {
        let step = self.session.report(Outcome::Skip, 0);
        self.step_to_after(step);
    }

    fn save_and_quit(&mut self) {
        self.session.suspend();
        self.after = Some(After::Quit);
    }

    fn save_and_quit_after_clear(&mut self) {
        self.session.suspend();
        self.after = Some(After::Quit);
    }

    fn extra_lives(&mut self, n: u32) {
        // 1-Ups wait for the goal (at most 3 a course)
        self.session.one_up(n);
    }

    fn apply_counters(&mut self, cx: &mut Cx) {
        self.apply(cx);
    }

    fn can_skip(&self) -> bool {
        true
    }

    fn pause_line(&self) -> String {
        let s = self.session.status();
        format!("{TITLE} ({} cleared)", s.clears)
    }

    fn course_name(&self) -> String {
        self.course
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }

    fn course_maker(&self) -> String {
        self.course
            .as_ref()
            .map(|c| c.maker.clone())
            .unwrap_or_default()
    }

    fn clear_ends_run(&self) -> bool {
        false
    }

    fn world_record_ms(&self) -> Option<i64> {
        self.entry()?.meta.world_record_ms
    }

    fn card_info(&self) -> RunCardInfo {
        let s = self.session.status();
        RunCardInfo {
            name: self.course_name(),
            maker: self.course_maker(),
            code: self.entry().and_then(|e| e.meta.code.clone()),
            lives: s.lives,
            number: s.clears + 1,
            total: 0,
        }
    }
}

/// SMM2's Endless Challenge: the menu → the map → a course → the map …, until game over or
/// Quit.
pub struct EndlessFlow {
    ui: Ui,
    scene: Scene,
    pub playing: Option<CoursePlay>,
    pub run: EndlessRun,
    handoff: Option<EndlessBoard>,
    /// The menu's header (Challenge Modes puts its ZL / ZR hint there).
    pub header: String,
    /// Captures: start the course without the card.
    pub intro: bool,
    /// Co-op: the run's players (one shared life counter), and the question before a run.
    pub players: u32,
    asking: Option<PlayersAsk>,
}

impl EndlessFlow {
    /// `library`: the courses the runs draw from (only the playable ones are used).
    pub fn new(cx: &mut Cx, ui: &Ui, library: &[CourseEntry], header: Option<String>) -> Self {
        let save = cx.host.load_value(SAVE_KEY);
        let seed = cx.host.random().max(1);
        let mut session = Session::from_json(save.as_deref(), seed);
        let mut entries = HashMap::new();
        for e in library.iter().filter(|e| e.playable) {
            if entries.contains_key(&e.id) {
                continue;
            }
            let m = &e.meta;
            session.add_course(Candidate {
                id: e.id.clone(),
                name: e.name.clone(),
                maker: m.maker.clone().unwrap_or_default(),
                owner: String::new(),
                style: e.style.clone(),
                theme: e.theme,
                attempts: m.attempts.unwrap_or(0).max(0) as u64,
                clears: m.clears.unwrap_or(0).max(0) as u64,
                hint: Some(Difficulty::from_difficulty_value(m.difficulty_value())),
                broken: false,
            });
            entries.insert(e.id.clone(), e.clone());
        }
        let difficulty = session.save.last.map_or(0, Difficulty::index);
        let mut f = Self {
            ui: ui.clone(),
            scene: Scene::None,
            playing: None,
            run: EndlessRun {
                session,
                entries,
                difficulty,
                course: None,
                start: (0, 0),
                after: None,
            },
            handoff: None,
            header: header.unwrap_or_else(|| TITLE.to_string()),
            intro: true,
            players: 1,
            asking: None,
        };
        f.make_menu();
        f
    }

    fn make_menu(&mut self) {
        self.handoff = None;
        let d = self.run.difficulty;
        self.scene = match EndlessMenuScene::new(&self.ui, d, &self.run) {
            Some(mut s) => {
                if let Some(b) = &mut s.board {
                    b.layout.text("T_Title_00", &self.header);
                }
                Scene::Menu(Box::new(s))
            }
            None => Scene::None,
        };
    }

    /// The difficulty select is up (Challenge Modes may switch away from it).
    pub fn at_menu(&self) -> bool {
        self.playing.is_none() && matches!(self.scene, Scene::Menu(_))
    }

    pub fn start(&mut self, cx: &mut Cx, d: usize, resume: bool) {
        let Some(diff) = Difficulty::from_index(d) else {
            return;
        };
        self.run.difficulty = d;
        let ok = if resume {
            self.run.session.continue_run(diff)
        } else {
            self.run.session.new_game(diff)
        };
        if ok.is_err() {
            cx.toast("There aren't any playable courses for the Endless Challenge yet.");
            self.make_menu();
            return;
        }
        self.run.persist(cx);
        self.show_map(if resume {
            MapEntry::Resume
        } else {
            MapEntry::NewGame
        });
    }

    fn show_map(&mut self, entry: MapEntry) {
        let Some(c) = self.run.session.current().cloned() else {
            self.make_menu();
            return;
        };
        let s = self.run.session.status();
        let dot = self.run.map_dot(s.clears as usize);
        let theme = self.run.entries.get(&c.id).map(|e| e.theme);
        let handoff = self.handoff.take().map(|mut b| {
            b.layout.text("T_Title_00", TITLE);
            b
        });
        self.run.course = Some(c);
        self.scene = match EndlessMapScene::new(
            &self.ui,
            entry,
            self.run.difficulty,
            s.lives,
            dot,
            theme,
            handoff,
        ) {
            Some(mut m) => {
                m.board.layout.text("T_Title_00", TITLE);
                m.board.set_counters(s.lives, s.clears as usize);
                Scene::Map(Box::new(m))
            }
            None => Scene::None,
        };
    }

    /// Load and play the current course (an unreadable one is passed at no cost).
    pub fn play(&mut self, cx: &mut Cx) {
        let Some(course) = self.run.session.current().cloned() else {
            self.make_menu();
            return;
        };
        self.run.course = Some(course.clone());
        cx.host.set_players(self.players.max(1));
        match cx.host.load_course(&Source::Library(course.id.clone())) {
            Ok(loaded) => {
                self.run.apply(cx);
                let s = self.run.session.status();
                self.run.start = (s.score, s.coins);
                let entry = self.run.entries.get(&course.id);
                let title = entry.map_or(loaded.name.clone(), |e| e.name.clone());
                let key = entry.map(|e| e.key.clone()).or(loaded.key);
                let info = CardInfo {
                    name: title.clone(),
                    maker: (!course.maker.is_empty()).then(|| course.maker.clone()),
                    lives: true,
                    mode_text: Some(self.run.pause_line()),
                    best_ticks: key.as_ref().and_then(|k| cx.saves.record(k).best_ticks),
                    ..Default::default()
                };
                let p = CoursePlay::new(
                    cx,
                    title,
                    key,
                    CourseScreens::new(&self.ui),
                    None,
                    info,
                    self.intro,
                    false,
                    true,
                );
                self.scene = Scene::None;
                self.playing = Some(p);
            }
            Err(_) => {
                let step = self.run.session.report(Outcome::Broken, 0);
                self.run.persist(cx);
                cx.toast(&format!(
                    "{} couldn't be loaded: on to the next one.",
                    course.name
                ));
                match step {
                    Step::Play | Step::Retry => self.show_map(MapEntry::Resume),
                    _ => self.back_to_select(cx),
                }
            }
        }
    }

    pub fn back_to_select(&mut self, cx: &mut Cx) {
        self.playing = None;
        self.run.course = None;
        cx.host.unload_course();
        self.make_menu();
    }

    /// The course screen closed (a clear's result, a skip, game over or Save & Quit).
    fn session_ended(&mut self, cx: &mut Cx) {
        self.playing = None;
        let after = self.run.after.take().unwrap_or(After::Quit);
        self.run.persist(cx);
        match after {
            After::Next => {
                cx.host.unload_course();
                self.show_map(MapEntry::AfterClear);
                if !matches!(self.scene, Scene::Map(_)) {
                    cx.toast("The library ran out of courses for this run. Your run is saved.");
                }
            }
            After::GameOver { clears, best } => {
                let msg = if clears >= best && clears > 0 {
                    format!("Game over: {clears} courses cleared, a new best!")
                } else {
                    format!("Game over: {clears} courses cleared.")
                };
                cx.toast(&msg);
                self.run.session.end();
                self.back_to_select(cx);
            }
            After::OutOfCourses => {
                cx.toast("The library ran out of courses for this run. Your run is saved.");
                self.run.session.end();
                self.back_to_select(cx);
            }
            After::Quit => {
                self.run.session.end();
                self.back_to_select(cx);
            }
        }
    }

    /// Run a deferred step (after its fade covered the screen).
    pub fn go(&mut self, cx: &mut Cx, g: ChallengeGo) {
        match g {
            ChallengeGo::BackToSelect => {
                self.run.session.suspend();
                self.run.persist(cx);
                self.run.session.end();
                self.back_to_select(cx);
            }
            ChallengeGo::Play => self.play(cx),
        }
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<ChallengeEvent> {
        if let Some(p) = &mut self.playing {
            if p.step(input, cx, Some(&mut self.run)) == Some(PlayEvent::Exit) {
                self.session_ended(cx);
            }
            return None;
        }
        if let Some(a) = &mut self.asking {
            match a.step(&input.ui, cx) {
                None => {}
                Some(Asked::Players(n)) => {
                    let (d, resume) = (a.difficulty, a.resume);
                    self.asking = None;
                    self.players = n;
                    if let Scene::Menu(s) = &mut self.scene {
                        self.handoff = s.board.take();
                    }
                    self.start(cx, d, resume);
                }
                Some(Asked::Cancel) => {
                    self.asking = None;
                    self.make_menu();
                }
            }
            return None;
        }
        match &mut self.scene {
            Scene::Menu(s) => match s.step(&input.ui, &self.run)? {
                EndlessEvent::Exit => {
                    self.run.session.end();
                    return Some(ChallengeEvent::Exit);
                }
                EndlessEvent::Start { difficulty, resume } => {
                    match PlayersAsk::open(cx, difficulty, resume) {
                        Some(a) => {
                            cx.toast(&a.question());
                            self.asking = Some(a);
                        }
                        None => {
                            self.players = 1;
                            self.handoff = s.board.take();
                            self.start(cx, difficulty, resume);
                        }
                    }
                }
                _ => {}
            },
            Scene::Map(s) => match s.step(&input.ui)? {
                EndlessEvent::BackToSelect => {
                    cx.fade_to(FadeKind::Black, Go::Challenge(ChallengeGo::BackToSelect))
                }
                EndlessEvent::Play => cx.fade_to(FadeKind::Black, Go::Challenge(ChallengeGo::Play)),
                _ => {}
            },
            Scene::None => {}
        }
        None
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if let Some(p) = &mut self.playing {
            p.draw(out);
            return;
        }
        match &mut self.scene {
            Scene::Menu(s) => s.draw(out),
            Scene::Map(s) => s.draw(out),
            Scene::None => {}
        }
    }

    // MARK: captures

    /// The select screen on difficulty `d`.
    pub fn debug_select(&mut self, d: usize) {
        self.run.difficulty = d.min(3);
        self.make_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buttons;
    use crate::host::{CourseMeta, Saves};
    use crate::mods::tests::MemHost;
    use crate::testing::{press, smm2};

    fn library(n: usize) -> Vec<CourseEntry> {
        (0..n)
            .map(|i| CourseEntry {
                id: format!("c{i}"),
                name: format!("Course {i}"),
                style: "M1".into(),
                playable: true,
                key: format!("k{i}"),
                meta: CourseMeta {
                    clear_rate: Some(60.0),
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect()
    }

    fn with<R>(host: &mut MemHost, saves: &mut Saves, f: impl FnOnce(&mut Cx) -> R) -> R {
        let mut online = crate::online::Online::default();
        let mut fade = crate::fade::SceneFade::new(None);
        fade.instant = true;
        let (mut pending, mut now, mut toast) = (None, Vec::new(), None);
        let mut cx = Cx {
            host,
            saves,
            online: &mut online,
            fade: &mut fade,
            pending: &mut pending,
            now: &mut now,
            toast: &mut toast,
        };
        f(&mut cx)
    }

    #[test]
    fn a_run_follows_smm2_rules_and_is_saved() {
        let mut host = MemHost::default();
        let mut saves = Saves::default();
        let lib = library(12);
        let mut run = EndlessRun {
            session: Session::from_json(None, 3),
            entries: HashMap::new(),
            difficulty: 0,
            course: None,
            start: (0, 0),
            after: None,
        };
        for e in &lib {
            run.session.add_course(Candidate {
                id: e.id.clone(),
                hint: Some(Difficulty::Easy),
                ..Default::default()
            });
        }
        assert_eq!(run.new_game_lives(0), 5);
        assert!(run.rule_text(0).unwrap().starts_with("5 lives"));
        run.session.new_game(Difficulty::Easy).unwrap();
        // a miss costs a life and retries the course
        assert!(with(&mut host, &mut saves, |cx| run.died(cx)));
        assert_eq!(run.session.status().lives, 4);
        assert_eq!(host.counters.map(|c| c.2), Some(4));
        // a 1-Up waits for the goal, then the clear counts
        run.extra_lives(1);
        run.cleared(1000, 10);
        assert_eq!(run.after, Some(After::Next));
        let s = run.session.status();
        assert_eq!((s.clears, s.lives, s.score), (1, 5, 1000));
        // four more misses on the next course: game over
        for _ in 0..4 {
            with(&mut host, &mut saves, |cx| run.died(cx));
        }
        assert!(!with(&mut host, &mut saves, |cx| run.died(cx)));
        assert_eq!(run.after, Some(After::GameOver { clears: 1, best: 1 }));
        assert!(host.values.contains_key(SAVE_KEY));
        assert!(!run.can_continue(0));
    }

    #[test]
    fn the_menu_starts_a_run_and_the_map_plays_it() {
        let Some(ui) = smm2() else { return };
        let mut host = MemHost::default();
        let mut saves = Saves::default();
        let lib = library(12);
        let mut f = with(&mut host, &mut saves, |cx| {
            EndlessFlow::new(cx, &ui, &lib, None)
        });
        assert!(f.at_menu());
        let step = |f: &mut EndlessFlow, host: &mut MemHost, saves: &mut Saves, b: Buttons| {
            with(host, saves, |cx| {
                let i = PlayInput {
                    ui: press(b),
                    start: false,
                };
                f.step(&i, cx)
            })
        };
        for _ in 0..40 {
            step(&mut f, &mut host, &mut saves, Buttons::NONE);
        }
        // R: the Normal tab; L back to Easy
        step(&mut f, &mut host, &mut saves, Buttons::R);
        assert_eq!(
            f.run.difficulty, 0,
            "the menu's own difficulty moved, the run's not yet"
        );
        step(&mut f, &mut host, &mut saves, Buttons::L);
        step(&mut f, &mut host, &mut saves, Buttons::CONFIRM);
        for _ in 0..60 {
            step(&mut f, &mut host, &mut saves, Buttons::NONE);
        }
        assert!(matches!(f.scene, Scene::Map(_)));
        assert!(f.run.session.run().is_some());
        // the course loads from the map
        with(&mut host, &mut saves, |cx| f.play(cx));
        assert!(f.playing.is_some());
        assert_eq!(host.loaded.len(), 1);
    }
}
