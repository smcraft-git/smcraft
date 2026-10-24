//! The Super World Toad House minigames (private spec `research/specs/super-worlds.md` §6), on
//! the game's own layouts over the map:
//! - MATCH & WIN `MyWEdit_Minigame_00`: three picture strips spin (top left, centre right,
//!   bottom left); A stops them top to bottom; three of a kind wins 5 (Star), 3 (Flower) or 1
//!   (Mushroom) lives;
//! - POP & WIN `MyWEdit_Minigame_01`: A pumps; the balloon grows at 4, 10, 18 and 28 pumps and
//!   bursts at 50 into 1, 3 or 5 lives (4 / 4 / 3 chances in 11);
//! - CATCH & WIN `MyWEdit_Minigame_02`: ten pitches (Fastball, Changeup, Knuckleball, Forkball,
//!   Heater); A jumps; the ball is caught when it meets the character's box (the game's test on
//!   the animated panes); three misses lose, else 5 / 3 / 1 lives for 0 / 1 / 2 misses.
//!
//! The rules are `mmk_modes::minigame`'s; the timings follow the game's sequences (each step
//! waits for its animation). Play Together: two to four players — Mario, Luigi, Toad, Toadette
//! (`TypeMember`) — each with their own A [obs v3.0.3 0x710197eb10: a press sets its player's
//! bit]: any of them stops Match & Win's next strip, each pumps with their own animation, each
//! jumps for Catch & Win's ball and catches it with their own box.

use mmk_modes::Rng;
use mmk_modes::minigame::{
    self as mg, BALLS, Bounds, Kind, MISS_LIMIT, Pitch, STRIP_START_GAPS, Strip,
};
use mmk_ui::Quad;

use crate::DrawItem;
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, Script, UiInput};

/// The characters' names in the layouts' animations and panes (0 Mario, 1 Luigi, 2 Toad,
/// 3 Toadette).
const CHARACTERS: [&str; 4] = ["Mario", "Luigi", "Kinopio", "Kinopico"];
/// The strips' pane suffixes, top to bottom.
const STRIPS: [&str; 3] = ["T", "C", "B"];
/// Match & Win's window over the strips.
const WINDOW: &str = "N_Scissor_00";

/// What a frame of the minigame told its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinigameStep {
    Running,
    /// Over: the lives won (0 for a loss).
    Done(u32),
}

/// Where a game stands (shared by the three).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The layout's In (and Match & Win's strips starting).
    Appear,
    /// Taking input.
    Play,
    /// The result's animation (Success / Fail).
    Result,
    /// The layout's Out.
    Leave,
}

struct MatchGame {
    strips: [Strip; 3],
    /// Strips started so far, and the tick the next one starts.
    started: usize,
    next_start: Option<i32>,
    /// Strips stopped by a press.
    stopped: usize,
}

