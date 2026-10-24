//! The Super World Toad House minigames' rules (private spec `research/specs/super-worlds.md` §6):
//! MATCH & WIN (three picture strips to stop), POP & WIN (pump a balloon until it bursts) and
//! CATCH & WIN (jump to catch ten pitches). The screens (layouts, animations, sounds) are the
//! front end's; this is the arithmetic they share with the tests.
//!
//! Units: UI frames (60 a second) and layout units (1280 × 720, y up).

use crate::Rng;

/// Which minigame a Toad House runs (its record's "minigame kind" byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Match,
    Pop,
    Catch,
}

impl Kind {
    /// 0 Match & Win, 1 Pop & Win, 2 Catch & Win; other values (not seen in real maps) run
    /// Match & Win [inf].
    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Kind::Pop,
            2 => Kind::Catch,
            _ => Kind::Match,
        }
    }

    /// The game's layout.
    pub fn layout(self) -> &'static str {
        match self {
            Kind::Match => "MyWEdit_Minigame_00",
            Kind::Pop => "MyWEdit_Minigame_01",
            Kind::Catch => "MyWEdit_Minigame_02",
        }
    }
}

/// The `TypeSuccess` frame that shows `lives` (1, 3 or 5 mushrooms).
pub fn success_frame(lives: u32) -> f32 {
    match lives {
        5.. => 2.0,
        3 | 4 => 1.0,
        _ => 0.0,
    }
}

// MARK: Match & Win

/// A picture of the strips (the game's ids: 0 Star, 1 Flower, 2 Mushroom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picture {
    Star,
    Flower,
    Mushroom,
}

impl Picture {
    /// Lives for three of a kind.
    pub fn lives(self) -> u32 {
        match self {
            Picture::Star => 5,
            Picture::Flower => 3,
            Picture::Mushroom => 1,
        }
    }

    /// The `FailT` / `FailC` / `FailB` frame showing this picture on its strip.
    pub fn fail_frame(self) -> f32 {
        match self {
            Picture::Star => 0.0,
            Picture::Flower => 1.0,
            Picture::Mushroom => 2.0,
        }
    }

    /// The pane name stem (`P_<stem><strip>_00`).
    pub fn stem(self) -> &'static str {
        match self {
            Picture::Star => "Star",
            Picture::Flower => "Flower",
            Picture::Mushroom => "Kinoko",
        }
    }
}

/// Width of one picture slot on a strip.
pub const SLOT: f32 = 384.0;
/// The strip's pictures left to right at offset 0 (Mushroom at −1 slot, Star in the window,
/// Flower at +1), looping every three slots.
pub const STRIP: [Picture; 3] = [Picture::Mushroom, Picture::Star, Picture::Flower];
/// Strips top, centre, bottom: spin speed (units per frame, + = right) and the stop's margin.
pub const STRIP_SPEED: [(f32, f32); 3] = [(-45.0, 25.0), (30.0, 20.0), (-50.0, 20.0)];
/// Frames from the end of the layout's In to each strip starting (one after the other).
pub const STRIP_START_GAPS: [i32; 3] = [30, 20, 19];
/// Slots a strip runs on after Stop before it rests.
pub const STOP_SLOTS: f32 = 4.0;
/// The stop's ease: this share of the distance left per frame.
const EASE: f32 = 0.28;
const EASE_MIN: f32 = 0.001;
const EASE_MAX: f32 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum StripState {
    Waiting,
    Spinning,
    Stopping { target: f32, max: f32, near: bool },
    Rest,
}

/// One strip of Match & Win: its offset (units, + = moved right) and motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub offset: f32,
    speed: f32,
    margin: f32,
    state: StripState,
}

impl Strip {
    /// Strip `i` (0 top, 1 centre, 2 bottom), not moving yet.
    pub fn new(i: usize) -> Self {
        let (speed, margin) = STRIP_SPEED[i.min(2)];
        Self {
            offset: 0.0,
            speed,
            margin,
            state: StripState::Waiting,
        }
    }

    pub fn start(&mut self) {
        if self.state == StripState::Waiting {
            self.state = StripState::Spinning;
        }
    }

