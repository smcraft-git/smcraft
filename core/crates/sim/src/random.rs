//! The course's random numbers (research/specs/random.md).
//!
//! The game keeps one generator per play scene (the area scene's +0x100 → +8) and every actor
//! draws from it in update order, so a draw's place in the stream matters as much as what is
//! drawn. It is a 4-word xorshift128; the play scene seeds it from 0 when the course starts
//! [obs v3.0.3 0x7100e489c0, Ryujinx traces: the state read each frame is init(0) plus the
//! frame's draws]. A constant seed makes a run a function of the course and the inputs, as
//! replays and rollback need.
//!
//! The world owns the state ([`Random`] in `World`, carried by its snapshots) and lends it for
//! the length of a tick ([`lend`]); actor code draws through [`below`] / [`unit`], which use the
//! lent stream. Code running outside a tick (unit tests of a single actor, the editor) gets the
//! caller's fallback state instead, so it stays deterministic too.

use std::cell::{Cell, RefCell};
use std::panic::Location;

/// The game's seed for the play scene's generator (the area init re-seeds it with 0).
pub const COURSE_SEED: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Random {
    s: [u32; 4],
    /// Words drawn since the seeding (for traces).
    draws: u32,
}

impl Random {
    /// The seeding from one word (0x710029a0b0): s[i] = 0x6c078965 · (p ^ (p >> 30)) + i + 1,
    /// p the previous word (the seed for s[0]).
    pub fn new(seed: u32) -> Self {
        let mut s = [0; 4];
        let mut p = seed;
        for (i, w) in s.iter_mut().enumerate() {
            p = (p ^ (p >> 30))
                .wrapping_mul(0x6c07_8965)
                .wrapping_add(i as u32 + 1);
            *w = p;
        }
        Self { s, draws: 0 }
    }

    pub fn course() -> Self {
        Self::new(COURSE_SEED)
    }

    /// The next word (0x710029a150).
    pub fn next_u32(&mut self) -> u32 {
        let s = &mut self.s;
        let t = s[0] ^ (s[0] << 11);
        let w = s[3];
        let n = t ^ (t >> 8) ^ w ^ (w >> 19);
        *s = [s[1], s[2], w, n];
        self.draws = self.draws.wrapping_add(1);
        n
    }

    /// 0 ≤ r < n as the game's callers scale a word: (word × n) >> 32 (the power-of-two
    /// callers' top bits are the same number).
    pub fn below(&mut self, n: u32) -> u32 {
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }

    /// [0, 1) from the word's top 23 bits as a float's mantissa (word >> 9 | 1.0's bits) − 1.
    pub fn unit(&mut self) -> f32 {
        f32::from_bits(self.next_u32() >> 9 | 0x3f80_0000) - 1.0
    }

    pub fn words(&self) -> [u32; 4] {
        self.s
    }

    pub fn draws(&self) -> u32 {
        self.draws
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::course()
    }
}

thread_local! {
    static LENT: Cell<Option<Random>> = const { Cell::new(None) };
    /// Draw log for traces ([`log_draws`]): the stream's count after each draw and its caller.
    static LOG: RefCell<Option<Vec<(u32, &'static Location<'static>)>>> = const { RefCell::new(None) };
}

/// Starts (true) or stops logging the lent stream's draws (`mmk trace --draws`).
pub fn log_draws(on: bool) {
    LOG.with(|l| *l.borrow_mut() = on.then(Vec::new));
}

/// The draws logged since the last call: (draws after it, caller).
pub fn take_log() -> Vec<(u32, &'static Location<'static>)> {
    LOG.with(|l| {
        l.borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

/// Runs `f` with `r` as the stream [`below`] / [`unit`] draw from, then stores it back.
pub fn lend<T>(r: &mut Random, f: impl FnOnce() -> T) -> T {
    let outer = LENT.with(|l| l.replace(Some(*r)));
    let out = f();
    if let Some(back) = LENT.with(|l| l.replace(outer)) {
        *r = back;
    }
    out
}

/// Draws from the lent stream, or from `own` when nothing is lent (the world's own draws).
#[track_caller]
pub fn with<T>(own: &mut Random, f: impl FnOnce(&mut Random) -> T) -> T {
    let at = Location::caller();
    let mut f = Some(f);
    if let Some(out) = draw(at, |r| (f.take().unwrap())(r)) {
        return out;
    }
    (f.take().unwrap())(own)
}

/// Replaces the stream: the lent one inside a tick (and `own`, which the tick's end overwrites).
pub fn reset(own: &mut Random, to: Random) {
    *own = to;
    LENT.with(|l| {
        if l.take().is_some() {
            l.set(Some(to));
        }
    });
}

/// A draw 0 ≤ r < n from the lent stream; None outside a tick.
#[track_caller]
pub fn try_below(n: u32) -> Option<u32> {
    draw(Location::caller(), |r| r.below(n))
}

/// A draw in [0, 1) from the lent stream; None outside a tick.
#[track_caller]
pub fn try_unit() -> Option<f32> {
    draw(Location::caller(), |r| r.unit())
}

/// A raw word from the lent stream; None outside a tick.
#[track_caller]
pub fn try_word() -> Option<u32> {
    draw(Location::caller(), |r| r.next_u32())
}

fn draw<T>(at: &'static Location<'static>, f: impl FnOnce(&mut Random) -> T) -> Option<T> {
    LENT.with(|l| {
        let mut r = l.take()?;
        let out = f(&mut r);
        let n = r.draws;
        l.set(Some(r));
        LOG.with(|g| {
            if let Some(v) = g.borrow_mut().as_mut() {
                v.push((n, at));
            }
        });
        Some(out)
    })
}

/// Whether a tick's stream is lent (the world is stepping).
pub fn lent() -> bool {
    peek().is_some()
}

/// The lent stream (for traces); None outside a tick.
pub fn peek() -> Option<Random> {
    LENT.with(|l| {
        let r = l.take();
        l.set(r);
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeding_matches_the_game() {
        // init(0) is the 4-word default the game's 4-word setter falls back to (0x710029a100)
        assert_eq!(
            Random::new(0).words(),
            [1, 0x6c07_8967, 0x714a_cb41, 0x4807_7044]
        );
        let mut r = Random::new(0);
        assert_eq!(r.next_u32(), 0x4807_714d);
        assert_eq!(r.next_u32(), 0x181b_859b);
    }

    #[test]
    fn scaling() {
        let mut a = Random::new(0);
        let w = {
            let mut c = a;
            c.next_u32()
        };
        assert_eq!(a.below(4), w >> 30);
        let mut b = Random::new(7);
        let w = {
            let mut c = b;
            c.next_u32()
        };
        let u = b.unit();
        assert!((0.0..1.0).contains(&u));
        assert_eq!(u, f32::from_bits(w >> 9 | 0x3f80_0000) - 1.0);
    }

    #[test]
    fn lending_shares_one_stream() {
        let mut world = Random::new(0);
        let mut own = Random::new(99);
        let mut expect = Random::new(0);
        let (x, y) = lend(&mut world, || {
            (
                with(&mut own, |r| r.next_u32()),
                with(&mut own, |r| r.next_u32()),
            )
        });
        assert_eq!((x, y), (expect.next_u32(), expect.next_u32()));
        assert_eq!(world, expect);
        assert_eq!(own, Random::new(99));
        assert!(!lent());
    }
}