struct PopGame {
    pumps: u32,
    /// `PumpKH_0n` stages played.
    stages: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pitching {
    /// `PitchingStart`.
    Windup,
    /// The Changeup's `PitchingAngle` up to its release frame.
    Angle,
    /// The ball is on its way.
    Flight,
    /// Caught (or missed): the catcher lands.
    Landing,
    /// `InPitch` (the pitch's name) and the miss marks.
    Banner,
    /// `OutPitch` before the next one.
    Next,
}

struct CatchGame {
    order: [Pitch; BALLS],
    ball: usize,
    misses: u32,
    step: Pitching,
    caught: bool,
    /// The ball's translate in its track (`N_BallStart_00`) and the track's origin on screen
    /// and turn (degrees).
    pos: [f32; 2],
    origin: [f32; 2],
    angle: f32,
    /// Caught: where the ball sits from the catcher's box centre.
    held: Option<[f32; 2]>,
    /// The player who caught it, and the one the Forkball drops at.
    catcher: usize,
    target: usize,
}

enum Game {
    Match(MatchGame),
    Pop(PopGame),
    Catch(CatchGame),
}

pub struct MinigameScene {
    ui: Ui,
    pub kind: Kind,
    layout: Layout,
    name: &'static str,
    tick: i32,
    phase: Phase,
    /// Tick the current wait started.
    at: i32,
    character: usize,
    /// Play Together's players (1: the player alone, as `character`).
    players: usize,
    rng: Rng,
    game: Game,
    /// Lives won (paid when the result shows).
    pub prize: Option<u32>,
}

impl MinigameScene {
    /// `character`: the player's (0 Mario … 3 Toadette); `players`: Play Together's (2–4: the
    /// four characters in order, `TypeMember` frame players − 2; else `TypeCharacter`
    /// [obs v3.0.3 0x710197aee0]); `seed` for the random parts.
    pub fn new(ui: &Ui, kind: Kind, character: usize, players: usize, seed: u64) -> Option<Self> {
        let name = kind.layout();
        let mut l = ui.layout(name)?;
        let character = character.min(3);
        let players = players.clamp(1, CHARACTERS.len());
        l.play("In", 0);
        if players > 1 {
            l.hold("TypeMember", (players - 2) as f32);
        } else {
            l.hold("TypeCharacter", character as f32);
        }
        let mut rng = Rng::new(seed);
        let game = match kind {
            Kind::Match => Game::Match(MatchGame {
                strips: [Strip::new(0), Strip::new(1), Strip::new(2)],
                started: 0,
                next_start: None,
                stopped: 0,
            }),
            Kind::Pop => {
                l.hold("PumpKH_04", 0.0);
                Game::Pop(PopGame {
                    pumps: 0,
                    stages: 0,
                })
            }
            Kind::Catch => {
                let order = mg::pitch_order(&mut rng);
                // the ball waits out of sight until the first pitch
                l.hold_end("OutBallMiss");
                Game::Catch(CatchGame {
                    order,
                    ball: 0,
                    misses: 0,
                    step: Pitching::Windup,
                    caught: false,
                    pos: [0.0, 0.0],
                    origin: [0.0, 0.0],
                    angle: 0.0,
                    held: None,
                    catcher: 0,
                    target: 0,
                })
            }
        };
        ui.sound_key(name, "In", &["In_bgm", "start_se"]);
        let mut s = Self {
            ui: ui.clone(),
            kind,
            layout: l,
            name,
            tick: 0,
            phase: Phase::Appear,
            at: 0,
            character,
            players,
            rng,
            game,
            prize: None,
        };
        s.place_strips();
        s.ball_count();
        Some(s)
    }

    /// Player `p`'s character: the player's own alone, else Mario, Luigi, Toad, Toadette.
    fn chara_of(&self, p: usize) -> &'static str {
        if self.players > 1 {
            CHARACTERS[p.min(CHARACTERS.len() - 1)]
        } else {
            CHARACTERS[self.character]
        }
    }

    fn sound(&self, key: &str, context: &[&str]) {
        self.ui.sound_key(self.name, key, context);
    }

    fn play(&mut self, anim: &str) {
        self.layout.play(anim, self.tick);
    }

    fn done(&mut self, anim: &str) -> bool {
        !self.layout.has(anim) || self.layout.finished(anim, self.tick)
    }

