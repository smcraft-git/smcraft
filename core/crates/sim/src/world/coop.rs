//! Local co-op ("Play Together"): 2–4 players on one screen by SMM2's local rules, from the
//! behavioural spec `research/specs/local-coop.md` (private), not from game code.
//!
//! - Every player runs the same player rules: each tick one player in play (the **lead**) runs
//!   the whole world tick ([`World::step`]); the others run only the player's part of it (the
//!   *extra pass*), swapped into `World::player` with the world fields that belong to a player.
//! - A player who dies while another is still in play doesn't cost a life: after the death fall
//!   they come back in a **bubble** that floats after the nearest player in play and pops when
//!   one touches it. The attempt is lost only when nobody is left in play (everyone dead or in a
//!   bubble at once): then the course restarts and one shared life is gone.
//! - One shared camera follows the players in play ([`Camera::update_players`]); its edges push
//!   players along, and one squeezed past an edge or left below the view dies.
//! - Players bounce off each other's heads (the springboard) and push each other aside.
//! - The goal waits up to 240 frames for the others once one player has it; one player at the
//!   goal clears the course. A warp takes everyone along: the others come out after the leader,
//!   15 frames apart.
//!
//! Slots: slot k is input k and character k (Mario, Luigi, Toad, Toadette). Outside a tick,
//! slot 0 is `World::player` and slot k is `World::others[k − 1]`.

use super::*;

/// Course start: player k stands 16·k px left of player 1 (local-coop.md §1.2).
pub const SPACING: f32 = 16.0;
/// The bubble (§2.2): steering per frame, the wobble across it, the speed cap, the dash.
const BUBBLE_STEER: f32 = 0.01;
const BUBBLE_WOBBLE: f32 = 0.01;
const BUBBLE_MAX: f32 = 1.0;
const BUBBLE_DASH: i32 = 30;
/// It appears 80 px right of the view at its middle height and stays 16 px inside the sides.
const BUBBLE_ENTER_X: f32 = 80.0;
const BUBBLE_SIDE: f32 = 16.0;
/// Hit box half width and height (x ± 16, y … y + 32); the player sits 8 px up inside.
const BUBBLE_HALF: f32 = 16.0;
const BUBBLE_H: f32 = 32.0;
/// The death fall ends in a bubble 64 px below the view (§2.1).
const BUBBLE_BELOW: f32 = 64.0;
/// Popped: the jump out (vy 3.628) and 127 frames of invincibility; through walls the player
/// first flies to the popper at 0.15 × the distance a frame (§2.3).
const POP_VY: f32 = 3.628;
const POP_INVULN: i32 = 127;
const RESCUE_RATE: f32 = 0.15;
/// The view's edges (§4.5): the centre x is kept 8.1 px inside; 16 px past an edge (a wall held
/// the player back) or 56 px below the view is death; the feet stay within 96 px over the top.
const EDGE: f32 = 8.1;
const SQUEEZE: f32 = 16.0;
const PIT_BELOW: f32 = 56.0;
const TOP_REACH: f32 = 96.0;
/// Head contact (§5.1): the springboard's wait and launches, the stomp bounce, the target's
/// cooldown and freeze.
const SPRING_WAIT: i32 = 4;
const SPRING_VY: f32 = 4.628;
const SPRING_VY_HELD: f32 = 3.828;
const SPRING_VY_HIGH: f32 = 4.428;
const BOUNCE_VY: f32 = 3.628;
const BOUNCE_VY_HELD: f32 = 4.128;
const STOMP_COOLDOWN: i32 = 10;
const STOMP_FREEZE: i32 = 4;
const SPRING_IGNORE: i32 = 5;
/// Side push (§5.2): only with 2 px of vertical overlap; at most 3 px a frame.
const PUSH_OVERLAP: f32 = 2.0;
const PUSH_MAX: f32 = 3.0;
/// The goal (§6): the wait for the others, the one-time grace for a player near the pole, how
/// near; the slide delay per place on the pole.
const GOAL_WAIT: i32 = 240;
const GOAL_GRACE: i32 = 30;
const GOAL_NEAR: f32 = 16.0;
const POLE_DELAY: i32 = 20;
/// Warps (§7): the followers come out 15 frames apart.
const WARP_GAP: i32 = 15;

/// A co-op player's situation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum State {
    /// Playing (or dead: the player's own death pose and fall).
    #[default]
    Play,
    /// Floating in a bubble.
    Bubble(Bubble),
    /// Popped behind a wall: flying to the popper (slot `to`) before jumping out.
    Rescue { to: usize },
    /// Sitting on slot `under`'s head before the springboard launch: x offset, frames sat, the
    /// speed it landed with, a jump press during the wait (the high jump).
    Spring {
        under: usize,
        dx: f32,
        t: i32,
        saved_vx: f32,
        high: bool,
    },
    /// On the goal pole: frames before sliding once the demo runs (−1: not yet counted).
    Pole { slide: i32 },
    /// Taken along through a warp: hidden until the frames run out.
    Follow { t: i32 },
}

/// The bubble: its bottom-centre, velocity, wobble angle (degrees), dash frames left and
/// direction, and whether it has been on screen yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bubble {
    pub x: f32,
    pub y: f32,
    vx: f32,
    vy: f32,
    wobble: f32,
    pub dash: i32,
    dash_dir: (f32, f32),
    shown: bool,
}

/// One slot's co-op bookkeeping.
#[derive(Clone, Debug, Default)]
pub struct Slot {
    pub state: State,
    prev_jump: bool,
    /// Just stomped (no new head contact) and frozen on the ground (frames).
    cooldown: i32,
    freeze: i32,
    /// Ignoring contact with slot .0 for .1 frames (after a springboard).
    ignore: Option<(usize, i32)>,
}

/// The goal's wait for the others (§6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GoalWait {
    pub timer: i32,
    extended: bool,
    /// Slots on the pole, in touch order (the first is the winner).
    pub list: Vec<usize>,
    pub started: bool,
}

/// World fields that belong to one player (swapped with them).
#[derive(Clone, Debug, Default)]
pub(super) struct Ctx {
    prev_up: bool,
    side_hold: SideHold,
    run_held: bool,
    jump_pressed_now: bool,
    prev_jump_tramp: bool,
    tramp_late: i32,
    clown_prev: (bool, bool),
    prev_jump_claw: bool,
    held_by: Option<u32>,
    yoshi_mouth: Option<Kind>,
    tongue: i32,
    yoshi_fire_cd: i32,
    yoshi_prev_run: bool,
    carry: f32,
    donut_ride: Option<((i32, i32), i32)>,
    pu: powerups::PuWorld,
}

