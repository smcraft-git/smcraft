//! The 100 Mario Challenge: difficulty select → map / intro → course → result, until the run is
//! cleared, lost or saved (the Swift app's `ChallengeFlow`). Rules, pools and save data are
//! `mmk_modes::challenge`; this sequences the screens — SMM2's Endless Challenge screens
//! (`endless`, the default) or SMM1's own (`smm1`) — and owns the course being played.

use std::collections::HashMap;

use mmk_modes::challenge::Challenge;
use mmk_modes::hundred::{Difficulty, Next, Outcome};
use mmk_modes::pool::CourseRef;

use crate::endless::{
    self, EndlessBoard, EndlessEvent, EndlessMapScene, EndlessMenuScene, MapEntry,
};
use crate::fade::FadeKind;
use crate::host::{CourseEntry, Source};
use crate::layout::{Layout, Ui};
use crate::menu::UiInput;
use crate::play::{CoursePlay, PlayEvent, PlayInput, RunLink};
use crate::screens::{CardInfo, CourseScreens};
use crate::smm1::{
    ChallengeMenuScene, EndingScene, Entry, GameOverScene, MapIntroScene, RUN_LENGTHS, RunCardInfo,
    RunOverlay, RunQuery, Smm1Event, WorldMap,
};
use crate::{Cx, DrawItem, Go};

pub const DIFFICULTIES: [&str; 4] = ["Easy", "Normal", "Expert", "Super Expert"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Smm1,
    Smm2,
}

/// Deferred work the flow asks the front end to run (behind a fade).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeGo {
    BackToSelect,
    Play,
}

enum Scene {
    Menu(EndlessMenuScene),
    Map(EndlessMapScene),
    Smm1Menu(ChallengeMenuScene),
    Smm1Intro(MapIntroScene),
    GameOver(GameOverScene),
    Ending(EndingScene),
    None,
}

/// The run itself, apart from the screens (the course play talks to it as a [`RunLink`]).
pub struct RunState {
    pub challenge: Challenge,
    pub difficulty: usize,
    pub course: Option<CourseRef>,
    pending_next: Option<Next>,
    entry: Entry,
    pub unlocked: Option<usize>,
    pub credits: Vec<(String, String)>,
    /// The library's courses by id (path), for names, makers, themes, codes.
    entries: HashMap<String, CourseEntry>,
}

pub struct ChallengeFlow {
    pub presentation: Presentation,
    ui: Ui,
    scene: Scene,
    pub playing: Option<CoursePlay>,
    pub run: RunState,
    /// Map and title bar from the SMM1 select screen to the MapIn that follows.
    handoff: Option<(WorldMap, Layout)>,
    /// SMM2: the Endless menu handed to the run's map (ChangeMap continues it).
    endless_handoff: Option<EndlessBoard>,
    /// Captures: start the course without the card (`MMK_UI_INTRO=0`).
    pub intro: bool,
    /// The SMM2 menu's header instead of "100 Mario Challenge" (Challenge Modes' ZL / ZR hint).
    pub header: Option<String>,
    /// The finished run's costume reward, announced after the ending.
    reward: Option<String>,
}

/// What a step tells the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeEvent {
    /// The player left the challenge.
    Exit,
}

impl RunQuery for RunState {
    fn unlocked(&self, d: usize) -> bool {
        self.challenge.save.unlocked(Difficulty::ALL[d.min(3)])
    }

    fn can_continue(&self, d: usize) -> bool {
        self.challenge.save.can_continue(Difficulty::ALL[d.min(3)])
    }

    fn continue_info(&self, d: usize) -> Option<(u32, usize)> {
        if !self.can_continue(d) {
            return None;
        }
        let r = self.challenge.save.records[d.min(3)].as_ref()?;
        Some((r.counters.lives, r.cleared.len()))
    }
}

impl RunState {
    fn persist(&self, cx: &mut Cx) {
        cx.host.store_value("hundred", &self.challenge.save_json());
    }

    pub fn status(&self) -> mmk_modes::challenge::Status {
        self.challenge.status()
    }