    /// Captures and tests: the game so far.
    pub fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Appear => "appear",
            Phase::Play => "play",
            Phase::Result => "result",
            Phase::Leave => "leave",
        }
    }

    /// Captures and tests: Match & Win's strips, Pop & Win's pumps, Catch & Win's balls thrown
    /// and misses.
    pub fn state(&self) -> String {
        match &self.game {
            Game::Match(m) => {
                let p: Vec<String> = m
                    .strips
                    .iter()
                    .map(|s| format!("{:?}", s.picture()))
                    .collect();
                format!("strips {} stopped {}", p.join(" "), m.stopped)
            }
            Game::Pop(p) => format!("pumps {}", p.pumps),
            Game::Catch(c) => format!("ball {} misses {}", c.ball, c.misses),
        }
    }

    /// A frame with one player's input (A is player 1's).
    pub fn step(&mut self, input: &UiInput, script: &mut Script) -> MinigameStep {
        let pressed = input.pressed | script.step();
        self.step_players(pressed.has(Buttons::CONFIRM) as u8)
    }

    /// Play Together's players in this game (1 alone).
    pub fn players(&self) -> usize {
        self.players
    }

    /// A frame: bit p of `presses` is player p's A this frame.
    pub fn step_players(&mut self, presses: u8) -> MinigameStep {
        self.tick += 1;
        let presses = presses & ((1u8 << self.players) - 1);
        let a = presses != 0;
        match self.phase {
            Phase::Appear => self.appear(),
            Phase::Play => match self.kind {
                Kind::Match => self.match_play(a),
                Kind::Pop => self.pop_play(presses),
                Kind::Catch => self.catch_play(a),
            },
            Phase::Result => {
                let anim = if self.prize.unwrap_or(0) > 0 {
                    "Success"
                } else {
                    "Fail"
                };
                if self.done(anim) && self.tick - self.at >= 30 {
                    self.phase = Phase::Leave;
                    self.at = self.tick;
                    self.play("Out");
                }
            }
            Phase::Leave => {
                if self.done("Out") {
                    return MinigameStep::Done(self.prize.unwrap_or(0));
                }
            }
        }
        if let Game::Match(m) = &mut self.game {
            let mut settled = false;
            for s in &mut m.strips {
                settled |= s.step();
            }
            if settled {
                self.sound("cScrollStop", &[]);
            }
        }
        if let Game::Catch(_) = self.game
            && matches!(self.phase, Phase::Play | Phase::Result)
        {
            for p in (0..self.players).filter(|p| presses & (1 << p) != 0) {
                self.jump(p);
            }
        }
        self.place_strips();
        MinigameStep::Running
    }

    fn appear(&mut self) {
        if !self.done("In") {
            return;
        }
        let tick = self.tick;
        let Game::Match(m) = &mut self.game else {
            return self.start_play();
        };
        // the strips start one by one after the In, then the Stop guide
        let next = *m.next_start.get_or_insert(tick + STRIP_START_GAPS[0]);
        if m.started < 3 && tick >= next {
            m.strips[m.started].start();
            m.started += 1;
            m.next_start = STRIP_START_GAPS.get(m.started).map(|g| tick + g);
            if m.started == 1 {
                self.sound("cScrolling", &[]);
            }
        } else if m.started == 3 {
            self.start_play();
        }
    }

    fn start_play(&mut self) {
        self.phase = Phase::Play;
        self.at = self.tick;
        self.play("InGuide");
        if let Game::Catch(_) = self.game {
            self.windup();
        }
    }

    fn result(&mut self, lives: Option<u32>) {
        self.phase = Phase::Result;
        self.at = self.tick;
        self.prize = Some(lives.unwrap_or(0));
        match lives {
            Some(n) => {
                self.layout.hold("TypeSuccess", mg::success_frame(n));
                self.play("Success");
                let ctx = format!("Success_{n}up");
                self.sound("Anm_Success", &[&ctx]);
            }
            None => {
                self.play("Fail");
                self.sound("Anm_Fail", &[]);
            }
        }
        if self.kind == Kind::Catch {
            self.play("OutGuide");
        }
    }

    // MARK: Match & Win

    fn match_play(&mut self, a: bool) {
        let Game::Match(m) = &mut self.game else {
            return;
        };
        if a && m.stopped < 3 {
            m.strips[m.stopped].stop();
            m.stopped += 1;
            self.sound("KI_Decide_Abtn", &[]);
            if let Game::Match(m) = &self.game
                && m.stopped == 3
            {
                self.play("OutGuide");
            }
            return;
        }
        let all_rest = m.stopped == 3 && m.strips.iter().all(Strip::resting);
        let pictures = m.strips.map(|s| s.picture());
        if all_rest && self.done("OutGuide") {
            let lives = mg::match_prize(pictures);
            if lives.is_none() {
                for (k, p) in STRIPS.iter().zip(pictures) {
                    self.layout.hold(&format!("Fail{k}"), p.fail_frame());
                }
            }
            self.result(lives);
        }
    }

    /// The strips' pictures where their offsets put them.
    fn place_strips(&mut self) {
        let Game::Match(m) = &self.game else {
            return;
        };
        for (i, strip) in m.strips.iter().enumerate() {
            for (k, p) in mg::STRIP.iter().enumerate() {
                let pane = format!("P_{}{}_00", p.stem(), STRIPS[i]);
                self.layout.set_translate(&pane, [strip.slot_x(k), 0.0]);
            }
        }
    }

    // MARK: Pop & Win

    /// Every player's A is a pump, with their own animation; the pumps add up [obs
    /// 0x710197fca0].
    fn pop_play(&mut self, presses: u8) {
        for k in (0..self.players).filter(|k| presses & (1 << k) != 0) {
            if let Game::Pop(p) = &mut self.game {
                p.pumps += 1;
            }
            let anim = format!("{}Pump", self.chara_of(k));
            self.play(&anim);
            self.sound(&format!("Anm_{anim}"), &[]);
        }
        let Game::Pop(p) = &mut self.game else {
            return;
        };
        if p.pumps >= mg::PUMPS_TO_POP {
            let lives = mg::pop_draw(&mut self.rng);
            self.play("TypeExtra");
            self.play("OutGuide");
            self.result(Some(lives));
            return;
        }
        // each stage waits for the one before it
        let (stages, pumps) = (p.stages, p.pumps);
        let due = mg::pump_stage(pumps);
        let prev_done = stages == 0 || self.done(&format!("PumpKH_{:02}", stages - 1));
        if due > stages && prev_done {
            let anim = format!("PumpKH_{stages:02}");
            self.play(&anim);
            self.sound(&format!("Anm_{anim}"), &[]);
            if let Game::Pop(p) = &mut self.game {
                p.stages += 1;
            }
        } else if stages == 4 && prev_done {
            let len = self.layout.length("PumpKH_04").max(0.0);
            self.layout.hold("PumpKH_04", mg::growth(pumps) * len);
        }
    }

    // MARK: Catch & Win

    fn pitch(&self) -> Pitch {
        match &self.game {
            Game::Catch(c) => c.order[c.ball.min(BALLS - 1)],
            _ => Pitch::Fastball,
        }
    }

    fn set_step(&mut self, step: Pitching) {
        if let Game::Catch(c) = &mut self.game {
            c.step = step;
        }
        self.at = self.tick;
    }

    fn ball_count(&mut self) {
        if let Game::Catch(c) = &self.game {
            let left = (BALLS - c.ball.min(BALLS)).to_string();
            self.layout.text("T_BallNum_00", &left);
        }
    }

    fn windup(&mut self) {
        let p = self.pitch();
        self.ball_count();
        self.layout.set_translate("N_ProgramBall_00", [0.0, 0.0]);
        self.layout.clear_pane("N_BallStart_00");
        self.layout.stop("OutBallCatch");
        self.layout.stop("OutBallMiss");
        self.layout.stop("Pitching_00");
        self.layout.stop("Pitching_01");
        self.layout.stop("PitchingAngle");
        let start = if p.second_set() {
            "PitchingStart_01"
        } else {
            "PitchingStart_00"
        };
        self.play(start);
        // the Forkball's mark: a player at random in Play Together [obs 0x7101981f30: +0x37c,
        // rand32 × players / 2³² per pitch]
        let target = if self.players > 1 {
            ((self.rng.next_u32() as u64 * self.players as u64) >> 32) as usize
        } else {
            0
        };
        if let Game::Catch(c) = &mut self.game {
            c.caught = false;
            c.pos = [0.0, 0.0];
            c.angle = 0.0;
            c.held = None;
            c.catcher = 0;
            c.target = target;
        }
        self.set_step(Pitching::Windup);
    }

    fn pitching(p: Pitch) -> &'static str {
        if p.second_set() {
            "Pitching_01"
        } else {
            "Pitching_00"
        }
    }

    fn throw(&mut self) {
        let p = self.pitch();
        let anim = Self::pitching(p);
        self.layout
            .play_ex(anim, self.tick, 0.0, None, false, p.pitching_rate());
        self.layout.play_ex(
            "RotationBall",
            self.tick,
            0.0,
            None,
            true,
            p.rotation_rate(),
        );
        let ctx = format!("Anm_{anim}_{}", p.sound_context());
        self.sound(&format!("Anm_{anim}"), &[&ctx]);
        // the track's origin: where the ball sits before it moves
        let origin = self.rect("N_ProgramBall_00").map(|b| b.center());
        if let Game::Catch(c) = &mut self.game {
            c.origin = origin.unwrap_or_default();
        }
        self.set_step(Pitching::Flight);
    }

    /// Put the ball (its centre) at `at` on screen.
    fn place_ball(&mut self, at: [f32; 2]) {
        let Game::Catch(c) = &mut self.game else {
            return;
        };
        let (sin, cos) = (-c.angle).to_radians().sin_cos();
        let d = [at[0] - c.origin[0], at[1] - c.origin[1]];
        c.pos = [cos * d[0] - sin * d[1], sin * d[0] + cos * d[1]];
        let pos = c.pos;
        self.layout.set_translate("N_ProgramBall_00", pos);
    }

    /// Caught: the ball stays in the catcher's hands as he lands.
    fn hold_ball(&mut self) {
        let Game::Catch(c) = &self.game else {
            return;
        };
        let Some(off) = c.held else {
            return;
        };
        let body = format!("B_{}_00", self.chara_of(c.catcher));
        if let Some(b) = self.rect(&body) {
            let at = b.center();
            self.place_ball([at[0] + off[0], at[1] + off[1]]);
        }
    }

    fn rect(&mut self, pane: &str) -> Option<Bounds> {
        self.layout.apply(self.tick);
        let r = self.layout.rect(pane)?;
        Some(Bounds::around(r.center, r.size))
    }

    fn catch_play(&mut self, _a: bool) {
        let Game::Catch(c) = &self.game else {
            return;
        };
        let (step, misses, ball) = (c.step, c.misses, c.ball);
        let p = self.pitch();
        match step {
            Pitching::Windup => {
                let start = if p.second_set() {
                    "PitchingStart_01"
                } else {
                    "PitchingStart_00"
                };
                if !self.done(start) {
                    return;
                }
                if p.release_angle().is_some() {
                    self.play("PitchingAngle");
                    self.set_step(Pitching::Angle);
                } else {
                    self.throw();
                }
            }
            Pitching::Angle => {
                let release = p.release_angle().unwrap_or(0.0);
                let frame = (self.tick - self.at) as f32;
                if frame >= release || self.done("PitchingAngle") {
                    // the ball's track turned by the angle reached
                    self.layout.hold("PitchingAngle", frame.min(release));
                    let o = mmk_ui::PaneOverride {
                        rotate: Some(frame.min(release)),
                        ..Default::default()
                    };
                    self.layout.set_pane("N_BallStart_00", o);
                    if let Game::Catch(c) = &mut self.game {
                        c.angle = frame.min(release);
                    }
                    self.throw();
                }
            }
            Pitching::Flight => self.flight(p),
            Pitching::Landing => {
                self.hold_ball();
                let catcher = match &self.game {
                    Game::Catch(c) => c.catcher,
                    _ => 0,
                };
                let jump = format!("{}Jump", self.chara_of(catcher));
                if !self.layout.is_playing(&jump) || self.done(&jump) {
                    self.banner(p);
                }
            }
            Pitching::Banner => {
                self.hold_ball();
                if !self.done("InPitch") {
                    return;
                }
                if misses >= MISS_LIMIT {
                    self.result(None);
                } else if ball >= BALLS {
                    self.result(mg::catch_prize(misses));
                } else {
                    self.play("OutPitch");
                    self.set_step(Pitching::Next);
                }
            }
            Pitching::Next => {
                if self.done("OutPitch") {
                    self.windup();
                }
            }
        }
    }

    /// The ball in the air: into the house or off the screen is a miss, the character's box a
    /// catch; then it flies on a frame.
    fn flight(&mut self, p: Pitch) {
        let Some(ball) = self.rect("N_ProgramBall_00") else {
            return self.ball_out(false, None);
        };
        let center = ball.center();
        if let Some(h) = self.rect("P_KinokoHouse_00")
            && mg::reached_house(center, &h)
        {
            return self.ball_out(false, None);
        }
        // each player's box in turn: Mario, Luigi, Toad, Toadette [obs 0x71019851b0]
        for p in 0..self.players {
            let body = self.rect(&format!("B_{}_00", self.chara_of(p)));
            if let Some(b) = body
                && mg::caught(center, &b)
            {
                if let Game::Catch(c) = &mut self.game {
                    c.catcher = p;
                }
                let at = b.center();
                return self.ball_out(true, Some([center[0] - at[0], center[1] - at[1]]));
            }
        }
        let target = match &self.game {
            Game::Catch(c) => c.target,
            _ => 0,
        };
        let body = self.rect(&format!("B_{}_00", self.chara_of(target)));
        let off = ball.max[0] < -640.0
            || ball.min[0] > 640.0
            || ball.max[1] < -360.0
            || ball.min[1] > 360.0;
        if off || self.tick - self.at > 600 {
            return self.ball_out(false, None);
        }
        let near = body.is_some_and(|b| mg::fork_near(center[0], b.max[0]));
        if let Game::Catch(c) = &mut self.game {
            c.pos = mg::ball_step(p, c.pos, near);
            let pos = c.pos;
            self.layout.set_translate("N_ProgramBall_00", pos);
        }
    }

    fn ball_out(&mut self, caught: bool, held: Option<[f32; 2]>) {
        // the ball stops spinning where it is
        let len = self.layout.length("RotationBall").max(1.0);
        let spun = ((self.tick - self.at) as f32 * self.pitch().rotation_rate()) % len;
        self.layout.hold("RotationBall", spun);
        let p = self.pitch();
        let pitching = Self::pitching(p);
        let frame = ((self.tick - self.at) as f32 * p.pitching_rate())
            .min(self.layout.length(pitching).max(0.0));
        self.layout.hold(pitching, frame);
        if caught {
            self.play("OutBallCatch");
            self.sound("Anm_OutBallCatch", &[]);
        } else {
            self.play("OutBallMiss");
        }
        if let Game::Catch(c) = &mut self.game {
            c.caught = caught;
            c.held = held;
        }
        self.set_step(Pitching::Landing);
    }

    fn banner(&mut self, p: Pitch) {
        let k = p.index();
        if let Some(t) = self.layout.message(&format!("T_PitchType_{k:02}")) {
            self.layout.text("T_Pitch_00", &t);
        }
        if let Some(t) = self.layout.message(&format!("T_PitchSpeedType_{k:02}")) {
            self.layout.text("T_PitchSpeed_00", &t);
        }
        self.play("InPitch");
        self.sound("Anm_InPitch", &[]);
        let Game::Catch(c) = &mut self.game else {
            return;
        };
        if !c.caught {
            c.misses += 1;
            let anim = format!("{}Miss", c.misses.min(MISS_LIMIT));
            self.play(&anim);
            self.sound(&format!("Anm_{anim}"), &[]);
        }
        if let Game::Catch(c) = &mut self.game {
            c.ball += 1;
        }
        self.ball_count();
        self.set_step(Pitching::Banner);
    }

    /// Player `p`'s A: their character jumps (not again while in the air) [obs 0x7101981f30:
    /// the input bits 0–3, the Jump animations in player order].
    fn jump(&mut self, p: usize) {
        let anim = format!("{}Jump", self.chara_of(p));
        if self.layout.is_playing(&anim) && !self.done(&anim) {
            return;
        }
        self.play(&anim);
        self.sound(&format!("Anm_{anim}"), &[]);
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let mut quads = Vec::new();
        self.layout.draw(self.tick, &mut quads);
        if let Game::Match(_) = &self.game
            && let Some(window) = self.layout.rect("N_Scissor_00")
        {
            // the window (`N_Scissor_00`) scissors what it holds (the strips scroll behind it):
            // its quads are the ones a draw without it lacks
            let mut without = Vec::new();
            self.layout.show(WINDOW, false);
            self.layout.draw(self.tick, &mut without);
            self.layout.visible(WINDOW, None);
            let n = quads.len().saturating_sub(without.len());
            let first = (0..without.len())
                .find(|&i| quads[i].pos != without[i].pos || quads[i].tex0 != without[i].tex0)
                .unwrap_or(without.len());
            let lo = [
                window.center[0] - window.size[0] / 2.0,
                window.center[1] - window.size[1] / 2.0,
            ];
            let hi = [
                window.center[0] + window.size[0] / 2.0,
                window.center[1] + window.size[1] / 2.0,
            ];
            quads = quads
                .into_iter()
                .enumerate()
                .filter_map(|(i, q)| {
                    if (first..first + n).contains(&i) {
                        clip(q, 0, lo[0], hi[0]).and_then(|q| clip(q, 1, lo[1], hi[1]))
                    } else {
                        Some(q)
                    }
                })
                .collect();
        }
        out.push(DrawItem::Layout {
            ui: self.ui.kind(),
            size: self.layout.size(),
            quads,
        });
    }
}

