//! The 100 Mario Challenge: difficulty select → map / intro → course → result, until the run is
//! cleared, lost or saved (the Swift app's `ChallengeFlow`). Rules, pools and save data are
//! `mmk_modes::challenge`; this sequences the screens — SMM2's Endless Challenge screens
//! (`endless`, the default) or SMM1's own (`smm1`) — and owns the course being played.

use std::collections::HashMap;

use mmk_modes::challenge::Challenge;
use mmk_modes::hundred::{Difficulty, Next, Outcome};
use mmk_modes::pool::CourseRef;

use crate::coop::{Asked, PlayersAsk};
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
    CardStart, ChallengeMenuScene, Entry, GameOverScene, MapIntroScene, RUN_LENGTHS, RunCardInfo,
    RunOverlay, RunQuery, Smm1Event, WorldMap,
};
use crate::smm1_ending::{CreditsReward, CreditsScene, RescueInfo, RescueScene};
use crate::smm1_fade::{Smm1Fade, Smm1FadeKind};
use crate::{Cx, DrawItem, Go};

pub const DIFFICULTIES: [&str; 4] = ["Easy", "Normal", "Expert", "Super Expert"];

/// The setting with, per difficulty, how many of the run's clears its world map has shown
/// (SMM1's "clear shown" save flag, 0x02084060 / 0x020844e0): a run saved at the course clear
/// shows Mario walking on when it is continued.
pub const CLEAR_SHOWN_KEY: &str = "hundred.clear_shown";

/// What the SMM1 screens do once their fade has covered the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterCover {
    /// Load the course (its intro card comes up under the reveal).
    Play,
    /// The course is over: the next screen of the run.
    Session,
    /// Back to the difficulty select.
    Select,
    /// The rescue scene is over: the credits.
    Credits,
    /// The ending is over: the select, with the news.
    EndingDone,
}

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

#[allow(clippy::large_enum_variant)] // one scene at a time
enum Scene {
    Menu(EndlessMenuScene),
    Map(EndlessMapScene),
    Smm1Menu(ChallengeMenuScene),
    Smm1Intro(MapIntroScene),
    GameOver(GameOverScene),
    Rescue(Box<RescueScene>),
    Credits(CreditsScene),
    None,
}

/// The run itself, apart from the screens (the course play talks to it as a [`RunLink`]).
pub struct RunState {
    pub challenge: Challenge,
    pub difficulty: usize,
    pub course: Option<CourseRef>,
    pending_next: Option<Next>,
    entry: Entry,
    /// How the next course's intro card starts.
    card: CardStart,
    /// Clears shown on each difficulty's map ([`CLEAR_SHOWN_KEY`]).
    clear_shown: [usize; 4],
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
    /// Map and title bar from the SMM1 select screen to the MapIn that follows, and whether
    /// the select showed a Continue.
    handoff: Option<(WorldMap, Layout, bool)>,
    /// SMM1: the scene fader and what follows its cover.
    fade: Option<Smm1Fade>,
    after_cover: Option<AfterCover>,
    /// SMM2: the Endless menu handed to the run's map (ChangeMap continues it).
    endless_handoff: Option<EndlessBoard>,
    /// Captures: start the course without the card (`MMK_UI_INTRO=0`).
    pub intro: bool,
    /// The SMM2 menu's header instead of "100 Mario Challenge" (Challenge Modes' ZL / ZR hint).
    pub header: Option<String>,
    /// The finished run's costume reward: shown at the credits' end (SMM1), announced (SMM2).
    reward: Option<Award>,
    /// Co-op: the run's players (one shared life counter), and the question before a run.
    pub players: u32,
    asking: Option<PlayersAsk>,
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

    fn clear_shown(&self, d: usize) -> bool {
        let n = self.challenge.save.records[d.min(3)]
            .as_ref()
            .map_or(0, |r| r.cleared.len());
        self.clear_shown[d.min(3)] >= n
    }
}

impl RunState {
    fn persist(&self, cx: &mut Cx) {
        cx.host.store_value("hundred", &self.challenge.save_json());
    }