    fn apply(&mut self, cx: &mut Cx) {
        let s = self.status();
        cx.host.set_run_counters(s.score, s.coins, s.lives);
    }

    pub fn pause_line(&self) -> String {
        let s = self.status();
        format!("100 Mario Challenge ({}/{})", s.course_number, s.total)
    }

    fn collect_credits(&mut self) {
        self.credits = self
            .challenge
            .cleared()
            .iter()
            .map(|c| (c.name.clone(), c.maker.clone()))
            .collect();
    }
}

impl RunLink for RunState {
    fn died(&mut self, cx: &mut Cx) -> bool {
        let n = self.challenge.finish(Outcome::Died, 0, 0);
        self.persist(cx);
        if n == Next::Retry {
            self.apply(cx);
            return true;
        }
        self.pending_next = Some(n);
        false
    }

    fn cleared(&mut self, score: u32, coins: u32) {
        self.entry = Entry::AfterClear;
        let n = self.challenge.finish(Outcome::Cleared, score, coins);
        self.pending_next = Some(n);
        if n == Next::Complete {
            self.collect_credits();
        }
    }

    fn skipped(&mut self) {
        self.entry = Entry::Direct;
        self.pending_next = Some(self.challenge.finish(Outcome::Skipped, 0, 0));
    }

    fn save_and_quit(&mut self) {
        self.pending_next = Some(self.challenge.finish(Outcome::SaveAndQuit, 0, 0));
    }

    fn save_and_quit_after_clear(&mut self) {
        self.pending_next = Some(Next::Quit);
    }

    fn extra_lives(&mut self, n: u32) {
        self.challenge.extra_lives(n);
    }

    fn apply_counters(&mut self, cx: &mut Cx) {
        self.apply(cx);
    }

    fn can_skip(&self) -> bool {
        true
    }

    fn pause_line(&self) -> String {
        RunState::pause_line(self)
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
        self.pending_next == Some(Next::Complete)
    }

    fn world_record_ms(&self) -> Option<i64> {
        let c = self.course.as_ref()?;
        self.entries.get(&c.id)?.meta.world_record_ms
    }

    fn card_info(&self) -> RunCardInfo {
        let s = self.status();
        RunCardInfo {
            name: self.course_name(),
            maker: self.course_maker(),
            code: self
                .course
                .as_ref()
                .and_then(|c| self.entries.get(&c.id))
                .and_then(|e| e.meta.code.clone()),
            lives: s.lives,
            number: s.course_number,
            total: s.total,
        }
    }
}

impl ChallengeFlow {
    /// `library`: the courses offered to the pools (only the playable ones are used). `smm1`:
    /// the SMM1 import (for that presentation).
    pub fn new(
        cx: &mut Cx,
        smm2: &Ui,
        smm1: Option<&Ui>,
        library: &[CourseEntry],
        wanted: Presentation,
    ) -> Self {
        let (presentation, ui) = match (wanted, smm1) {
            (Presentation::Smm1, Some(u)) => (Presentation::Smm1, u.clone()),
            _ => (Presentation::Smm2, smm2.clone()),
        };
        let save = cx.host.load_value("hundred");
        let seed = cx.host.random().max(1);
        let mut challenge = Challenge::from_json(save.as_deref(), seed);
        let mut entries = HashMap::new();
        for e in library.iter().filter(|e| e.playable) {
            challenge.add_course(
                CourseRef {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    maker: e.meta.maker.clone().unwrap_or_default(),
                    difficulty: e.meta.difficulty_value(),
                },
                e.theme == 2,
            );
            entries.insert(e.id.clone(), e.clone());
        }
        let mut f = Self {
            presentation,
            ui,
            scene: Scene::None,
            playing: None,
            run: RunState {
                challenge,
                difficulty: 0,
                course: None,
                pending_next: None,
                entry: Entry::NewGame,
                unlocked: None,
                credits: Vec::new(),
                entries,
            },
            handoff: None,
            endless_handoff: None,
            intro: true,
            header: None,
            reward: None,
        };
        f.make_menu();
        f
    }

