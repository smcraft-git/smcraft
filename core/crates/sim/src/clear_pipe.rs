//! Clear pipes in play (part 93, 3D World): the traveller that carries players, enemies and
//! items along a pipe's path, and the tests that put them in (objects/93-clear-pipe.md).
//!
//! All of it is the game's own helper, one per rider (vtable 0x7102852ff8: reset 0x7100d866e0,
//! start 0x7100d86720, start inside 0x7100d86ca0, update 0x7100d86d90; the reverse 0x7100d87a50,
//! the straight step 0x7100d8a120) [obs v3.0.3]: the player's lives at +0x2ad0, an enemy's is
//! allocated in the enemy init (0x7101286ee0, +0xb28). The paths are the area's point lists
//! (level/clear_pipe.rs `path_points`); the traveller heads for one point at a time, the
//! *target*, coming from the one behind it. Units: px, px/frame, y up; angles are the game's
//! 32-bit binary angles.

use crate::fmath::table_sin_cos;

/// Base travel speed, px/frame: the reset's +0x14, the same for players, enemies and items
/// (nothing that owns a traveller changes it) [obs v3.0.3 0x7100d866e0, 0x7101286ee0,
/// 0x7100d85af0].
pub const SPEED: f32 = 3.0;
/// A bend's turn per frame: 10° (0x071c71c8), so a quarter bend takes 9 frames [obs: reset].
pub const BEND_STEP: u32 = 0x071c_71c8;
const QUARTER: u32 = 0x4000_0000;
/// How far past an open end the traveller goes before it lets go (8 px; an upward exit goes
/// only the rider's −offset y past it) [obs 0x7100d86d90].
pub const OVERSHOOT: f32 = 8.0;
/// Blocked: the pause eases the scale +0x58 down by 0.12 a frame to 0.7, then the traveller
/// turns round and eases it back up by 0.16 a frame to 1.0 before moving again [obs
/// 0x7100d86d90: DAT 0x7102318a50, 0x7102318b94, 0x7102318f78].
const PAUSE_DOWN: f32 = -0.12;
const PAUSE_LOW: f32 = 0.7;
const PAUSE_UP: f32 = 0.16;
/// Two coordinates are "the same" within 1e-5 [obs DAT 0x7102318cf4].
const EPS: f32 = 1e-5;

/// Which way a side faces or a rider moves: the game's side codes (0 right, 1 left, 2 up,
/// 3 down) [obs: the entry finder 0x7100e5a8a0 and the block test's side].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Up,
    Down,
}

impl Side {
    pub fn vector(self) -> (f32, f32) {
        match self {
            Side::Right => (1.0, 0.0),
            Side::Left => (-1.0, 0.0),
            Side::Up => (0.0, 1.0),
            Side::Down => (0.0, -1.0),
        }
    }
}

/// How the traveller is moving (+0x44): along a straight segment, or round a bend entered from
/// a vertical (4) or a horizontal (5) segment [obs].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    Up,
    Down,
    Left,
    Right,
    BendFromVertical,
    BendFromHorizontal,
}

impl Leg {
    pub fn vertical(self) -> bool {
        matches!(self, Leg::Up | Leg::Down)
    }
    pub fn horizontal(self) -> bool {
        matches!(self, Leg::Left | Leg::Right)
    }
}

/// The area's clear-pipe paths (slot order) and the course's game version, which a few rules
/// depend on (the course header's +0x14, `DAT_7102a692f8 + 0x28` in play).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paths {
    pub pipes: Vec<Vec<(f32, f32)>>,
    pub version: u32,
}