/// The co-op state of a world (empty with one player).
#[derive(Clone, Debug, Default)]
pub struct Coop {
    pub slots: Vec<Slot>,
    /// Position → slot (position 0 is `World::player`, k is `others[k − 1]`).
    at: Vec<usize>,
    /// The per-player world fields of positions 1… (position 0's are the world's own).
    ctx: Vec<Ctx>,
    /// An extra player's pass is running.
    pub(super) extra: bool,
    /// The course was (re)built during this tick.
    pub(super) reset: bool,
    /// The slot that went down last (it carries the all-down death sequence).
    last_down: usize,
    /// The slot travelling through a warp.
    warp_lead: Option<usize>,
    pub goal: Option<GoalWait>,
    rng: u32,
}

impl Coop {
    fn new(n: usize) -> Self {
        if n < 2 {
            return Self::default();
        }
        Self {
            slots: vec![Slot::default(); n],
            at: (0..n).collect(),
            ctx: vec![Ctx::default(); n - 1],
            rng: 0x2545_f491,
            ..Self::default()
        }
    }

    /// Several players.
    pub fn active(&self) -> bool {
        self.slots.len() > 1
    }

    /// The goal is waiting for the others (the course timer stops).
    pub fn goal_waiting(&self) -> bool {
        self.goal.as_ref().is_some_and(|g| !g.started)
    }

    fn random(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }
}