    /// Show `header` over the SMM2 menu (Challenge Modes).
    pub fn set_header(&mut self, header: &str) {
        self.header = Some(header.to_string());
        if let Scene::Menu(s) = &mut self.scene
            && let Some(b) = &mut s.board
        {
            b.layout.text("T_Title_00", header);
        }
    }

    /// The difficulty select is up (Challenge Modes may switch away from it).
    pub fn at_menu(&self) -> bool {
        self.playing.is_none() && matches!(self.scene, Scene::Menu(_) | Scene::Smm1Menu(_))
    }

    fn smm2(&self) -> bool {
        self.presentation == Presentation::Smm2
    }

    fn make_menu(&mut self) {
        self.handoff = None;
        self.endless_handoff = None;
        let d = self.run.difficulty;
        self.scene = if self.smm2() {
            let mut m = EndlessMenuScene::new(&self.ui, d, &self.run);
            if let (Some(m), Some(h)) = (&mut m, &self.header)
                && let Some(b) = &mut m.board
            {
                b.layout.text("T_Title_00", h);
            }
            m.map_or(Scene::None, Scene::Menu)
        } else {
            ChallengeMenuScene::new(&self.ui, d, &self.run).map_or(Scene::None, Scene::Smm1Menu)
        };
    }

    pub fn start(&mut self, cx: &mut Cx, d: usize, resume: bool) {
        self.run.difficulty = d;
        self.run.unlocked = None;
        self.run.entry = if resume {
            Entry::Resume
        } else {
            Entry::NewGame
        };
        if self
            .run
            .challenge
            .start(Difficulty::ALL[d], resume)
            .is_err()
        {
            let msg = if self.run.challenge.available(Difficulty::ALL[d]) == 0 {
                format!(
                    "There aren't any playable {} courses in your library yet.",
                    DIFFICULTIES[d]
                )
            } else {
                "Couldn't start the run: not enough courses.".to_string()
            };
            cx.toast(&msg);
            // the select screen stays (its Decide started the run)
            self.make_menu();
            return;
        }
        self.run.persist(cx);
        self.show_intro(cx);
    }

    fn show_intro(&mut self, cx: &mut Cx) {
        let Some(c) = self.run.challenge.current().cloned() else {
            cx.toast("The library ran out of courses for this run. Your progress is saved.");
            self.make_menu();
            return;
        };
        self.run.course = Some(c.clone());
        let status = self.run.status();
        let cleared = status.course_number.saturating_sub(1) as usize;
        let d = self.run.difficulty;
        if self.smm2() {
            let e = match self.run.entry {
                Entry::NewGame => MapEntry::NewGame,
                Entry::Resume => MapEntry::Resume,
                Entry::AfterClear | Entry::Direct if cleared > 0 => MapEntry::AfterClear,
                _ => MapEntry::Resume,
            };
            let theme = self.run.entries.get(&c.id).map(|e| e.theme);
            let handoff = self.endless_handoff.take();
            self.scene =
                EndlessMapScene::new(&self.ui, e, d, status.lives, cleared, theme, handoff)
                    .map_or(Scene::None, Scene::Map);
        } else {
            let info = self.run.card_info();
            let handoff = self.handoff.take();
            self.scene = MapIntroScene::new(&self.ui, self.run.entry, d, &info, cleared, handoff)
                .map_or(Scene::None, Scene::Smm1Intro);
        }
        self.handoff = None;
        self.run.entry = Entry::Direct;
    }