    pub fn spinning(&self) -> bool {
        self.state == StripState::Spinning
    }

    pub fn resting(&self) -> bool {
        self.state == StripState::Rest
    }

    /// Stop: run on four slots past the slot in the window and ease in there.
    pub fn stop(&mut self) {
        if self.state != StripState::Spinning {
            return;
        }
        let dir = self.speed.signum();
        let target = ((self.offset / SLOT).round() + STOP_SLOTS * dir) * SLOT;
        let max = (EASE * (self.speed.abs() + 2.0 * self.margin)).clamp(EASE_MIN, EASE_MAX);
        self.state = StripState::Stopping {
            target,
            max,
            near: false,
        };
    }

    /// One frame; true on the frame it comes to rest.
    pub fn step(&mut self) -> bool {
        match self.state {
            StripState::Spinning => self.offset += self.speed,
            StripState::Stopping { target, max, near } => {
                let left = target - self.offset;
                let near = near || left.abs() <= self.margin;
                let reach = if near {
                    left.abs()
                } else {
                    left.abs() + self.margin
                };
                let step = (EASE * reach).clamp(EASE_MIN, max);
                if left.abs() <= step.max(EASE_MIN) {
                    self.offset = target;
                    self.state = StripState::Rest;
                    return true;
                }
                self.offset += step * left.signum();
                self.state = StripState::Stopping { target, max, near };
            }
            StripState::Waiting | StripState::Rest => {}
        }
        false
    }

    /// Where picture slot `k` of [`STRIP`] is drawn (its centre's x, the window at 0), wrapped
    /// into the loop's three slots around the window.
    pub fn slot_x(&self, k: usize) -> f32 {
        let span = SLOT * STRIP.len() as f32;
        let x = (k as f32 - 1.0) * SLOT + self.offset;
        (x + span / 2.0).rem_euclid(span) - span / 2.0
    }

    /// The picture nearest the window.
    pub fn picture(&self) -> Picture {
        (0..STRIP.len())
            .min_by(|&a, &b| self.slot_x(a).abs().total_cmp(&self.slot_x(b).abs()))
            .map(|k| STRIP[k])
            .unwrap_or(Picture::Star)
    }
}

/// Three of a kind wins its picture's lives.
pub fn match_prize(p: [Picture; 3]) -> Option<u32> {
    (p[0] == p[1] && p[1] == p[2]).then(|| p[0].lives())
}

// MARK: Pop & Win

/// Total pumps at which the balloon's growth stages (`PumpKH_00…03`) play.
pub const PUMP_STAGES: [u32; 4] = [4, 10, 18, 28];
/// Pumps that burst the balloon.
pub const PUMPS_TO_POP: u32 = 50;

/// Growth stages reached after `pumps` (0–4).
pub fn pump_stage(pumps: u32) -> usize {
    PUMP_STAGES.iter().filter(|&&t| pumps >= t).count()
}

/// The last stage's share (`PumpKH_04` held at this part of its length): 0 at 28 pumps, 1 at 50.
pub fn growth(pumps: u32) -> f32 {
    let (from, to) = (PUMP_STAGES[3] as f32, PUMPS_TO_POP as f32);
    ((pumps as f32 - from) / (to - from)).clamp(0.0, 1.0)
}

/// The burst's prize from a draw r in 0–10: r mod 3 = 0 one life, 1 three, 2 five (4, 4 and 3
/// chances in 11).
pub fn pop_prize(r: u32) -> u32 {
    match r % 3 {
        0 => 1,
        1 => 3,
        _ => 5,
    }
}

pub fn pop_draw(rng: &mut Rng) -> u32 {
    pop_prize(rng.below(11))
}

// MARK: Catch & Win

/// Balls per game.
pub const BALLS: usize = 10;
/// Misses that end it.
pub const MISS_LIMIT: u32 = 3;

/// A pitch kind (the layout's `T_PitchType_0n` / `T_PitchSpeedType_0n` index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pitch {
    Fastball,
    Changeup,
    Knuckleball,
    Forkball,
    Heater,
}