/// One traveller (the helper's fields named by use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traveller {
    /// Path index (+0x40).
    pub pipe: usize,
    /// The target point (+0x08) and the path's point count (+0x0c).
    pub i: usize,
    pub n: usize,
    /// Reversal count (+0x10).
    pub reversals: u32,
    pub speed: f32,
    /// The rider's offset from the path (+0x18): the player's (0, −0.1), an enemy's
    /// (0, offset y).
    pub off: (f32, f32),
    /// Where it is on the path (+0x20).
    pub x: f32,
    pub y: f32,
    /// Its heading: a unit vector on straight segments, the last frame's move in a bend (+0x28).
    pub dx: f32,
    pub dy: f32,
    /// Heading toward lower point indices (+0x30).
    pub back: bool,
    /// Out of the far end (+0x31).
    pub done: bool,
    pub leg: Option<Leg>,
    /// A bend just finished (+0x48), and the junction scan of the end segment done (+0x49).
    arrived: bool,
    joined: bool,
    /// Progress round the bend (+0x4c) and its step (+0x50).
    pub angle: u32,
    pub step: u32,
    /// Blocked: paused (+0x54), still slowing before the turn (+0x55), the scale (+0x58).
    pub paused: bool,
    pub slowing: bool,
    pub scale: f32,
}

impl Default for Traveller {
    fn default() -> Self {
        Self::new()
    }
}