    /// The map has shown difficulty `d`'s clears up to now.
    fn mark_clear_shown(&mut self, cx: &mut Cx, d: usize) {
        let n = self.challenge.save.records[d.min(3)]
            .as_ref()
            .map_or(0, |r| r.cleared.len());
        self.clear_shown[d.min(3)] = n;
        let list: Vec<String> = self.clear_shown.iter().map(usize::to_string).collect();
        cx.host.store_value(CLEAR_SHOWN_KEY, &list.join(","));
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

    fn course_counts(&self) -> (Option<i64>, Option<i64>) {
        let Some(e) = self.course.as_ref().and_then(|c| self.entries.get(&c.id)) else {
            return (None, None);
        };
        (e.meta.likes, e.meta.plays)
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
        let mut clear_shown = [0usize; 4];
        for (i, v) in cx
            .host
            .load_value(CLEAR_SHOWN_KEY)
            .unwrap_or_default()
            .split(',')
            .enumerate()
            .take(4)
        {
            clear_shown[i] = v.trim().parse().unwrap_or(0);
        }
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
        let fade = (presentation == Presentation::Smm1).then(|| Smm1Fade::new(&ui));
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
                card: CardStart::NewRun,
                clear_shown,
                unlocked: None,
                credits: Vec::new(),
                entries,
            },
            fade,
            after_cover: None,
            handoff: None,
            endless_handoff: None,
            intro: true,
            header: None,
            reward: None,
            players: 1,
            asking: None,
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

    /// New Game / Continue decided: with two or more controllers, ask how many play first.
    fn begin(&mut self, cx: &mut Cx, d: usize, resume: bool) {
        match PlayersAsk::open(cx, d, resume) {
            Some(a) => {
                cx.toast(&a.question());
                self.asking = Some(a);
            }
            None => {
                self.players = 1;
                self.start_from_menu(cx, d, resume);
            }
        }
    }

    /// Start the run from the select screen, handing its map over to the run's.
    fn start_from_menu(&mut self, cx: &mut Cx, d: usize, resume: bool) {
        match std::mem::replace(&mut self.scene, Scene::None) {
            Scene::Menu(mut s) => {
                self.endless_handoff = s.board.take().map(|mut b| {
                    // the run's map: the mode's own name again
                    b.layout.text("T_Title_00", "100 Mario Challenge");
                    b
                });
            }
            Scene::Smm1Menu(s) => self.handoff = s.into_handoff(&self.run),
            other => self.scene = other,
        }
        self.start(cx, d, resume);
    }

    pub fn start(&mut self, cx: &mut Cx, d: usize, resume: bool) {
        self.run.difficulty = d;
        self.run.unlocked = None;
        self.run.entry = if resume {
            Entry::Resume
        } else {
            Entry::NewGame
        };
        self.run.card = if resume {
            CardStart::Resume
        } else {
            CardStart::NewRun
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
            let entry = self.run.entry;
            self.run.entry = Entry::Direct;
            if entry == Entry::Direct {
                // after a skip the next course comes without the map [inf]
                self.handoff = None;
                self.run.card = CardStart::Next;
                self.play(cx);
                return;
            }
            if entry == Entry::AfterClear {
                self.run.card = CardStart::Next;
            }
            let lives = status.lives;
            let shown = self.run.clear_shown(d);
            let (handoff, had) = match self.handoff.take() {
                Some((m, t, had)) => (Some((m, t)), had),
                None => (None, false),
            };
            self.scene =
                MapIntroScene::new(&self.ui, entry, d, lives, cleared, shown, had, handoff)
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
        cx.host.set_players(self.players.max(1));
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
                let mut p = CoursePlay::new(
                    cx, title, key, screens, overlay, info, self.intro, false, true,
                );
                if self.intro {
                    p.start_card(&self.run.card_info(), self.run.card);
                }
                self.run.card = CardStart::Next;
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
        // the costume had its window at the credits' end
        self.reward = None;
        let mut news = Vec::new();
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
                        msg += &format!(" {}", r.words);
                    }
                    cx.toast(&msg);
                    self.back_to_select(cx);
                    return;
                }
                // the goal's form and costume go to the rescue scene (0x0277109c) [obs]
                let st = cx.host.course_status();
                cx.host.unload_course();
                let info = RescueInfo {
                    style: st.style,
                    difficulty: self.run.difficulty,
                    form: st.form,
                    costume: st.costume,
                };
                self.scene = Scene::Rescue(Box::new(RescueScene::new(&self.ui, info)));
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

    /// SMM1: cover the screen with `kind` and do `then` once covered (at once without the
    /// fade layout).
    fn cover(&mut self, cx: &mut Cx, kind: Smm1FadeKind, center: [f32; 2], then: AfterCover) {
        // the scene's music goes with it, over the cover [inf: the fade-out's length]
        let ticks = self.fade.as_mut().map_or(0, |f| f.cover_ticks(kind));
        self.ui.push_cues(vec![crate::sound::SoundCue::stop_music(
            ticks as f32,
            self.ui.kind(),
        )]);
        let started = self.fade.as_mut().is_some_and(|f| f.cover(kind, center));
        if started {
            self.after_cover = Some(then);
        } else {
            self.after_cover(cx, then);
        }
    }

    fn after_cover(&mut self, cx: &mut Cx, then: AfterCover) {
        match then {
            AfterCover::Play => self.play(cx),
            AfterCover::Session => self.session_ended(cx),
            AfterCover::Select => self.back_to_select(cx),
            AfterCover::Credits => {
                let reward = self.reward.as_ref().map(|a| a.reward.clone());
                self.scene = Scene::Credits(CreditsScene::new(
                    &self.ui,
                    self.run.difficulty,
                    &self.run.credits,
                    reward.unwrap_or_default(),
                ));
            }
            AfterCover::EndingDone => self.ending_finished(cx),
        }
    }

    /// SMM1: the course screen closed; the next screen comes behind a fade: blocks to the
    /// between-course map, every other scene request's type 1 (game over and the run's end
    /// come behind the fade the course already covered the screen with) [obs 0x0277117c].
    fn course_left(&mut self, cx: &mut Cx) {
        if self.playing.as_ref().is_some_and(|p| p.covered()) {
            if let Some(f) = &mut self.fade {
                f.hold_covered(Smm1FadeKind::SCENE);
            }
            self.session_ended(cx);
            return;
        }
        let kind = match self.run.pending_next {
            Some(Next::Play) if self.run.entry == Entry::AfterClear => Smm1FadeKind::TO_MAP,
            _ => Smm1FadeKind::SCENE,
        };
        self.cover(cx, kind, [0.0, 0.0], AfterCover::Session);
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<ChallengeEvent> {
        let mut faded_in = true;
        if let Some(f) = &mut self.fade {
            if f.step()
                && let Some(then) = self.after_cover.take()
            {
                self.after_cover(cx, then);
            }
            faded_in = self.fade.as_ref().is_none_or(|f| !f.active());
        }
        if self.after_cover.is_some() {
            // covering: the screen below waits
            return None;
        }
        if let Some(p) = &mut self.playing {
            match p.step(input, cx, Some(&mut self.run)) {
                Some(PlayEvent::Exit) if !self.smm2() => self.course_left(cx),
                Some(PlayEvent::Exit) => self.session_ended(cx),
                Some(PlayEvent::Cleared) | None => {}
            }
            return None;
        }
        let quiet = UiInput::default();
        let ui: &UiInput = if faded_in { &input.ui } else { &quiet };
        if let Some(a) = &mut self.asking {
            match a.step(ui, cx) {
                None => {}
                Some(Asked::Players(n)) => {
                    let (d, resume) = (a.difficulty, a.resume);
                    self.asking = None;
                    self.players = n;
                    self.start_from_menu(cx, d, resume);
                }
                Some(Asked::Cancel) => {
                    self.asking = None;
                    self.make_menu();
                }
            }
            return None;
        }
        match &mut self.scene {
            Scene::Menu(s) => match s.step(ui, &self.run)? {
                EndlessEvent::Exit => {
                    self.run.challenge.end();
                    return Some(ChallengeEvent::Exit);
                }
                EndlessEvent::Locked(d) => cx.toast(endless::locked_message(d)),
                EndlessEvent::Start { difficulty, resume } => self.begin(cx, difficulty, resume),
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
                    // leaving for Course World: the select's music fades with the screen [inf]
                    self.ui.push_cues(vec![crate::sound::SoundCue::stop_music(
                        30.0,
                        self.ui.kind(),
                    )]);
                    return Some(ChallengeEvent::Exit);
                }
                Smm1Event::Locked(d) => cx.toast(endless::locked_message(d)),
                Smm1Event::Start { difficulty, resume } => self.begin(cx, difficulty, resume),
                _ => {}
            },
            Scene::Smm1Intro(s) => {
                if let Some(Smm1Event::Play) = s.step(ui, faded_in) {
                    if s.clear_shown {
                        let d = self.run.difficulty;
                        self.run.mark_clear_shown(cx, d);
                    }
                    let (kind, center) = s.leave_fade();
                    self.cover(cx, kind, center, AfterCover::Play);
                }
            }
            Scene::GameOver(s) => {
                if s.step(ui).is_some() {
                    // GameOverScene → NetworkMainScene2 (0x02700d44) [obs]
                    self.cover(cx, Smm1FadeKind::SCENE, [0.0, 0.0], AfterCover::Select);
                }
            }
            Scene::Rescue(s) => {
                // NetEndingDemoScene → NetEndingScene (0x02768ba4), every request's fade [obs]
                if s.step() {
                    self.cover(cx, Smm1FadeKind::SCENE, [0.0, 0.0], AfterCover::Credits);
                }
            }
            Scene::Credits(s) => {
                // → NetworkMainScene2 (0x02767f18) [obs]
                if s.step(ui) {
                    self.cover(cx, Smm1FadeKind::SCENE, [0.0, 0.0], AfterCover::EndingDone);
                }
            }
            Scene::None => {}
        }
        None
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if let Some(p) = &mut self.playing {
            p.draw(out);
        } else {
            match &mut self.scene {
                Scene::Menu(s) => s.draw(out),
                Scene::Map(s) => s.draw(out),
                Scene::Smm1Menu(s) => s.draw(out),
                Scene::Smm1Intro(s) => s.draw(out),
                Scene::GameOver(s) => s.draw(out),
                Scene::Rescue(s) => s.draw(out),
                Scene::Credits(s) => s.draw(out),
                Scene::None => {}
            }
        }
        if let Some(f) = &mut self.fade {
            f.draw(out);
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
            self.scene = MapIntroScene::new(&self.ui, e, d, 87, cleared, false, false, None)
                .map_or(Scene::None, Scene::Smm1Intro);
        }
    }

    pub fn debug_game_over(&mut self) {
        if !self.smm2() {
            self.scene = GameOverScene::new(&self.ui).map_or(Scene::None, Scene::GameOver);
        }
    }

    /// The rescue scene of a run on difficulty `d` finished in a course of `style`.
    pub fn debug_ending(&mut self, d: usize, style: u32, library: &[CourseEntry]) {
        if self.smm2() {
            return;
        }
        self.run.difficulty = d.min(3);
        self.run.credits = library
            .iter()
            .take(RUN_LENGTHS[d.min(3)])
            .map(|e| (e.name.clone(), e.meta.maker.clone().unwrap_or_default()))
            .collect();
        let info = RescueInfo {
            style,
            difficulty: d.min(3),
            ..Default::default()
        };
        self.scene = Scene::Rescue(Box::new(RescueScene::new(&self.ui, info)));
    }

    /// The staff credits of a finished run on difficulty `d` (captures).
    pub fn debug_credits(&mut self, d: usize, library: &[CourseEntry]) {
        if self.smm2() {
            return;
        }
        self.debug_ending(d, 0, library);
        self.scene = Scene::Credits(CreditsScene::new(
            &self.ui,
            d.min(3),
            &self.run.credits,
            CreditsReward::default(),
        ));
    }

    /// The costume window after the credits, for costume `id` (captures): the credits' end
    /// with the list as owned so far plus it, and `note` after.
    pub fn debug_costume(
        &mut self,
        d: usize,
        id: u16,
        note: Option<mmk_modes::hundred::RewardNote>,
    ) {
        if self.smm2() {
            return;
        }
        let reward = CreditsReward {
            costume: Some(id),
            owned: [0, id].into(),
            note,
        };
        let mut s = CreditsScene::new(&self.ui, d.min(3), &[], reward);
        s.skip_to_reward();
        self.scene = Scene::Credits(s);
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

/// A finished run's reward: what the credits show (SMM1) and the words announcing it (SMM2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Award {
    pub reward: CreditsReward,
    pub words: String,
}

/// SMM1's Mystery Mushroom reward for a finished 100 Mario run (`difficulty` 0–3): the costume
/// joins the owned set (None without the SMM1 import's costume table).
fn award_costume(cx: &mut Cx, difficulty: usize) -> Option<Award> {
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
    Some(Award {
        reward: CreditsReward {
            costume: r.costume,
            owned,
            note: r.note,
        },
        words: words.join(" "),
    })
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
        let words = with(&mut host, &mut saves, |cx| award_costume(cx, 0))
            .unwrap()
            .words;
        assert!(words.contains("Mario"), "{words}");
        assert!(words.contains("try another"), "{words}");
        assert_eq!(host.values[OWNED_COSTUMES_KEY], "0");
        let a = with(&mut host, &mut saves, |cx| award_costume(cx, 1)).unwrap();
        assert_eq!(a.reward.costume, Some(7));
        assert_eq!(a.reward.owned, [0, 7].into());
        let words = a.words;
        assert!(
            words.contains("Peach") && words.contains("every costume"),
            "{words}"
        );
        assert_eq!(host.values[OWNED_COSTUMES_KEY], "0,7");
        // nothing left (Bowser is amiibo-only)
        let a = with(&mut host, &mut saves, |cx| award_costume(cx, 3)).unwrap();
        assert_eq!(a.reward.costume, None);
        assert!(a.reward.note.is_some());
        let words = a.words;
        assert!(!words.contains("You got"), "{words}");
        // without the SMM1 import: no reward at all
        let mut bare = MockHost::default();
        assert_eq!(with(&mut bare, &mut saves, |cx| award_costume(cx, 3)), None);
    }
}