impl Pitch {
    pub const ALL: [Pitch; 5] = [
        Pitch::Fastball,
        Pitch::Changeup,
        Pitch::Knuckleball,
        Pitch::Forkball,
        Pitch::Heater,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The Heater uses the second set of pitching animations.
    pub fn second_set(self) -> bool {
        self == Pitch::Heater
    }

    /// Frames per frame of the `Pitching` animation (the ball's run).
    pub fn pitching_rate(self) -> f32 {
        match self {
            Pitch::Forkball => 0.5,
            _ => 1.0,
        }
    }

    /// Frames per frame of `RotationBall` (the ball's spin).
    pub fn rotation_rate(self) -> f32 {
        match self {
            Pitch::Fastball => 3.0,
            Pitch::Changeup => 1.0,
            Pitch::Knuckleball => 0.1,
            Pitch::Forkball => 0.2,
            Pitch::Heater => 5.0,
        }
    }

    /// The Changeup is thrown along the `PitchingAngle` once it reaches this frame.
    pub fn release_angle(self) -> Option<f32> {
        (self == Pitch::Changeup).then_some(30.0)
    }

    /// The pitch sound's context (`Anm_Pitching_00_<c…>`) [inf: which kind is which].
    pub fn sound_context(self) -> &'static str {
        match self {
            Pitch::Fastball => "cStraight",
            Pitch::Changeup => "cSlow",
            Pitch::Knuckleball => "cKnuckle",
            Pitch::Forkball => "cFork",
            Pitch::Heater => "cHighStraight",
        }
    }
}

/// One frame of the ball's flight: its translate in its track (`N_BallStart_00`, x running
/// left from 0). Fastball 18 and Heater 28 a frame straight on; the Changeup slows from 17.6 to
/// 8 over its first 200 and rides a sine wave (100 high, 1200 long); the Knuckleball slows from
/// 16 to 10 over its first 100 and wobbles (300 long, 0 / 40 / 80 high from its first, second,
/// third stretch on); the Forkball slows from 28.6 to 13 and, once within 250 of its target's
/// right edge (`near_target`), drops 0.6 of its speed a frame.
pub fn ball_step(p: Pitch, pos: [f32; 2], near_target: bool) -> [f32; 2] {
    use std::f32::consts::TAU;
    let [x, y] = pos;
    let ramp = |from: f32, base: f32, past: f32| {
        if -x >= past {
            base
        } else {
            ((x + from) * 6.0 * base / 1000.0).max(0.0) + base
        }
    };
    match p {
        Pitch::Fastball => [x - 18.0, y],
        Pitch::Heater => [x - 28.0, y],
        Pitch::Changeup => [
            x - ramp(200.0, 8.0, 200.0),
            (x / -1200.0 * TAU).sin() * 100.0,
        ],
        Pitch::Knuckleball => {
            let stretch = (x / -300.0).trunc();
            let high = (stretch * 40.0).min(80.0);
            [
                x - ramp(100.0, 10.0, 100.0),
                (x / -300.0 * TAU).sin() * high,
            ]
        }
        Pitch::Forkball => {
            let v = ramp(200.0, 13.0, 100.0);
            let drop = if near_target { v / (5.0 / 3.0) } else { 0.0 };
            [x - v, y - drop]
        }
    }
}

/// The Forkball starts dropping when the ball (its centre's x, 10 left of it) is within 250 of
/// the target's right edge.
pub fn fork_near(ball_x: f32, target_right: f32) -> bool {
    ball_x - 10.0 - target_right <= 250.0
}

/// The ten pitches: kinds 0, 0, 0, 1, 2, 2, 3, 3, 4, 4 with each of slots 2–8 swapped with a
/// random slot 2–8 (the first two are always Fastballs, the last a Heater).
pub fn pitch_order(rng: &mut Rng) -> [Pitch; BALLS] {
    use Pitch::*;
    let mut a = [
        Fastball,
        Fastball,
        Fastball,
        Changeup,
        Knuckleball,
        Knuckleball,
        Forkball,
        Forkball,
        Heater,
        Heater,
    ];
    for k in 2..=8 {
        let j = 2 + rng.below(7) as usize;
        a.swap(k, j);
    }
    a
}