fn hyp(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn unit(x: f32, y: f32) -> (f32, f32) {
    let d = hyp(x, y);
    if d > 0.0 { (x / d, y / d) } else { (x, y) }
}

/// A point of a path the game's way: an index outside the 72-point table reads point 0.
fn at(pts: &[(f32, f32)], k: isize) -> (f32, f32) {
    if (0..72).contains(&k) {
        pts.get(k as usize).copied().unwrap_or((0.0, 0.0))
    } else {
        pts.first().copied().unwrap_or((0.0, 0.0))
    }
}

/// The leg from `prev` to `cur` (`old`: the leg before, which picks a bend's kind).
fn leg_of(cur: (f32, f32), prev: (f32, f32), old: Option<Leg>) -> Option<Leg> {
    let dx = prev.0 - cur.0;
    let dy = prev.1 - cur.1;
    if dx.abs() > EPS {
        if dy.abs() > EPS {
            match old {
                Some(l) if l.horizontal() => Some(Leg::BendFromHorizontal),
                Some(l) if l.vertical() => Some(Leg::BendFromVertical),
                other => other,
            }
        } else if cur.0 <= prev.0 {
            Some(Leg::Left)
        } else {
            Some(Leg::Right)
        }
    } else if prev.1 < cur.1 {
        Some(Leg::Up)
    } else {
        Some(Leg::Down)
    }
}

fn within(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// A path (other than `not`) with an open end at `p`: its index and whether it is its last
/// point (first ends are found first, paths in slot order) [obs: the junction scan in
/// 0x7100d86d90, the same in 0x7100d87a50].
fn joined_at(paths: &Paths, p: (f32, f32), not: usize) -> Option<(usize, bool)> {
    paths.pipes.iter().enumerate().find_map(|(k, pts)| {
        if k == not || pts.is_empty() {
            return None;
        }
        if pts[0] == p {
            Some((k, false))
        } else if pts[pts.len() - 1] == p {
            Some((k, true))
        } else {
            None
        }
    })
}

impl Traveller {
    /// The reset (vtable slot 0) [obs 0x7100d866e0].
    pub fn new() -> Self {
        Self {
            pipe: 0,
            i: 0,
            n: 0,
            reversals: 0,
            speed: SPEED,
            off: (0.0, 0.0),
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
            back: false,
            done: true,
            leg: None,
            arrived: false,
            joined: false,
            angle: 0,
            step: BEND_STEP,
            paused: false,
            slowing: false,
            scale: 1.0,
        }
    }

    /// Ready to ride path `pipe` in from its first point, or from its last (`back`): the
    /// entry tests' set-up (0x71015aa8c0, 0x7101055e90) then the start (vtable slot 1,
    /// 0x7100d86720): at the mouth, heading for the next point.
    pub fn enter(paths: &Paths, pipe: usize, back: bool, off: (f32, f32)) -> Self {
        let pts = &paths.pipes[pipe];
        let mut t = Self::new();
        t.pipe = pipe;
        t.n = pts.len();
        t.off = off;
        // the set-up flips the direction of a fresh traveller (+0x31 set) without counting it
        t.back = back;
        (t.x, t.y) = if back { pts[t.n - 1] } else { pts[0] };
        t.i = if back { t.n - 2 } else { 1 };
        t.done = false;
        t.relegs(pts);
        t
    }

    fn prev_index(&self) -> isize {
        self.i as isize + if self.back { 1 } else { -1 }
    }

    /// The leg between the target and the point behind it.
    fn relegs(&mut self, pts: &[(f32, f32)]) {
        let cur = at(pts, self.i as isize);
        let prev = at(pts, self.prev_index());
        self.leg = leg_of(cur, prev, self.leg);
    }

    /// The rider's position (path point plus offset).
    pub fn pos(&self) -> (f32, f32) {
        (self.x + self.off.0, self.y + self.off.1)
    }

    /// The heading as a side (the block test's code): left when dx < 0, right otherwise, up or
    /// down when dx is 0 [obs 0x7100d86d90].
    pub fn side(&self) -> Side {
        if self.dx < 0.0 {
            Side::Left
        } else if self.dx == 0.0 {
            if self.dy <= 0.0 { Side::Down } else { Side::Up }
        } else {
            Side::Right
        }
    }

    /// The straight step toward the target (0x7100d8a120): at most `speed`; on arrival the
    /// next point becomes the target, or the traveller is done at a path's end.
    fn step_straight(&mut self, pts: &[(f32, f32)]) {
        let cur = at(pts, self.i as isize);
        (self.dx, self.dy) = unit(cur.0 - self.x, cur.1 - self.y);
        let (ex, ey) = (cur.0 - self.x, cur.1 - self.y);
        let d = hyp(ex, ey);
        if self.speed < d {
            self.x += self.speed * ex * (1.0 / d);
            self.y += self.speed * ey * (1.0 / d);
            return;
        }
        (self.x, self.y) = cur;
        if self.back {
            if self.i == 0 {
                self.done = true;
                return;
            }
            self.i -= 1;
        } else {
            if self.i + 1 == self.n {
                self.done = true;
                return;
            }
            self.i += 1;
        }
    }

    /// Switch to path `k` at a junction, heading in from its first point (`last` false) or its
    /// last, where the traveller already is; the start inside (vtable slot 2, 0x7100d86ca0)
    /// makes the second point the target.
    fn switch_to(&mut self, paths: &Paths, k: usize, last: bool) {
        let pts = &paths.pipes[k];
        self.pipe = k;
        self.n = pts.len();
        self.joined = false;
        self.back = last;
        self.done = false;
        self.i = if last { self.n - 2 } else { 1 };
        self.relegs(pts);
    }

    /// One frame (vtable slot 3, 0x7100d86d90). `blocked(point, side)`: whether something
    /// stops the way out past `point` toward `side` (the owner's block test; it is asked once
    /// the traveller is within its speed + 8 px of an end).
    pub fn update(&mut self, paths: &Paths, blocked: impl Fn((f32, f32), Side) -> bool) {
        if self.paused {
            if self.slowing {
                self.scale += PAUSE_DOWN;
                if PAUSE_LOW <= self.scale {
                    return;
                }
                self.scale = PAUSE_LOW;
                self.slowing = false;
                self.reverse(paths);
                self.dx = -self.dx;
                self.dy = -self.dy;
                return;
            }
            self.scale += PAUSE_UP;
            if self.scale < 1.0 {
                return;
            }
            self.scale = 1.0;
            self.paused = false;
        }
        let Some(pts) = paths.pipes.get(self.pipe) else {
            self.done = true;
            return;
        };
        let cur = at(pts, self.i as isize);
        let prev = at(pts, self.prev_index());
        // round a bend: an arc of radius 16 about the corner, the angle +0x4c going from 0 to
        // 90° by `step` a frame; entered from a horizontal segment the arc's centre is
        // (prev.x, cur.y), from a vertical one (cur.x, prev.y)
        if (prev.0 - cur.0).abs() > EPS && (prev.1 - cur.1).abs() > EPS {
            if self.arrived {
                // two bends in a row: the second turns the other way round
                self.leg = Some(if self.leg == Some(Leg::BendFromVertical) {
                    Leg::BendFromHorizontal
                } else {
                    Leg::BendFromVertical
                });
                self.arrived = false;
            }
            (self.dx, self.dy) = unit(cur.0 - prev.0, cur.1 - prev.1);
            let left = QUARTER - self.angle;
            if self.step < left {
                self.angle += self.step;
            } else {
                self.angle = 0;
                self.i = if self.back { self.i - 1 } else { self.i + 1 };
                self.dx = cur.0 - self.x;
                self.dy = cur.1 - self.y;
                (self.x, self.y) = cur;
                self.arrived = true;
                return;
            }
            let (s, c) = table_sin_cos(self.angle);
            let (nx, ny) = if self.leg == Some(Leg::BendFromHorizontal) {
                (
                    (cur.0 - prev.0) * s + prev.0,
                    (cur.1 - prev.1) + prev.1 + (prev.1 - cur.1) * c,
                )
            } else {
                (
                    (cur.0 - prev.0) + prev.0 + (prev.0 - cur.0) * c,
                    (cur.1 - prev.1) * s + prev.1,
                )
            };
            self.dx = nx - self.x;
            self.dy = ny - self.y;
            (self.x, self.y) = (nx, ny);
            return;
        }
        // a middle point: the straight step, then the leg toward the new target
        if self.i != 0 && self.i + 1 != self.n {
            self.step_straight(pts);
            self.relegs(pts);
            self.arrived = false;
            return;
        }
        // the end segment: out through the mouth, unless another pipe's end meets this one
        self.arrived = false;
        self.leg = leg_of(cur, prev, self.leg);
        (self.dx, self.dy) = unit(cur.0 - prev.0, cur.1 - prev.1);
        if !self.joined && within((self.x, self.y), prev, cur) {
            self.joined = true;
            if let Some((k, last)) = joined_at(paths, cur, self.pipe) {
                let (x, y) = (self.x, self.y);
                self.switch_to(paths, k, last);
                (self.x, self.y) = (x, y);
                self.step_straight(&paths.pipes[k]);
                return;
            }
        }
        let past = if self.leg == Some(Leg::Up) {
            -self.off.1
        } else {
            OVERSHOOT
        };
        let (tx, ty) = (cur.0 + self.dx * past, cur.1 + self.dy * past);
        let (ex, ey) = (tx - self.x, ty - self.y);
        let d = hyp(ex, ey);
        if self.speed < d {
            self.x += self.speed * ex * (1.0 / d);
            self.y += self.speed * ey * (1.0 / d);
        } else {
            (self.x, self.y) = (tx, ty);
            self.done = true;
        }
        let near = hyp(self.x - cur.0, self.y - cur.1);
        if near < self.speed + 8.0 && blocked(cur, self.side()) {
            self.paused = true;
            self.slowing = true;
        }
    }

    /// Turn round (0x7100d87a50): the point behind becomes the target; if the traveller is not
    /// on that segment of this path (it went on into a joined pipe), it goes back into the pipe
    /// whose end is at the new target. The game resets the traveller first in that case, which
    /// also clears the rider's offset and the reversal count [obs].
    pub fn reverse(&mut self, paths: &Paths) {
        let was_forward = !self.back;
        self.back = was_forward;
        let i = self.i as isize + if was_forward { -1 } else { 1 };
        if !self.done {
            self.reversals += 1;
        }
        self.i = i.clamp(0, self.n as isize - 1) as usize;
        self.done = false;
        self.joined = false;
        let Some(pts) = paths.pipes.get(self.pipe) else {
            return;
        };
        let cur = at(pts, self.i as isize);
        let other = at(pts, self.i as isize + if was_forward { 1 } else { -1 });
        if within((self.x, self.y), cur, other) {
            return;
        }
        if let Some((k, last)) = joined_at(paths, cur, self.pipe) {
            let (x, y, reversals) = (self.x, self.y, self.reversals);
            *self = Self::new();
            self.switch_to(paths, k, last);
            self.reversals = reversals;
            (self.x, self.y) = (x, y);
        }
    }
}

/// The entry finder (0x7100e5a8a0) [obs v3.0.3]: a path in this area with an open end within
/// `radius` of `probe` whose pipe leads on in the rider's direction `side` (the next point
/// lies that way); `clear(mouth, side)` is the owner's test that the mouth is not covered
/// (0x7100d83600). The result: the path and whether it is entered from its last point.
pub fn find_entrance(
    paths: &Paths,
    probe: (f32, f32),
    side: Side,
    radius: f32,
    clear: impl Fn((f32, f32), Side) -> bool,
) -> Option<(usize, bool)> {
    let leads = |from: (f32, f32), next: (f32, f32)| match side {
        Side::Right => next.0 - from.0 > 0.0,
        Side::Left => next.0 - from.0 < 0.0,
        Side::Up => next.1 - from.1 > 0.0,
        Side::Down => next.1 - from.1 < 0.0,
    };
    paths.pipes.iter().enumerate().find_map(|(k, pts)| {
        let n = pts.len();
        if n < 2 {
            return None;
        }
        let near = |p: (f32, f32)| hyp(p.0 - probe.0, p.1 - probe.1) < radius;
        if near(pts[0]) {
            return (leads(pts[0], pts[1]) && clear(pts[0], side)).then_some((k, false));
        }
        if near(pts[n - 1]) && leads(pts[n - 1], pts[n - 2]) && clear(pts[n - 1], side) {
            return Some((k, true));
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(pipes: Vec<Vec<(f32, f32)>>) -> Paths {
        Paths { pipes, version: 1 }
    }

    fn ride(p: &Paths, t: &mut Traveller, blocked: impl Fn((f32, f32), Side) -> bool + Copy) {
        for _ in 0..1000 {
            if t.done {
                return;
            }
            t.update(p, blocked);
        }
        panic!("never came out");
    }

    #[test]
    fn a_straight_pipe_carries_at_three_px_and_lets_go_eight_past() {
        // left mouth at x 160, right mouth at 320 (a 10-tile run going right)
        let p = paths(vec![vec![(160.0, 96.0), (320.0, 96.0)]]);
        let mut t = Traveller::enter(&p, 0, false, (0.0, -0.1));
        assert_eq!((t.x, t.i, t.leg), (160.0, 1, Some(Leg::Right)));
        let mut frames = 0;
        while !t.done {
            t.update(&p, |_, _| false);
            frames += 1;
            assert!(frames < 200);
        }
        // 160 px to the mouth + 8 past at 3 a frame: 56 frames
        assert_eq!(frames, 56);
        assert_eq!((t.x, t.y), (328.0, 96.0));
        assert_eq!(t.side(), Side::Right);
    }

    #[test]
    fn a_bend_is_a_quarter_circle_in_nine_frames() {
        // right 32 px, bend right→up about (192, 112), up 32 px
        let p = paths(vec![vec![
            (144.0, 96.0),
            (176.0, 96.0),
            (192.0, 112.0),
            (192.0, 144.0),
        ]]);
        let mut t = Traveller::enter(&p, 0, false, (0.0, 0.0));
        // 32 px straight: 11 frames (the last one short)
        for _ in 0..11 {
            t.update(&p, |_, _| false);
        }
        assert_eq!((t.x, t.y, t.i), (176.0, 96.0, 2));
        assert_eq!(t.leg, Some(Leg::BendFromHorizontal));
        let mut on_arc = Vec::new();
        for _ in 0..9 {
            t.update(&p, |_, _| false);
            on_arc.push((t.x, t.y));
        }
        // every point of the arc 16 px from its centre (176, 112) (the table sine's
        // interpolation is good to ~1e-3 px)
        for &(x, y) in &on_arc[..8] {
            assert!((hyp(x - 176.0, y - 112.0) - 16.0).abs() < 1e-2, "{x} {y}");
        }
        assert_eq!(on_arc[8], (192.0, 112.0));
        assert_eq!(t.i, 3);
        ride(&p, &mut t, |_, _| false);
        // an upward exit lets go at the mouth (−offset y = 0 past it)
        assert_eq!((t.x, t.y), (192.0, 144.0));
    }

    #[test]
    fn a_blocked_exit_pauses_and_turns_back() {
        let p = paths(vec![vec![(160.0, 96.0), (256.0, 96.0)]]);
        let mut t = Traveller::enter(&p, 0, false, (0.0, 0.0));
        let block = |pt: (f32, f32), s: Side| pt == (256.0, 96.0) && s == Side::Right;
        let mut frames = 0;
        while !t.paused {
            t.update(&p, block);
            frames += 1;
        }
        // stopped within speed + 8 of the end: 86 px out of 96 (29 frames: 87 > 85)
        assert!(t.x >= 256.0 - 11.0 && t.x < 256.0, "{}", t.x);
        let at_stop = t.x;
        // 0.88, 0.76, then below 0.7: the turn on the third frame, with no move
        for _ in 0..3 {
            t.update(&p, block);
            assert_eq!(t.x, at_stop);
        }
        assert!(t.back && !t.slowing && t.paused);
        assert_eq!((t.i, t.reversals), (0, 1));
        // eases up 0.86, 1.02: moves again on the second frame, back the way it came
        t.update(&p, block);
        assert_eq!(t.x, at_stop);
        t.update(&p, block);
        assert!(!t.paused && t.x < at_stop);
        ride(&p, &mut t, block);
        assert_eq!((t.x, t.side()), (152.0, Side::Left));
        assert!(frames > 0);
    }

    #[test]
    fn a_junction_carries_on_into_the_joined_pipe() {
        // pipe 0 ends at (256, 96) where pipe 1 ends too (drawn the other way)
        let p = paths(vec![
            vec![(160.0, 96.0), (256.0, 96.0)],
            vec![(352.0, 96.0), (256.0, 96.0)],
        ]);
        let mut t = Traveller::enter(&p, 0, false, (0.0, 0.0));
        t.update(&p, |_, _| false);
        // on the end segment the scan finds pipe 1's last point: ridden backward
        assert_eq!((t.pipe, t.back, t.i), (1, true, 0));
        ride(&p, &mut t, |_, _| false);
        assert_eq!((t.x, t.y), (360.0, 96.0));
    }

    #[test]
    fn entrances_lead_in_from_either_end() {
        let p = paths(vec![vec![(160.0, 96.0), (320.0, 96.0)]]);
        let free = |_: (f32, f32), _: Side| true;
        // moving right into the left mouth
        assert_eq!(
            find_entrance(&p, (161.0, 100.0), Side::Right, 7.89, free),
            Some((0, false))
        );
        // moving left into the right mouth: entered from the last point
        assert_eq!(
            find_entrance(&p, (319.0, 100.0), Side::Left, 7.89, free),
            Some((0, true))
        );
        // the wrong way, too far, or covered
        assert_eq!(
            find_entrance(&p, (161.0, 100.0), Side::Left, 7.89, free),
            None
        );
        assert_eq!(
            find_entrance(&p, (161.0, 106.0), Side::Right, 7.89, free),
            None
        );
        assert_eq!(
            find_entrance(&p, (161.0, 100.0), Side::Right, 7.89, |_, _| false),
            None
        );
    }
}