    /// Load and play the current course (an unreadable one is replaced).
    pub fn play(&mut self, cx: &mut Cx) {
        let Some(course) = self.run.course.clone() else {
            return;
        };
        let entry = self.run.entries.get(&course.id).cloned();
        match cx.host.load_course(&Source::Library(course.id.clone())) {
            Ok(loaded) => {
                self.run.apply(cx);
                let title = entry
                    .as_ref()
                    .map(|e| e.name.clone())
                    .unwrap_or(loaded.name.clone());
                let key = entry.as_ref().map(|e| e.key.clone()).or(loaded.key);
                let (screens, overlay) = if self.smm2() {
                    (CourseScreens::new(&self.ui), None)
                } else {
                    (None, Some(RunOverlay::new(&self.ui)))
                };
                let info = CardInfo {
                    name: title.clone(),
                    maker: (!course.maker.is_empty()).then(|| course.maker.clone()),
                    lives: true,
                    mode_text: Some(self.run.pause_line()),
                    best_ticks: key.as_ref().and_then(|k| cx.saves.record(k).best_ticks),
                    ..Default::default()
                };
                let p = CoursePlay::new(
                    cx, title, key, screens, overlay, info, self.intro, false, true,
                );
                self.run.challenge.course_started();
                self.run.persist(cx);
                self.scene = Scene::None;
                self.playing = Some(p);
            }
            Err(_) => {
                if self.run.challenge.unloadable().is_ok() {
                    cx.toast(&format!(
                        "{} couldn't be loaded. The next course will be loaded instead.",
                        course.name
                    ));
                    self.show_intro(cx);
                } else {
                    cx.toast(&format!(
                        "{} couldn't be loaded and no other course is left.",
                        course.name
                    ));
                    self.make_menu();
                }
            }
        }
    }

    pub fn back_to_select(&mut self, cx: &mut Cx) {
        self.run.challenge.end();
        self.playing = None;
        cx.host.unload_course();
        self.make_menu();
    }

    fn ending_finished(&mut self, cx: &mut Cx) {
        let mut news = Vec::new();
        if let Some(r) = self.reward.take() {
            news.push(r);
        }
        if let Some(u) = self.run.unlocked {
            news.push(format!("{} is now open!", DIFFICULTIES[u]));
        }
        if !news.is_empty() {
            cx.toast(&news.join(" "));
        }
        self.back_to_select(cx);
    }

    /// The course screen closed (after its result card, a skip or Save & Quit).
    fn session_ended(&mut self, cx: &mut Cx) {
        let n = self.run.pending_next.take().unwrap_or(Next::Quit);
        self.playing = None;
        self.next(cx, n);
    }

    fn next(&mut self, cx: &mut Cx, n: Next) {
        self.run.persist(cx);
        match n {
            Next::Play => self.show_intro(cx),
            Next::GameOver => {
                if self.smm2() {
                    // the course already showed Play_GameOver_00: back to the menu
                    self.back_to_select(cx);
                    return;
                }
                cx.host.unload_course();
                self.scene = GameOverScene::new(&self.ui).map_or(Scene::None, Scene::GameOver);
            }
            Next::Complete => {
                self.run.unlocked = self.run.challenge.take_unlock().map(|d| d as usize);
                self.run.persist(cx);
                self.report_complete(cx);
                self.reward = award_costume(cx, self.run.difficulty);
                if self.smm2() {
                    // an endless run has no ending in SMM2: the menu, with the news [departure]
                    let mut msg = format!(
                        "{} run cleared with {} lives left!",
                        DIFFICULTIES[self.run.difficulty],
                        self.run.status().lives
                    );
                    if let Some(u) = self.run.unlocked {
                        msg += &format!(" {} is now open!", DIFFICULTIES[u]);
                    }
                    if let Some(r) = self.reward.take() {
                        msg += &format!(" {r}");
                    }
                    cx.toast(&msg);
                    self.back_to_select(cx);
                    return;
                }
                cx.host.unload_course();
                let lives = self.run.status().lives;
                self.scene =
                    EndingScene::new(&self.ui, self.run.difficulty, lives, &self.run.credits)
                        .map_or(Scene::None, Scene::Ending);
            }
            _ => self.back_to_select(cx),
        }
    }

    /// A completed run counts on the course server's 100 Mario leaderboard of its difficulty
    /// (`hundred_<d>`, completed runs) when signed in. The runs follow SMM1's rules on either
    /// screens, so they stay off SMM2's Endless boards.
    pub(crate) fn report_complete(&mut self, cx: &mut Cx) {
        if !cx.online.signed_in() {
            return;
        }
        let difficulty = ["easy", "normal", "expert", "super_expert"][self.run.difficulty.min(3)];
        cx.online.call(
            cx.host,
            "hundred",
            serde_json::json!({ "difficulty": difficulty }),
        );
    }