/// An axis-aligned box (layout units, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Bounds {
    pub fn around(center: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            min: [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0],
            max: [center[0] + size[0] / 2.0, center[1] + size[1] / 2.0],
        }
    }

    pub fn center(&self) -> [f32; 2] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }

    fn contains(&self, p: [f32; 2]) -> bool {
        (self.min[0]..=self.max[0]).contains(&p[0]) && (self.min[1]..=self.max[1]).contains(&p[1])
    }
}

/// The catch: the ball's centre between the character box's centre and its right edge, and 30
/// above or 35 below it within the box's height.
pub fn caught(ball: [f32; 2], body: &Bounds) -> bool {
    let cx = (body.min[0] + body.max[0]) / 2.0;
    let across = (cx..=body.max[0]).contains(&ball[0]);
    let height = |y: f32| (body.min[1]..=body.max[1]).contains(&y);
    across && (height(ball[1] + 30.0) || height(ball[1] - 35.0))
}

/// The ball got past everyone: 60 left and 40 right of it, 35 up, both inside the Toad House.
pub fn reached_house(ball: [f32; 2], house: &Bounds) -> bool {
    let y = ball[1] + 35.0;
    house.contains([ball[0] - 60.0, y]) && house.contains([ball[0] + 40.0, y])
}

/// Lives for a finished game by its misses (0 → 5, 1 → 3, 2 → 1); None for a lost one.
pub fn catch_prize(misses: u32) -> Option<u32> {
    match misses {
        0 => Some(5),
        1 => Some(3),
        2 => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_the_house_byte() {
        assert_eq!(Kind::from_byte(0), Kind::Match);
        assert_eq!(Kind::from_byte(1), Kind::Pop);
        assert_eq!(Kind::from_byte(2), Kind::Catch);
        assert_eq!(Kind::Catch.layout(), "MyWEdit_Minigame_02");
    }

    #[test]
    fn strips_spin_their_ways_and_stop_four_slots_on() {
        let mut s: Vec<Strip> = (0..3).map(Strip::new).collect();
        for x in &mut s {
            assert_eq!(x.picture(), Picture::Star);
            x.start();
            x.step();
        }
        assert_eq!(
            s.iter().map(|x| x.offset).collect::<Vec<_>>(),
            [-45.0, 30.0, -50.0]
        );
        // the top strip: stopped at −45 it runs to −4 slots (the Star again, a lap and one on)
        let top = &mut s[0];
        top.stop();
        let mut frames = 0;
        while !top.step() {
            frames += 1;
            assert!(frames < 600, "never rests");
        }
        assert!(top.resting());
        assert_eq!(top.offset, -4.0 * SLOT);
        // −4 slots = −1 loop and −1 slot: the Flower moves into the window
        assert_eq!(top.picture(), Picture::Flower);
        // it never moved faster than its stop speed (0.28 × (45 + 50) = 26.6 a frame)
        assert!(frames >= (4.0 * SLOT - 45.0) as i32 / 27);
        // the centre one, stopped after 13 frames (390 = one slot on, the Mushroom showing):
        // five slots on, the Flower
        let c = &mut s[1];
        for _ in 0..12 {
            c.step();
        }
        assert_eq!(c.picture(), Picture::Mushroom);
        c.stop();
        while !c.step() {}
        assert_eq!(c.offset, 5.0 * SLOT);
        assert_eq!(c.picture(), Picture::Flower);
    }

    #[test]
    fn three_of_a_kind_pays_by_picture() {
        use Picture::*;
        assert_eq!(match_prize([Star, Star, Star]), Some(5));
        assert_eq!(match_prize([Flower, Flower, Flower]), Some(3));
        assert_eq!(match_prize([Mushroom, Mushroom, Mushroom]), Some(1));
        assert_eq!(match_prize([Star, Star, Flower]), None);
        assert_eq!(success_frame(5), 2.0);
        assert_eq!(success_frame(1), 0.0);
    }

    #[test]
    fn the_balloon_grows_in_stages_and_pops_at_fifty() {
        assert_eq!(pump_stage(3), 0);
        assert_eq!(pump_stage(4), 1);
        assert_eq!(pump_stage(17), 2);
        assert_eq!(pump_stage(18), 3);
        assert_eq!(pump_stage(28), 4);
        assert_eq!(growth(28), 0.0);
        assert_eq!(growth(39), 0.5);
        assert_eq!(growth(50), 1.0);
        let prizes: Vec<u32> = (0..11).map(pop_prize).collect();
        assert_eq!(prizes, [1, 3, 5, 1, 3, 5, 1, 3, 5, 1, 3]);
        let mut rng = Rng::new(7);
        let mut seen = [0; 6];
        for _ in 0..1100 {
            seen[pop_draw(&mut rng) as usize] += 1;
        }
        assert!(seen[1] > seen[5] && seen[3] > seen[5] && seen[5] > 200);
    }

    #[test]
    fn ten_pitches_start_fast_and_end_with_a_heater() {
        for seed in 1..50 {
            let order = pitch_order(&mut Rng::new(seed));
            assert_eq!(&order[..2], &[Pitch::Fastball, Pitch::Fastball]);
            assert_eq!(order[9], Pitch::Heater);
            let count = |p| order.iter().filter(|&&x| x == p).count();
            assert_eq!(
                Pitch::ALL.map(count),
                [3, 1, 2, 2, 2],
                "the same ten every game"
            );
        }
        assert_eq!(Pitch::Forkball.pitching_rate(), 0.5);
        // the flights: straight, slowing, waving, dropping
        assert_eq!(ball_step(Pitch::Fastball, [0.0, 0.0], false), [-18.0, 0.0]);
        assert_eq!(ball_step(Pitch::Heater, [-50.0, 0.0], false), [-78.0, 0.0]);
        let c = ball_step(Pitch::Changeup, [0.0, 0.0], false);
        assert!((c[0] + 17.6).abs() < 1e-4 && c[1] == 0.0);
        assert_eq!(
            ball_step(Pitch::Changeup, [-300.0, 0.0], false),
            [-308.0, 100.0]
        );
        assert_eq!(
            ball_step(Pitch::Knuckleball, [-150.0, 5.0], false)[0],
            -160.0
        );
        let k = ball_step(Pitch::Knuckleball, [-375.0, 0.0], false);
        assert!((k[1] - 40.0).abs() < 1e-3, "{k:?}");
        assert_eq!(
            ball_step(Pitch::Forkball, [-200.0, 0.0], false),
            [-213.0, 0.0]
        );
        let f = ball_step(Pitch::Forkball, [-200.0, 0.0], true);
        assert!((f[1] + 7.8).abs() < 1e-4);
        assert!(fork_near(20.0, -221.0) && !fork_near(100.0, -221.0));
        assert_eq!(Pitch::Changeup.release_angle(), Some(30.0));
        assert!(Pitch::Heater.second_set());
    }

    #[test]
    fn catches_misses_and_the_prize() {
        // Mario's box: 82 × 120 around (−250, −200)
        let body = Bounds::around([-250.0, -200.0], [82.0, 120.0]);
        // still right of him, or past his middle: no
        assert!(!caught([-200.0, -200.0], &body));
        assert!(!caught([-260.0, -200.0], &body));
        // over his right half at a height his box covers (ball 30 below the top or 35 above
        // the bottom)
        assert!(caught([-230.0, -200.0], &body));
        assert!(caught([-230.0, -170.0], &body));
        assert!(caught([-230.0, -105.0], &body));
        assert!(!caught([-230.0, -60.0], &body), "over his head");
        // jumped up 100: the high ball is caught
        let up = Bounds::around([-250.0, -100.0], [82.0, 120.0]);
        assert!(caught([-230.0, -60.0], &up));
        let house = Bounds::around([-431.0, -200.0], [270.0, 270.0]);
        assert!(reached_house([-431.0, -200.0], &house));
        assert!(!reached_house([-250.0, -200.0], &house));
        assert_eq!(catch_prize(0), Some(5));
        assert_eq!(catch_prize(1), Some(3));
        assert_eq!(catch_prize(2), Some(1));
        assert_eq!(catch_prize(3), None);
    }
}