fn rotate(v: (f32, f32), deg: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().dsin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

impl World {
    /// Local multiplayer: `n` players in all (1–4). Players 2–4 are Luigi, Toad and Toadette and
    /// start 16 px apart to the left of player 1 (who is Mario in co-op).
    pub fn set_players(&mut self, n: usize) {
        let n = n.clamp(1, 4);
        self.others = (1..n).map(|_| self.player.clone()).collect();
        self.other_chars = (1..n).collect();
        if n > 1 {
            self.main_char = 0;
        }
        self.coop = Coop::new(n);
        self.place_players();
    }

    /// How many players.
    pub fn players(&self) -> usize {
        1 + self.others.len()
    }

    /// Everyone beside player 1 again (course start, restart), fresh.
    fn place_players(&mut self) {
        if self.others.is_empty() {
            return;
        }
        let (x, y) = (self.player.x, self.player.y);
        for (k, o) in self.others.iter_mut().enumerate() {
            *o = Player::new(x - SPACING * (k + 1) as f32, y);
        }
        for s in &mut self.coop.slots {
            *s = Slot::default();
        }
        for c in &mut self.coop.ctx {
            *c = Ctx::default();
        }
        self.coop.goal = None;
        self.coop.warp_lead = None;
        self.coop.last_down = 0;
        self.coop_reset_camera();
    }

    /// The shared camera re-centred with a seat per player (start, restart, after a warp).
    pub(super) fn coop_reset_camera(&mut self) {
        let area = self.camera_area();
        let lead = self.slot(self.coop.warp_lead.unwrap_or(0));
        let centre = (lead.x, lead.y + 16.0);
        let feet: Vec<(f32, f32)> = (0..self.players())
            .map(|k| (self.slot(k).x, self.slot(k).y))
            .collect();
        let level = &self.level;
        self.camera.reset_players(&area, centre, &feet, |tx, ty| {
            level.cell(tx, ty) == Cell::Solid
        });
        self.sync_camera();
    }

    // ------------------------------------------------------------ slots and positions

    fn position_of(&self, k: usize) -> usize {
        self.coop.at.iter().position(|&s| s == k).unwrap_or(k)
    }

    /// Slot `k`'s player, wherever it is this moment.
    pub fn slot(&self, k: usize) -> &Player {
        match self.position_of(k) {
            0 => &self.player,
            p => &self.others[p - 1],
        }
    }

    fn slot_mut(&mut self, k: usize) -> &mut Player {
        match self.position_of(k) {
            0 => &mut self.player,
            p => &mut self.others[p - 1],
        }
    }

    /// Swap the world's per-player fields with `c`.
    fn swap_ctx(&mut self, c: &mut Ctx) {
        use std::mem::swap;
        swap(&mut self.prev_up, &mut c.prev_up);
        swap(&mut self.side_hold, &mut c.side_hold);
        swap(&mut self.run_held, &mut c.run_held);
        swap(&mut self.jump_pressed_now, &mut c.jump_pressed_now);
        swap(&mut self.prev_jump_tramp, &mut c.prev_jump_tramp);
        swap(&mut self.tramp_late, &mut c.tramp_late);
        swap(&mut self.clown_prev, &mut c.clown_prev);
        swap(&mut self.prev_jump_claw, &mut c.prev_jump_claw);
        swap(&mut self.held_by, &mut c.held_by);
        swap(&mut self.yoshi_mouth, &mut c.yoshi_mouth);
        swap(&mut self.tongue, &mut c.tongue);
        swap(&mut self.yoshi_fire_cd, &mut c.yoshi_fire_cd);
        swap(&mut self.yoshi_prev_run, &mut c.yoshi_prev_run);
        swap(&mut self.carry, &mut c.carry);
        swap(&mut self.donut_ride, &mut c.donut_ride);
        swap(&mut self.pu, &mut c.pu);
    }

    /// Swap the players at positions `a` < `b` (with their characters and fields).
    fn swap_positions(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        let (a, b) = (a.min(b), a.max(b));
        if a == 0 {
            std::mem::swap(&mut self.player, &mut self.others[b - 1]);
            std::mem::swap(&mut self.main_char, &mut self.other_chars[b - 1]);
            let mut c = std::mem::take(&mut self.coop.ctx[b - 1]);
            self.swap_ctx(&mut c);
            self.coop.ctx[b - 1] = c;
        } else {
            self.others.swap(a - 1, b - 1);
            self.other_chars.swap(a - 1, b - 1);
            self.coop.ctx.swap(a - 1, b - 1);
        }
        self.coop.at.swap(a, b);
    }

    /// Put slot `k` into `World::player`.
    fn bring_front(&mut self, k: usize) {
        let p = self.position_of(k);
        self.swap_positions(0, p);
    }

    /// Back to slot k at position k.
    fn restore_order(&mut self) {
        for pos in 0..self.coop.at.len() {
            let p = self.position_of(pos);
            self.swap_positions(pos, p);
        }
    }

    /// In play: alive and not in a bubble (§3; pole and springboard players count).
    pub fn in_play(&self, k: usize) -> bool {
        match self.coop.slots.get(k).map(|s| s.state) {
            Some(State::Play) => self.slot(k).alive() && !self.slot(k).hidden,
            Some(State::Bubble(_)) | Some(State::Follow { .. }) | None => false,
            Some(_) => true,
        }
    }

    /// Free: in play and playing (not on the pole, not sitting on a head, not flying back).
    fn free(&self, k: usize) -> bool {
        self.coop.slots[k].state == State::Play
            && self.in_play(k)
            && self.slot(k).control != Control::Demo
    }

    fn anyone_in_play(&self) -> bool {
        (0..self.players()).any(|k| self.in_play(k))
    }

    /// The co-op "down": nobody in play, the death sequence that restarts the course (with one
    /// player: player 1 is dead).
    pub fn all_down(&self) -> bool {
        if self.others.is_empty() {
            return !self.player.alive();
        }
        !self.anyone_in_play()
    }

    /// The player whose death sequence restarts the course (with one player: player 1, while
    /// dead; in co-op: the last one down, once nobody is in play).
    pub fn down_player(&self) -> Option<&Player> {
        if self.others.is_empty() {
            return (!self.player.alive()).then_some(&self.player);
        }
        self.all_down().then(|| self.slot(self.coop.last_down))
    }

    /// Frames until the death sequence restarts the course; −1 while someone plays.
    pub fn restart_in(&self) -> i32 {
        match self.down_player().map(|p| p.control) {
            Some(Control::DownPose { timer }) => timer + crate::player::DOWN_FALL_FRAMES,
            Some(Control::DownFall { timer }) => timer,
            _ => -1,
        }
    }

    /// Who runs the world this tick.
    fn pick_lead(&self) -> usize {
        if self.goal_phase != GoalPhase::None
            && let Some(w) = self.coop.goal.as_ref().and_then(|g| g.list.first())
        {
            return *w;
        }
        if self.transit.is_some()
            && let Some(k) = self.coop.warp_lead
        {
            return k;
        }
        let n = self.players();
        (0..n)
            .find(|&k| self.free(k))
            .or_else(|| (0..n).find(|&k| self.in_play(k)))
            .unwrap_or(self.coop.last_down)
    }

    // ------------------------------------------------------------ the tick

    /// One tick with an input per player (player 1 first). With one player it is
    /// [`World::step`].
    pub fn step_players(&mut self, inputs: &[Input]) {
        let input = |k: usize| inputs.get(k).copied().unwrap_or_default();
        if self.others.is_empty() {
            self.step(input(0));
            return;
        }
        if self.coop.slots.len() != self.players() {
            let n = self.players();
            self.coop = Coop::new(n);
        }
        self.coop.reset = false;
        let lead = self.pick_lead();
        self.bring_front(lead);
        // the world waits (as in single player) while the lead grows or shrinks, travels
        // through a warp, does the goal demo or carries the all-down death sequence
        let waits = self.transit.is_some()
            || self.goal_phase != GoalPhase::None
            || !self.player.alive()
            || matches!(self.player.control, Control::Change { .. });
        let area = self.area;
        let warping = self.transit.is_some();
        self.step(input(lead));
        // a restart rebuilt the course with the lead in front: player 1 starts there
        let restart_at = self.coop.reset.then_some((self.player.x, self.player.y));
        if !self.player.alive() && !waits {
            self.coop.last_down = lead;
        }
        if self.transit.is_some() && !warping {
            self.coop.warp_lead = Some(lead);
        }
        if !waits && self.transit.is_none() && !self.coop.reset {
            for k in 0..self.players() {
                if k != lead {
                    self.step_slot(k, input(k));
                    if self.transit.is_some() {
                        break; // someone took a warp: everyone else waits from here
                    }
                }
            }
        }
        let area_changed = self.area != area;
        let warp_done = warping && self.transit.is_none();
        self.restore_order();
        if let Some((x, y)) = restart_at {
            self.player = Player::new(x, y);
            self.place_players();
        } else {
            if area_changed {
                self.take_along();
            }
            if warp_done {
                self.coop.warp_lead = None;
            }
            self.coop_after(inputs);
        }
        self.coop_camera();
    }

    /// One extra player's part of a tick (in its slot's state).
    fn step_slot(&mut self, k: usize, input: Input) {
        if self.coop.slots[k].state != State::Play {
            return; // bubbles, rescues, springboards, the pole, warps: coop_after
        }
        self.bring_front(k);
        if !self.player.alive() {
            // the death pose, then the fall; it ends in a bubble below the view while someone
            // else is still in play (§2.1)
            self.player.step(input, &self.level);
            let fallen = matches!(self.player.control, Control::DownFall { .. })
                && (self.player.y <= self.camera_y - BUBBLE_BELOW || self.player.restart_due());
            if fallen && !self.player.hidden {
                if self.anyone_in_play() {
                    self.enter_bubble(k);
                } else {
                    self.player.hidden = true;
                }
            }
            return;
        }
        if self.player.control == Control::Demo {
            return;
        }
        self.coop.extra = true;
        self.step(input);
        self.coop.extra = false;
        if !self.player.alive() {
            self.coop.last_down = k;
        }
        if self.transit.is_some() {
            self.coop.warp_lead = Some(k);
        }
    }

    /// The rest of an extra player's pass: the player-side calls of [`World::step`] after the
    /// world's blocks and actors, in the same order.
    pub(super) fn extra_player_rest(&mut self, input: Input, jump_held: bool) {
        self.carry_held();
        self.step_crates();
        self.twister_lift();
        self.claws(input);
        self.pow_quake();
        self.cape_quake();
        self.bump_moving_block();
        self.tail_attack();
        self.pound_blocks();
        self.step_yoshi_rider();
        self.check_checkpoints();
        self.player_interactions(jump_held);
        if self
            .events
            .iter()
            .any(|e| matches!(e, Event::PowerDown | Event::Death))
        {
            self.clear.damaged();
        }
        if self.player.in_water {
            self.clear.in_water();
        }
        self.check_goal();
        self.autoscroll_push();
    }

    /// A player of `k` goes into a bubble at the view's right, small (§2.1–2.2).
    fn enter_bubble(&mut self, k: usize) {
        let (bx, by) = (
            self.camera_x + VIEW_W + BUBBLE_ENTER_X,
            self.camera_y + VIEW_H / 2.0 - 8.0,
        );
        let wobble = (self.coop.random() % 360) as f32;
        let p = self.slot_mut(k);
        let mut fresh = Player::new(bx, by + 8.0);
        fresh.control = Control::Demo;
        fresh.on_ground = false;
        fresh.pose = Pose::Jump;
        fresh.facing = -1;
        fresh.drawn_facing = -1;
        *p = fresh;
        self.coop.slots[k].state = State::Bubble(Bubble {
            x: bx,
            y: by,
            vx: 0.0,
            vy: 0.0,
            wobble,
            dash: 0,
            dash_dir: (0.0, 0.0),
            shown: false,
        });
        self.events.push(Event::Bubble);
    }

    /// Out of a bubble (or a rescue flight): the jump out with 127 frames of invincibility.
    fn jump_out(&mut self, k: usize) {
        self.coop.slots[k].state = State::Play;
        let p = self.slot_mut(k);
        p.control = Control::Normal;
        p.hidden = false;
        p.vx = 0.0;
        p.vy = POP_VY;
        p.on_ground = false;
        p.pose = Pose::Jump;
        p.invuln = POP_INVULN;
    }

    /// Nothing solid on the way from `a` to `b` at the feet and at head height.
    fn clear_line(&self, a: (f32, f32), b: (f32, f32)) -> bool {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let steps = ((dx.abs().max(dy.abs())) / 4.0).ceil().max(1.0) as i32;
        (0..=steps).all(|i| {
            let t = i as f32 / steps as f32;
            let (x, y) = (a.0 + dx * t, a.1 + dy * t);
            !self.level.cell_at(x, y + 1.0).is_solid()
                && !self.level.cell_at(x, y + 16.0).is_solid()
        })
    }

    /// After everyone moved: bubbles, rescues, springboards, contact, the view's edges, the
    /// goal's wait, warps (in slot order, positions restored).
    fn coop_after(&mut self, inputs: &[Input]) {
        let n = self.players();
        let jump = |k: usize| inputs.get(k).is_some_and(|i| i.held(input::JUMP));
        let frozen = self.transit.is_some() || self.all_down();
        for k in 0..n {
            let s = &mut self.coop.slots[k];
            s.cooldown = (s.cooldown - 1).max(0);
            if let Some((o, t)) = s.ignore {
                s.ignore = (t > 1).then_some((o, t - 1));
            }
        }
        if !frozen && self.goal_phase == GoalPhase::None {
            for k in 0..n {
                match self.coop.slots[k].state {
                    State::Bubble(b) => self.step_bubble(k, b, jump(k)),
                    State::Rescue { to } => self.step_rescue(k, to),
                    State::Spring { .. } => self.step_spring(k, jump(k)),
                    State::Follow { t } => self.step_follow(k, t),
                    _ => {}
                }
            }
            self.pop_bubbles();
            self.head_contact(inputs);
            self.side_push();
            self.view_edges();
            self.time_up();
        }
        for k in 0..n {
            self.coop.slots[k].prev_jump = jump(k);
        }
        self.goal_wait();
        self.pole_demo();
    }

    // ------------------------------------------------------------ bubbles

    fn step_bubble(&mut self, k: usize, mut b: Bubble, jump: bool) {
        let pressed = jump && !self.coop.slots[k].prev_jump;
        // the nearest player in play (body centre)
        let (cx, cy) = (b.x, b.y + BUBBLE_H / 2.0);
        let target = (0..self.players())
            .filter(|&j| j != k && self.in_play(j))
            .map(|j| {
                let p = self.slot(j);
                (p.x, p.y + 8.0)
            })
            .min_by(|a, c| {
                let da = (a.0 - cx).dhypot(a.1 - cy);
                let dc = (c.0 - cx).dhypot(c.1 - cy);
                da.total_cmp(&dc)
            });
        let unit = target.map(|(tx, ty)| {
            let (dx, dy) = (tx - cx, ty - cy);
            let d = dx.dhypot(dy).max(1e-3);
            (dx / d, dy / d)
        });
        if let Some(u) = unit {
            let w = b.wobble.to_radians().dsin() * BUBBLE_WOBBLE;
            b.vx += u.0 * BUBBLE_STEER - u.1 * w;
            b.vy += u.1 * BUBBLE_STEER + u.0 * w;
        }
        b.wobble = (b.wobble + 1.0) % 360.0;
        if pressed && b.dash == 0 {
            // a 30-frame dash toward the target, up to 30° off
            let off = (self.coop.random() % 61) as f32 - 30.0;
            b.dash_dir = rotate(unit.unwrap_or((0.0, 1.0)), off);
            b.dash = BUBBLE_DASH;
        }
        if b.dash > 0 {
            let speed = b.dash as f32 / BUBBLE_DASH as f32 + 1.0;
            b.vx = b.dash_dir.0 * speed;
            b.vy = b.dash_dir.1 * speed;
            b.dash -= 1;
        } else {
            let s = b.vx.dhypot(b.vy);
            if s > BUBBLE_MAX {
                b.vx *= BUBBLE_MAX / s;
                b.vy *= BUBBLE_MAX / s;
            }
        }
        // no terrain: it passes through everything, kept to the view
        b.x += b.vx;
        b.y += b.vy;
        let (l, bo, r, t) = self.view_rect(0.0);
        if !b.shown {
            b.x = b.x.clamp(l - 32.0 + BUBBLE_HALF, r + 32.0 - BUBBLE_HALF);
            b.y = b.y.clamp(bo - 48.0, t + 32.0 - BUBBLE_H);
            b.shown = b.x - BUBBLE_HALF >= l
                && b.x + BUBBLE_HALF <= r
                && b.y >= bo
                && b.y + BUBBLE_H <= t;
        } else {
            b.x = b.x.clamp(l + BUBBLE_SIDE, r - BUBBLE_SIDE);
            b.y = b.y.clamp(bo, t - BUBBLE_H);
        }
        let p = self.slot_mut(k);
        p.x = b.x;
        p.y = b.y + 8.0;
        p.vx = b.vx;
        p.vy = b.vy;
        if b.vx.abs() > 0.05 {
            p.facing = if b.vx > 0.0 { 1 } else { -1 };
            p.drawn_facing = p.facing;
        }
        self.coop.slots[k].state = State::Bubble(b);
    }

    /// A player in play touching a bubble on screen pops it (§2.3).
    fn pop_bubbles(&mut self) {
        let n = self.players();
        let v = self.view_rect(0.0);
        for k in 0..n {
            let State::Bubble(b) = self.coop.slots[k].state else {
                continue;
            };
            let bbox = (b.x - BUBBLE_HALF, b.y, b.x + BUBBLE_HALF, b.y + BUBBLE_H);
            let on_screen = b.x >= v.0 && b.x <= v.2 && b.y >= v.1 && b.y <= v.3;
            let popper = (0..n).find(|&j| {
                j != k
                    && self.coop.slots[j].state == State::Play
                    && self.in_play(j)
                    && overlap(self.slot(j).contact_box(), bbox)
            });
            let Some(j) = popper.filter(|_| on_screen) else {
                continue;
            };
            self.events.push(Event::BubblePop);
            let to = (self.slot(j).x, self.slot(j).y);
            if self.clear_line((b.x, b.y + 8.0), to) {
                self.jump_out(k);
            } else {
                self.coop.slots[k].state = State::Rescue { to: j };
                let p = self.slot_mut(k);
                p.control = Control::Demo;
            }
        }
    }

    /// Flying to the popper through walls: 0.15 × the distance a frame (at least 0.01), 8 px
    /// steps in the last 16 px, then the jump out; at once if the way is clear.
    fn step_rescue(&mut self, k: usize, to: usize) {
        let t = (self.slot(to).x, self.slot(to).y + 1.0);
        let (x, y) = (self.slot(k).x, self.slot(k).y);
        if self.clear_line((x, y), t) || !self.in_play(to) {
            self.jump_out(k);
            return;
        }
        let (dx, dy) = (t.0 - x, t.1 - y);
        let d = dx.dhypot(dy);
        let (nx, ny) = if d <= 8.0 {
            t
        } else {
            let step = if d <= 16.0 {
                8.0
            } else {
                (d * RESCUE_RATE).max(0.01)
            };
            (x + dx / d * step, y + dy / d * step)
        };
        let p = self.slot_mut(k);
        p.x = nx;
        p.y = ny;
        if d <= 8.0 {
            self.jump_out(k);
        }
    }

    // ------------------------------------------------------------ player contact

    /// Size class for the push table: 0 small, 1 big, 2 Mega.
    fn size_class(p: &Player) -> usize {
        if p.form == Form::Mega {
            2
        } else if p.big() {
            1
        } else {
            0
        }
    }

    /// Can take part in contact: playing, alive, in control.
    fn touchable(&self, k: usize) -> bool {
        self.free(k) && self.slot(k).control == Control::Normal
    }

    fn ignoring(&self, a: usize, b: usize) -> bool {
        let ig = |x: usize, y: usize| self.coop.slots[x].ignore.is_some_and(|(o, _)| o == y);
        ig(a, b) || ig(b, a)
    }

    /// A player falling onto another's head (§5.1): off a helmet, a stomp bounce off a rising
    /// player, else the springboard.
    fn head_contact(&mut self, inputs: &[Input]) {
        let n = self.players();
        for a in 0..n {
            for b in 0..n {
                if a == b || !self.touchable(a) || !self.touchable(b) || self.ignoring(a, b) {
                    continue;
                }
                if self.coop.slots[b].cooldown > 0 {
                    continue;
                }
                let (pa, pb) = (self.slot(a), self.slot(b));
                if pa.on_ground || pa.vy >= 0.0 || pa.climbing.is_some() || pa.held.is_some() {
                    continue;
                }
                let (ab, bb) = (pa.contact_box(), pb.contact_box());
                if !overlap(ab, bb) {
                    continue;
                }
                let top = bb.3;
                let window = ((bb.1 + bb.3) * 0.5).max(top - 16.0);
                if pa.y < window {
                    continue;
                }
                let held = inputs.get(a).is_some_and(|i| i.held(input::JUMP));
                let (b_vy, b_vx, b_rising, b_helmet) = (
                    pb.vy,
                    pb.vx,
                    pb.vy > 0.0 && !pb.on_ground,
                    pb.helmet.is_some(),
                );
                let (ax, bx) = (pa.x, pb.x);
                self.events.push(Event::Stomp);
                if b_helmet {
                    let p = self.slot_mut(a);
                    p.y = top;
                    p.vy = b_vy + BOUNCE_VY;
                    p.vx = p.facing as f32 * if b_vy > 0.0 { 1.5 } else { 1.0 } + 0.5 * b_vx.abs();
                } else if b_rising {
                    let p = self.slot_mut(a);
                    p.y = top;
                    p.vy = if held { BOUNCE_VY_HELD } else { BOUNCE_VY };
                    let q = self.slot_mut(b);
                    q.vy = q.vy.min(0.0);
                    self.coop.slots[b].cooldown = STOMP_COOLDOWN;
                } else {
                    let p = self.slot_mut(a);
                    let saved_vx = p.vx;
                    p.y = top;
                    p.vx = 0.0;
                    p.vy = 0.0;
                    p.control = Control::Demo;
                    p.pose = Pose::Jump;
                    self.coop.slots[a].state = State::Spring {
                        under: b,
                        dx: ax - bx,
                        t: 0,
                        saved_vx,
                        high: false,
                    };
                    let s = &mut self.coop.slots[b];
                    s.cooldown = STOMP_COOLDOWN;
                    s.freeze = STOMP_FREEZE;
                }
            }
        }
        // the one stood on doesn't move sideways on the ground for 4 frames
        for k in 0..n {
            if self.coop.slots[k].freeze > 0 {
                self.coop.slots[k].freeze -= 1;
                let p = self.slot_mut(k);
                if p.on_ground {
                    p.vx = 0.0;
                }
            }
        }
    }

    /// Sitting on a head for 4 frames, then the launch: 4.628 (3.828 with Jump held, which
    /// keeps the slower held-jump gravity) and half the landing speed; a jump press during the
    /// wait launches a frame later at 4.428 with no speed.
    fn step_spring(&mut self, k: usize, jump: bool) {
        let State::Spring {
            under,
            dx,
            t,
            saved_vx,
            mut high,
        } = self.coop.slots[k].state
        else {
            return;
        };
        let pressed = jump && !self.coop.slots[k].prev_jump;
        high |= pressed;
        let t = t + 1;
        if !self.in_play(under) {
            self.coop.slots[k].state = State::Play;
            self.slot_mut(k).control = Control::Normal;
            return;
        }
        let (ux, top) = (self.slot(under).x, self.slot(under).contact_box().3);
        let launch = if high {
            t > SPRING_WAIT + 1
        } else {
            t > SPRING_WAIT
        };
        let p = self.slot_mut(k);
        p.x = ux + dx;
        p.y = top;
        if !launch {
            self.coop.slots[k].state = State::Spring {
                under,
                dx,
                t,
                saved_vx,
                high,
            };
            return;
        }
        p.control = Control::Normal;
        p.on_ground = false;
        p.vy = if high {
            SPRING_VY_HIGH
        } else if jump {
            SPRING_VY_HELD
        } else {
            SPRING_VY
        };
        p.vx = if high { 0.0 } else { 0.5 * saved_vx };
        let s = &mut self.coop.slots[k];
        s.state = State::Play;
        s.ignore = Some((under, SPRING_IGNORE));
        self.events.push(Event::Jump);
    }

    /// Players side by side push each other apart (§5.2).
    fn side_push(&mut self) {
        // push factor by size class (me × other): small, big, Mega
        const F: [[f32; 3]; 3] = [[0.6, 0.6, 1.0], [0.6, 0.6, 1.0], [0.1, 0.1, 0.6]];
        let n = self.players();
        let mut moves = vec![0.0f32; n];
        let mut speeds: Vec<Option<f32>> = vec![None; n];
        for a in 0..n {
            for b in (a + 1)..n {
                if !self.touchable(a) || !self.touchable(b) || self.ignoring(a, b) {
                    continue;
                }
                let (pa, pb) = (self.slot(a), self.slot(b));
                let (ab, bb) = (pa.contact_box(), pb.contact_box());
                let oy = ab.3.min(bb.3) - ab.1.max(bb.1);
                let ox = ab.2.min(bb.2) - ab.0.max(bb.0);
                if oy < PUSH_OVERLAP || ox <= 0.0 {
                    continue;
                }
                // the one on the left goes left
                let dir = if pa.x < pb.x || (pa.x == pb.x && a < b) {
                    -1.0
                } else {
                    1.0
                };
                let (ca, cb) = (Self::size_class(pa), Self::size_class(pb));
                for (me, other, d, fm, pm, po) in [
                    (a, b, dir, F[ca][cb], pa, pb),
                    (b, a, -dir, F[cb][ca], pb, pa),
                ] {
                    let _ = other;
                    // no push while already moving that way
                    if pm.vx * d <= 0.0 {
                        moves[me] += (fm * ox * 0.5 * d).clamp(-PUSH_MAX, PUSH_MAX);
                    }
                    let (vm, vo) = (pm.vx, po.vx);
                    if vm * vo < 0.0 && vm.abs() + vo.abs() > 2.5 {
                        speeds[me] = Some(0.3 * vo);
                    } else if vo.abs() < vm.abs() && vm * d < 0.0 {
                        speeds[me] = Some(vm.clamp(-1.3, 1.3));
                    }
                }
            }
        }
        for k in 0..n {
            if let Some(v) = speeds[k] {
                self.slot_mut(k).vx = v;
            }
            let dx = moves[k].clamp(-PUSH_MAX, PUSH_MAX);
            if dx != 0.0 && !self.body_blocked(k, dx) {
                self.slot_mut(k).x += dx;
            }
        }
    }

    /// Moving slot `k` by `dx` would put its body into a wall.
    fn body_blocked(&self, k: usize, dx: f32) -> bool {
        let p = self.slot(k);
        let s = p.sensors();
        let x = p.x + dx + s.wall_x * dx.signum();
        [s.wall_y0, s.wall_y1]
            .iter()
            .any(|&wy| self.level.cell_at(x, p.y + wy).is_solid())
    }

    // ------------------------------------------------------------ the view's edges

    /// The shared view pushes players along at its sides and holds them under its top; one
    /// squeezed 16 px past a side by a wall, or 56 px below it, dies (§4.5).
    fn view_edges(&mut self) {
        let (l, b, r, t) = self.view_rect(0.0);
        for k in 0..self.players() {
            if !self.touchable(k) || self.slot(k).car.is_some() || self.slot(k).clown.is_some() {
                continue;
            }
            let p = self.slot(k);
            let (x, y) = (p.x, p.y);
            let mut nx = x;
            if x < l + EDGE && !self.body_blocked(k, l + EDGE - x) {
                nx = l + EDGE;
            } else if x > r - EDGE && !self.body_blocked(k, r - EDGE - x) {
                nx = r - EDGE;
            }
            let p = self.slot_mut(k);
            if nx != x {
                p.x = nx;
                if (p.vx < 0.0 && nx > x) || (p.vx > 0.0 && nx < x) {
                    p.vx = 0.0;
                }
            }
            if p.y > t + TOP_REACH {
                p.y = t + TOP_REACH;
                p.vy = p.vy.min(0.0);
            }
            if p.x < l - SQUEEZE || p.x > r + SQUEEZE || y < b - PIT_BELOW {
                p.star = 0;
                p.invuln = 0;
                p.kill();
                self.coop.last_down = k;
                self.events.push(Event::Death);
            }
        }
    }

    /// Time up: everyone in play dies at once (§8).
    fn time_up(&mut self) {
        if self.time != 0 {
            return;
        }
        for k in 0..self.players() {
            if self.coop.slots[k].state == State::Play && self.slot(k).alive() {
                self.slot_mut(k).kill();
                self.coop.last_down = k;
            }
        }
    }

    // ------------------------------------------------------------ goal and warps

    /// The player in `World::player` took the pole (check_goal): on it until the demo, and the
    /// goal waits for the others.
    pub(super) fn coop_goal_touch(&mut self) {
        let k = self.coop.at[0];
        self.coop.slots[k].state = State::Pole { slide: -1 };
        let g = self.coop.goal.get_or_insert_with(|| GoalWait {
            timer: GOAL_WAIT,
            ..GoalWait::default()
        });
        if !g.list.contains(&k) {
            g.list.push(k);
        }
    }

    /// Once one player is on the pole: start the demo when everyone in play is there, else
    /// after 240 frames (30 more once for a player within 16 px of the pole).
    fn goal_wait(&mut self) {
        let Some(g) = self.coop.goal.clone() else {
            return;
        };
        if g.started {
            return;
        }
        let n = self.players();
        let everyone = (0..n)
            .filter(|&k| self.in_play(k))
            .all(|k| matches!(self.coop.slots[k].state, State::Pole { .. }));
        let mut g = g;
        let mut start = everyone && g.timer > 0;
        if !start {
            g.timer = (g.timer - 1).max(0);
            if g.timer == 0 {
                let pole = self.level.goal.map_or(0.0, |(x, _)| x);
                let near =
                    (0..n).any(|k| self.free(k) && (self.slot(k).x - pole).abs() <= GOAL_NEAR);
                if near && !g.extended {
                    g.extended = true;
                    g.timer = GOAL_GRACE;
                } else {
                    start = true;
                }
            }
        }
        if start {
            g.started = true;
            self.goal_phase = GoalPhase::Hold { t: 10 };
            // the others on the pole slide after the winner, the lowest first
            let mut on_pole: Vec<usize> = g.list.iter().copied().skip(1).collect();
            on_pole.sort_by(|&a, &b| self.slot(b).y.total_cmp(&self.slot(a).y));
            let m = on_pole.len() as i32;
            for (i, k) in on_pole.into_iter().enumerate() {
                self.coop.slots[k].state = State::Pole {
                    slide: 10 + POLE_DELAY * (m - i as i32),
                };
            }
        }
        self.coop.goal = Some(g);
    }

    /// The other pole players during the demo: slide down, then walk into the castle with the
    /// winner.
    fn pole_demo(&mut self) {
        let Some((x0, y0)) = self.level.goal else {
            return;
        };
        if self.goal_phase == GoalPhase::None {
            return;
        }
        let winner_walks = matches!(
            self.goal_phase,
            GoalPhase::Walk | GoalPhase::Inside { .. } | GoalPhase::FlagUp | GoalPhase::Finish
        );
        for k in 0..self.players() {
            let State::Pole { slide } = self.coop.slots[k].state else {
                continue;
            };
            if self.coop.goal.as_ref().and_then(|g| g.list.first()) == Some(&k) {
                continue;
            }
            if slide > 0 {
                self.coop.slots[k].state = State::Pole { slide: slide - 1 };
                continue;
            }
            let mut p = self.slot(k).clone();
            if p.hidden {
                continue;
            }
            if p.y > y0 {
                p.y = (p.y - 1.9).max(y0);
                p.pose = Pose::Pole;
            } else if winner_walks {
                p.control = Control::Normal;
                p.step(
                    Input {
                        buttons: input::RIGHT,
                    },
                    &self.level,
                );
                if p.x >= x0 + 112.0 {
                    p.hidden = true;
                    p.control = Control::Demo;
                }
            } else {
                p.on_ground = true;
                p.x = x0 + 6.0;
                p.facing = -1;
                p.pose = Pose::Wait;
            }
            *self.slot_mut(k) = p;
        }
    }

    /// The leader went through a warp: the others are taken along, hidden, and come out after
    /// it 15 frames apart (bubbles and downed players too, as they are) (§7).
    fn take_along(&mut self) {
        let Some(lead) = self.coop.warp_lead else {
            return;
        };
        let mut order = 0;
        for k in 0..self.players() {
            if k == lead {
                continue;
            }
            order += 1;
            self.coop.slots[k].state = State::Follow {
                t: WARP_GAP * order,
            };
            let p = self.slot_mut(k);
            let form = p.form;
            *p = Player::new(0.0, 0.0);
            if !matches!(form, Form::Small) {
                p.form = form;
            }
            p.hidden = true;
            p.control = Control::Demo;
        }
    }

    /// Waiting to come out of the warp the leader took (counted once it is out).
    fn step_follow(&mut self, k: usize, t: i32) {
        let Some(lead) = (0..self.players()).find(|&j| self.in_play(j)) else {
            return;
        };
        if t > 1 {
            self.coop.slots[k].state = State::Follow { t: t - 1 };
            return;
        }
        let (x, y, facing) = {
            let l = self.slot(lead);
            (l.x, l.y, l.facing)
        };
        self.coop.slots[k].state = State::Play;
        let p = self.slot_mut(k);
        p.x = x;
        p.y = y;
        p.facing = facing;
        p.drawn_facing = facing;
        p.hidden = false;
        p.control = Control::Normal;
        p.on_ground = true;
    }

    // ------------------------------------------------------------ camera and drawing

    /// The shared camera over every slot (after everyone has moved).
    fn coop_camera(&mut self) {
        let goal = self.goal_phase != GoalPhase::None;
        let targets: Vec<camera::Target> = (0..self.players())
            .map(|k| {
                let p = self.slot(k);
                let activity = match self.coop.slots[k].state {
                    State::Bubble(_) | State::Rescue { .. } | State::Follow { .. } => {
                        camera::Activity::Away
                    }
                    _ if p.hidden => camera::Activity::Away,
                    _ if !p.alive() => camera::Activity::Frozen,
                    State::Pole { .. } => camera::Activity::Goal,
                    _ if goal => camera::Activity::Goal,
                    _ => camera::Activity::Play,
                };
                camera::Target {
                    x: p.x,
                    y: p.y,
                    vx: p.vx,
                    vy: p.vy,
                    on_ground: p.on_ground,
                    activity,
                }
            })
            .collect();
        if self.transit.is_none() {
            let area = self.camera_area();
            let level = &self.level;
            self.camera
                .update_players(&area, &targets, |x, y| level.cell(x, y) == Cell::Solid);
        }
        self.sync_camera();
    }

    /// The bubbles' sprites (drawn over their players).
    pub(super) fn bubble_sprites(&self, out: &mut Vec<Sprite>) {
        for s in &self.coop.slots {
            if let State::Bubble(b) = s.state {
                out.push(Self::model_sprite(
                    "M1_Object_balloon",
                    "balloon.0",
                    b.x - BUBBLE_HALF,
                    b.y + 8.0,
                    BUBBLE_HALF * 2.0,
                    BUBBLE_H,
                    false,
                    62,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Input = Input {
        buttons: input::RIGHT,
    };
    const L: Input = Input {
        buttons: input::LEFT,
    };
    const NONE: Input = Input { buttons: 0 };

    /// A flat course with a pit at tiles 0–2 (left of the start).
    fn pit_course() -> Level {
        let mut lvl = Level::flat(60);
        for x in -3..3 {
            for y in 0..2 {
                lvl.set(x, y, Cell::Empty);
            }
        }
        lvl
    }

    fn run(w: &mut World, inputs: &[Input], frames: usize) {
        for _ in 0..frames {
            w.step_players(inputs);
        }
    }

    #[test]
    fn one_player_steps_exactly_as_before() {
        let mut lvl = Level::flat(60);
        lvl.coins.push(crate::level::CoinTile { x: 9, y: 2 });
        let mut a = World::with_level(lvl);
        let mut b = a.clone();
        b.set_players(1);
        for i in 0..300 {
            let inp = Input {
                buttons: input::RIGHT | if i % 40 < 20 { input::JUMP } else { 0 },
            };
            a.step(inp);
            b.step_players(&[inp]);
        }
        assert_eq!(a.checksum(), b.checksum());
        assert!(!b.coop.active());
    }

    #[test]
    fn players_start_16_px_apart_as_mario_luigi_toad_toadette() {
        let mut w = World::with_level(Level::flat(60));
        w.set_players(4);
        assert_eq!(w.players(), 4);
        assert_eq!(w.other_chars, vec![1, 2, 3]);
        let x = w.player.x;
        for (k, o) in w.others.iter().enumerate() {
            assert_eq!(o.x, x - 16.0 * (k + 1) as f32);
        }
    }

    #[test]
    fn walking_into_a_player_pushes_them_along() {
        let mut w = World::with_level(Level::flat(60));
        w.set_players(2);
        let x1 = w.player.x;
        run(&mut w, &[NONE, R], 90);
        assert!(w.player.x > x1 + 20.0, "player 1 pushed: {}", w.player.x);
        assert!(w.others[0].x < w.player.x, "player 2 still behind");
    }

    #[test]
    fn a_death_while_another_plays_comes_back_in_a_bubble_and_pops() {
        let mut w = World::with_level(pit_course());
        w.set_players(2);
        let lives = w.lives;
        // player 2 walks left into the pit; player 1 stands
        let mut bubbled = false;
        for _ in 0..400 {
            w.step_players(&[NONE, L]);
            if matches!(w.coop.slots[1].state, State::Bubble(_)) {
                bubbled = true;
                break;
            }
        }
        assert!(bubbled, "player 2 is in a bubble");
        assert_eq!(w.lives, lives, "no life lost");
        assert!(w.player.alive() && !w.all_down());
        assert!(!w.in_play(1));
        assert_eq!(w.others[0].form, Form::Small);
        // the bubble floats to player 1 and pops on touch
        let mut popped = false;
        for _ in 0..1200 {
            w.step_players(&[NONE, NONE]);
            if w.coop.slots[1].state == State::Play {
                popped = true;
                break;
            }
        }
        assert!(popped, "the bubble reached player 1 and popped");
        assert!(w.in_play(1));
        assert_eq!(w.others[0].invuln, POP_INVULN);
        assert!(w.others[0].vy > 3.0, "jumped out");
    }

    #[test]
    fn everyone_down_costs_one_shared_life_and_restarts() {
        let mut w = World::with_level(pit_course());
        w.set_players(2);
        let lives = w.lives;
        let mut restarted = false;
        for _ in 0..800 {
            w.step_players(&[L, L]);
            if w.lives != lives {
                restarted = true;
                break;
            }
        }
        assert!(restarted);
        assert_eq!(w.lives, lives - 1, "one life for the party");
        assert!(w.in_play(0) && w.in_play(1));
        assert_eq!(w.player.x, w.level.start.0);
        assert_eq!(w.others[0].x, w.level.start.0 - SPACING);
    }

    #[test]
    fn landing_on_a_head_is_a_springboard() {
        let mut w = World::with_level(Level::flat(60));
        w.set_players(2);
        run(&mut w, &[NONE, NONE], 5);
        // player 2 drops onto player 1's head
        w.others[0].x = w.player.x;
        w.others[0].y = w.player.y + 30.0;
        w.others[0].vy = -2.0;
        w.others[0].on_ground = false;
        let mut sat = 0;
        let mut launched = None;
        for _ in 0..20 {
            w.step_players(&[NONE, NONE]);
            if matches!(w.coop.slots[1].state, State::Spring { .. }) {
                sat += 1;
            } else if sat > 0 && launched.is_none() {
                launched = Some(w.others[0].vy);
            }
        }
        assert!(sat >= 4, "sat on the head ({sat} frames)");
        let vy = launched.expect("launched");
        assert!(vy > 4.0, "the springboard launch: {vy}");
        assert!(w.player.alive(), "nobody is hurt");
    }

    #[test]
    fn the_goal_waits_for_the_others_then_one_at_the_pole_clears() {
        let mut lvl = Level::flat(80);
        lvl.set(20, 2, Cell::Solid);
        lvl.goal = Some((20.0 * TILE + 8.0, 3.0 * TILE));
        let mut w = World::with_level(lvl);
        w.set_players(2);
        // player 1 runs to the pole; player 2 stands far behind (no push: it's behind)
        let mut touched_at = None;
        for i in 0..2000 {
            let b = input::RIGHT | if i % 30 < 20 { input::JUMP } else { 0 };
            let p1 = if touched_at.is_none() {
                Input { buttons: b }
            } else {
                NONE
            };
            w.step_players(&[p1, NONE]);
            if touched_at.is_none() && w.coop.goal.is_some() {
                touched_at = Some(i);
                assert_eq!(w.goal_phase, GoalPhase::None, "the demo waits");
            }
            if w.goal_phase != GoalPhase::None {
                let t = touched_at.unwrap();
                assert!(i - t >= 200, "waited {} frames", i - t);
                break;
            }
        }
        assert!(touched_at.is_some());
        run(&mut w, &[NONE, NONE], 1500);
        assert!(w.cleared, "one player at the goal clears the course");
    }

    #[test]
    fn the_shared_camera_keeps_both_players_in_view() {
        let mut w = World::with_level(Level::flat(120));
        w.set_players(2);
        // player 1 runs right, player 2 stays: the view edge drags player 2 along
        run(
            &mut w,
            &[
                Input {
                    buttons: input::RIGHT | input::RUN,
                },
                NONE,
            ],
            400,
        );
        let (l, r) = (w.camera_x, w.camera_x + VIEW_W);
        for p in std::iter::once(&w.player).chain(&w.others) {
            assert!(p.x >= l && p.x <= r, "{} in {l}..{r}", p.x);
        }
        assert!(
            w.others[0].x > 100.0,
            "player 2 pushed by the view's left edge"
        );
    }

    #[test]
    fn co_op_replays_deterministically() {
        let mut w = World::with_level(pit_course());
        w.set_players(3);
        let script = |f: usize, k: usize| Input {
            buttons: match (f / 60 + k) % 3 {
                0 => input::LEFT,
                1 => input::RIGHT | input::JUMP,
                _ => 0,
            },
        };
        run(&mut w, &[NONE, NONE, NONE], 10);
        let mut copy = w.clone();
        for f in 0..600 {
            let i = [script(f, 0), script(f, 1), script(f, 2)];
            w.step_players(&i);
            copy.step_players(&i);
        }
        assert_eq!(w.checksum(), copy.checksum());
    }
}