    /// Run a deferred step (after its fade covered the screen).
    pub fn go(&mut self, cx: &mut Cx, g: ChallengeGo) {
        match g {
            ChallengeGo::BackToSelect => self.back_to_select(cx),
            ChallengeGo::Play => self.play(cx),
        }
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<ChallengeEvent> {
        if let Some(p) = &mut self.playing {
            match p.step(input, cx, Some(&mut self.run)) {
                Some(PlayEvent::Exit) => self.session_ended(cx),
                Some(PlayEvent::Cleared) | None => {}
            }
            return None;
        }
        let ui: &UiInput = &input.ui;
        match &mut self.scene {
            Scene::Menu(s) => match s.step(ui, &self.run)? {
                EndlessEvent::Exit => {
                    self.run.challenge.end();
                    return Some(ChallengeEvent::Exit);
                }
                EndlessEvent::Locked(d) => cx.toast(endless::locked_message(d)),
                EndlessEvent::Start { difficulty, resume } => {
                    self.endless_handoff = s.board.take().map(|mut b| {
                        // the run's map: the mode's own name again
                        b.layout.text("T_Title_00", "100 Mario Challenge");
                        b
                    });
                    self.start(cx, difficulty, resume);
                }
                _ => {}
            },
            Scene::Map(s) => match s.step(ui)? {
                EndlessEvent::BackToSelect => {
                    cx.fade_to(FadeKind::Black, Go::Challenge(ChallengeGo::BackToSelect))
                }
                EndlessEvent::Play => cx.fade_to(FadeKind::Black, Go::Challenge(ChallengeGo::Play)),
                _ => {}
            },
            Scene::Smm1Menu(s) => match s.step(ui, &self.run)? {
                Smm1Event::Exit => {
                    self.run.challenge.end();
                    return Some(ChallengeEvent::Exit);
                }
                Smm1Event::Locked(d) => cx.toast(endless::locked_message(d)),
                Smm1Event::Start { difficulty, resume } => {
                    if let Scene::Smm1Menu(s) = std::mem::replace(&mut self.scene, Scene::None) {
                        self.handoff = s.into_handoff();
                    }
                    self.start(cx, difficulty, resume);
                }
                _ => {}
            },
            Scene::Smm1Intro(s) => match s.step(ui)? {
                Smm1Event::BackToSelect => self.back_to_select(cx),
                Smm1Event::Play => self.play(cx),
                _ => {}
            },
            Scene::GameOver(s) => {
                if s.step(ui).is_some() {
                    self.back_to_select(cx);
                }
            }
            Scene::Ending(s) => {
                if s.step(ui).is_some() {
                    self.ending_finished(cx);
                }
            }
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
            Scene::Smm1Menu(s) => s.draw(out),
            Scene::Smm1Intro(s) => s.draw(out),
            Scene::GameOver(s) => s.draw(out),
            Scene::Ending(s) => s.draw(out),
            Scene::None => {}
        }
    }

    // MARK: captures

    /// The select screen on difficulty `d`.
    pub fn debug_select(&mut self, d: usize) {
        self.run.difficulty = d.min(3);
        self.make_menu();
    }

    /// The map walk after clearing `cleared` courses (or a new run's map when 0 and `new_game`).
    pub fn debug_map(&mut self, d: usize, new_game: bool, cleared: usize) {
        self.run.difficulty = d.min(3);
        let total = RUN_LENGTHS[d.min(3)] as u32;
        let mut info = RunCardInfo {
            name: "Course".into(),
            maker: "Maker".into(),
            lives: 87,
            number: cleared as u32 + 1,
            total,
            code: None,
        };
        if let Some(e) = self.run.entries.values().next() {
            info.name = e.name.clone();
            info.maker = e.meta.maker.clone().unwrap_or_default();
        }
        if self.smm2() {
            let e = if new_game {
                MapEntry::NewGame
            } else {
                MapEntry::AfterClear
            };
            self.scene = EndlessMapScene::new(&self.ui, e, d, 87, cleared, Some(0), None)
                .map_or(Scene::None, Scene::Map);
        } else {
            let e = if new_game {
                Entry::NewGame
            } else {
                Entry::AfterClear
            };
            self.scene = MapIntroScene::new(&self.ui, e, d, &info, cleared, None)
                .map_or(Scene::None, Scene::Smm1Intro);
        }
    }

    pub fn debug_game_over(&mut self) {
        if !self.smm2() {
            self.scene = GameOverScene::new(&self.ui).map_or(Scene::None, Scene::GameOver);
        }
    }

    pub fn debug_ending(&mut self, d: usize, library: &[CourseEntry]) {
        if self.smm2() {
            return;
        }
        self.run.difficulty = d.min(3);
        self.run.credits = library
            .iter()
            .take(RUN_LENGTHS[d.min(3)])
            .map(|e| (e.name.clone(), e.meta.maker.clone().unwrap_or_default()))
            .collect();
        let lives = self.run.status().lives;
        self.scene = EndingScene::new(&self.ui, d, lives, &self.run.credits)
            .map_or(Scene::None, Scene::Ending);
    }

    /// The current course's play with its screens (captures).
    pub fn debug_play(&mut self, screen: &str, cx: &mut Cx) {
        if let Some(p) = &mut self.playing {
            p.debug(screen, cx, Some(&mut self.run));
        }
    }
}

/// The setting with the costumes won in 100 Mario runs (ids, comma-separated).
pub const OWNED_COSTUMES_KEY: &str = "costumes.owned";

/// SMM1's Mystery Mushroom reward for a finished 100 Mario run (`difficulty` 0–3): the costume
/// joins the owned set, and the words announcing it come back (None without the SMM1 import's
/// costume table).
fn award_costume(cx: &mut Cx, difficulty: usize) -> Option<String> {
    use mmk_modes::hundred::{Difficulty, RewardNote, costume_reward};
    let table = cx.host.costume_categories();
    if table.is_empty() {
        return None;
    }
    let mut owned: std::collections::BTreeSet<u16> = cx
        .host
        .load_value(OWNED_COSTUMES_KEY)
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let d = Difficulty::ALL[difficulty.min(3)];
    let roll = cx.host.random();
    let r = costume_reward(d, &table, &owned, roll);
    let mut words = Vec::new();
    if let Some(id) = r.costume {
        owned.insert(id);
        let list: Vec<String> = owned.iter().map(u16::to_string).collect();
        cx.host.store_value(OWNED_COSTUMES_KEY, &list.join(","));
        let name = cx
            .host
            .costumes()
            .into_iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| format!("costume {id}"), |(_, n)| n);
        words.push(format!("You got the {name} Mystery Mushroom costume!"));
    }
    match r.note {
        Some(RewardNote::AllOwned) => words.push("You have every costume this level gives.".into()),
        Some(RewardNote::TryAnotherLevel) => {
            words.push("No costumes left at this level: try another one.".into())
        }
        None => {}
    }
    (!words.is_empty()).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_run_wins_a_costume_once() {
        use crate::play::tests::{MockHost, with};
        let mut host = MockHost {
            costumes: vec![
                (0, "Mario".into(), 1),
                (7, "Peach".into(), 2),
                (9, "Bowser".into(), 4),
            ],
            ..Default::default()
        };
        let mut saves = crate::host::Saves::default();
        // Easy: Mario, the only category-1 costume; Normal still has Peach
        let words = with(&mut host, &mut saves, |cx| award_costume(cx, 0)).unwrap();
        assert!(words.contains("Mario"), "{words}");
        assert!(words.contains("try another"), "{words}");
        assert_eq!(host.values[OWNED_COSTUMES_KEY], "0");
        let words = with(&mut host, &mut saves, |cx| award_costume(cx, 1)).unwrap();
        assert!(
            words.contains("Peach") && words.contains("every costume"),
            "{words}"
        );
        assert_eq!(host.values[OWNED_COSTUMES_KEY], "0,7");
        // nothing left (Bowser is amiibo-only)
        let words = with(&mut host, &mut saves, |cx| award_costume(cx, 3)).unwrap();
        assert!(!words.contains("You got"), "{words}");
        // without the SMM1 import: no reward at all
        let mut bare = MockHost::default();
        assert_eq!(with(&mut bare, &mut saves, |cx| award_costume(cx, 3)), None);
    }
}