/// Cut an axis-aligned quad to `lo..hi` along `axis` (0 x, 1 y), its texture coordinates and
/// colours with it; None when nothing is left.
fn clip(mut q: Quad, axis: usize, lo: f32, hi: f32) -> Option<Quad> {
    // the corners' pairs along the axis: (TL, TR), (BL, BR) across, (TL, BL), (TR, BR) down
    let pairs = if axis == 0 {
        [(0, 1), (2, 3)]
    } else {
        [(2, 0), (3, 1)]
    };
    let (a0, b0) = (q.pos[pairs[0].0][axis], q.pos[pairs[0].1][axis]);
    if a0.max(b0) <= lo || a0.min(b0) >= hi {
        return None;
    }
    let lerp2 =
        |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let lerp4 = |a: [u8; 4], b: [u8; 4], t: f32| {
        std::array::from_fn(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t).round() as u8)
    };
    for (l, r) in pairs {
        let (pl, pr) = (q.pos[l], q.pos[r]);
        let span = pr[axis] - pl[axis];
        if span == 0.0 {
            continue;
        }
        // the ends' shares along the edge, kept in order
        let t = |v: f32| ((v - pl[axis]) / span).clamp(0.0, 1.0);
        let (tl, tr) = if span > 0.0 {
            (t(lo), t(hi))
        } else {
            (t(hi), t(lo))
        };
        let (u0, u1, c) = (
            (q.uv0[l], q.uv0[r]),
            (q.uv1[l], q.uv1[r]),
            (q.color[l], q.color[r]),
        );
        q.pos[l] = lerp2(pl, pr, tl);
        q.pos[r] = lerp2(pl, pr, tr);
        q.uv0[l] = lerp2(u0.0, u0.1, tl);
        q.uv0[r] = lerp2(u0.0, u0.1, tr);
        q.uv1[l] = lerp2(u1.0, u1.1, tl);
        q.uv1[r] = lerp2(u1.0, u1.1, tr);
        q.color[l] = lerp4(c.0, c.1, tl);
        q.color[r] = lerp4(c.0, c.1, tr);
    }
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(kind: Kind) -> Option<MinigameScene> {
        let ui = crate::testing::smm2()?;
        MinigameScene::new(&ui, kind, 0, 1, 1)
    }

    fn run(s: &mut MinigameScene, frames: usize, press: impl Fn(i32) -> bool) -> Option<u32> {
        let mut script = Script::default();
        for _ in 0..frames {
            let input = UiInput {
                pressed: if press(s.tick + 1) {
                    Buttons::CONFIRM
                } else {
                    Buttons::NONE
                },
                ..Default::default()
            };
            if let MinigameStep::Done(n) = s.step(&input, &mut script) {
                return Some(n);
            }
        }
        None
    }

    #[test]
    fn clipping_cuts_the_texture_too() {
        let mut q = Quad {
            pos: [
                [-100.0, 10.0],
                [100.0, 10.0],
                [-100.0, -10.0],
                [100.0, -10.0],
            ],
            uv0: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            ..Default::default()
        };
        q = clip(q, 0, 0.0, 50.0).unwrap();
        assert_eq!(q.pos[0], [0.0, 10.0]);
        assert_eq!(q.pos[1], [50.0, 10.0]);
        assert_eq!(q.uv0[0], [0.5, 0.0]);
        assert_eq!(q.uv0[3], [0.75, 1.0]);
        assert!(clip(q, 0, 60.0, 80.0).is_none());
        // down: the top half
        let h = clip(q, 1, 0.0, 20.0).unwrap();
        assert_eq!((h.pos[2], h.uv0[2]), ([0.0, 0.0], [0.5, 0.5]));
        assert_eq!((h.pos[0], h.uv0[0]), ([0.0, 10.0], [0.5, 0.0]));
    }

    #[test]
    fn match_and_win_stops_three_strips_and_pays() {
        let Some(mut s) = scene(Kind::Match) else {
            return;
        };
        // nothing until the strips are going; then three presses
        assert_eq!(run(&mut s, 400, |_| false), None);
        assert_eq!(s.phase_name(), "play");
        let start = s.tick;
        let n = run(&mut s, 3000, |t| {
            [start + 10, start + 25, start + 40].contains(&t)
        });
        let Game::Match(m) = &s.game else {
            unreachable!()
        };
        let pictures = m.strips.map(|x| x.picture());
        assert_eq!(n, Some(mg::match_prize(pictures).unwrap_or(0)));
        assert!(m.strips.iter().all(Strip::resting));
        let mut out = Vec::new();
        s.draw(&mut out);
        assert!(!out.is_empty());
    }

    #[test]
    fn pop_and_win_bursts_at_fifty_pumps() {
        let Some(mut s) = scene(Kind::Pop) else {
            return;
        };
        assert_eq!(run(&mut s, 300, |_| false), None);
        assert_eq!(s.phase_name(), "play");
        // waiting does nothing (no time limit)
        assert_eq!(run(&mut s, 600, |_| false), None);
        assert_eq!(s.state(), "pumps 0");
        let n = run(&mut s, 3000, |t| t % 6 == 0).unwrap();
        assert!([1, 3, 5].contains(&n));
        assert_eq!(s.state(), "pumps 50");
    }

    /// A player who jumps when the ball is `lead` frames from the character (not at Forkballs,
    /// which drop into the glove).
    fn catcher(s: &mut MinigameScene, lead: f32) -> Option<u32> {
        let mut script = Script::default();
        let mut last: Option<f32> = None;
        for _ in 0..12000 {
            let mut press = false;
            if let Game::Catch(c) = &s.game
                && c.step == Pitching::Flight
                && s.pitch() != Pitch::Forkball
            {
                let ball = s.rect("N_ProgramBall_00").map(|b| b.center()[0]);
                let body = s.rect("B_Mario_00").map(|b| b.max[0]);
                if let (Some(x), Some(right), Some(prev)) = (ball, body, last) {
                    let v = (prev - x).max(1.0);
                    press = x - right <= v * lead && x > right;
                }
                last = ball;
            } else {
                last = None;
            }
            let input = UiInput {
                pressed: if press {
                    Buttons::CONFIRM
                } else {
                    Buttons::NONE
                },
                ..Default::default()
            };
            if let MinigameStep::Done(n) = s.step(&input, &mut script) {
                return Some(n);
            }
        }
        None
    }

    #[test]
    fn play_together_pumps_and_catches_with_every_player() {
        let Some(ui) = crate::testing::smm2() else {
            return;
        };
        // Pop & Win for three: two players' presses in a frame are two pumps
        let mut s = MinigameScene::new(&ui, Kind::Pop, 0, 3, 1).unwrap();
        for _ in 0..300 {
            s.step_players(0);
        }
        assert_eq!(s.phase_name(), "play");
        s.step_players(0b011);
        assert_eq!(s.state(), "pumps 2");
        // player 4 isn't playing: their bit counts for nothing
        s.step_players(0b1000);
        assert_eq!(s.state(), "pumps 2");
        assert_eq!(s.chara_of(1), "Luigi");
        // Catch & Win for two: only Luigi jumps, and his box catches
        let mut s = MinigameScene::new(&ui, Kind::Catch, 0, 2, 7).unwrap();
        let mut last: Option<f32> = None;
        let mut won = None;
        for _ in 0..12000 {
            let mut press = 0u8;
            if let Game::Catch(c) = &s.game
                && c.step == Pitching::Flight
                && s.pitch() != Pitch::Forkball
            {
                let ball = s.rect("N_ProgramBall_00").map(|b| b.center()[0]);
                let body = s.rect("B_Luigi_00").map(|b| b.max[0]);
                if let (Some(x), Some(right), Some(prev)) = (ball, body, last) {
                    let v = (prev - x).max(1.0);
                    if x - right <= v * 5.0 && x > right {
                        press = 0b10;
                    }
                }
                last = ball;
            } else {
                last = None;
            }
            if let MinigameStep::Done(n) = s.step_players(press) {
                won = Some(n);
                break;
            }
        }
        assert!(won.is_some_and(|n| n > 0), "{won:?} {}", s.state());
    }

    #[test]
    fn catch_and_win_on_time_catches_them() {
        let Some(mut s) = scene(Kind::Catch) else {
            return;
        };
        let n = catcher(&mut s, 5.0);
        assert!(n.is_some_and(|n| n > 0), "{n:?} {}", s.state());
        assert_eq!(&s.state()[..7], "ball 10");
    }

    #[test]
    fn catch_and_win_without_jumping_misses_three_and_loses() {
        let Some(mut s) = scene(Kind::Catch) else {
            return;
        };
        let n = run(&mut s, 6000, |_| false);
        assert_eq!(n, Some(0));
        assert_eq!(s.state(), "ball 3 misses 3");
    }
}
