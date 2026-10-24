//! The area's projectile budget [obs v3.0.3]: one manager per area (area +0x108, built in the
//! area's ctor at 0x7100e439d4: 0x678 bytes, a 200-entry request ring inline) caps what the
//! throwers have reserved at 100 units. A thrower asks for its units before it throws (request
//! 0x7100de9960; the thrower's own +0x348 units / +0x34c state: 0 none, 1 queued, 2 pending,
//! 3 held):
//! - held already → granted; pending (promoted from the queue) → held, granted;
//! - none: refused and queued when another request is already waiting, or when held (+0x10) +
//!   the units + pending (+0x670) + the manager's +0x8 would pass 100; else held, granted;
//! - queued → refused until promoted.
//!
//! The area update (0x7100e443c0) promotes the queue's front to pending while it fits. A
//! reservation lasts until the thrower lets it go (release 0x7100de9a60: the state exits that
//! drop a held projectile, the WingWalk exit 0x7101096c50, the Fire clown car's Chase exit
//! 0x710118d070 …) or is deleted (0x71008d8530) — not after each throw: a Hammer Bro's timed
//! throw helper (0x7101098560) asks again each time and is granted at once while it holds its
//! unit. So the budget is a cap on the throwers acting at once: 100 Hammer Bros throw, the 101st
//! waits for one of them to go. The manager's +0x8 has no writer but the ctor and the area's
//! reset (0x7100e4b8c8), which zero it: always 0.
//!
//! The engine keeps the queue and the totals in the world (`ThrowBudget`) and each actor's
//! reservation in `Actor::budget`; the world shows an actor the manager's totals before it
//! steps and settles its change after, so requests within a frame are taken in actor order as
//! the game's are. [inf: the promotion runs before the actors' updates; a reservation of a
//! thrower knocked out (Stomped / DieFall) is let go at once.]

use std::collections::VecDeque;

use super::Actor;

/// The units the throwers may hold at once [obs 0x7100de9960 / 0x7100e443c0].
pub const LIMIT: i32 = 100;
/// The request ring's size (+0x20 = 200, set by the area ctor 0x7100e439d4) [obs].
pub const QUEUE: usize = 200;

/// A reservation's state (+0x34c).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hold {
    #[default]
    None,
    Queued,
    Pending,
    Held,
}

/// A thrower's reservation, with the manager's totals as the world showed them for this step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reserve {
    pub units: u8,
    pub hold: Hold,
    /// Held + pending units of the area (+0x10 + +0x670), and whether a request is waiting.
    pub used: i32,
    pub waiting: bool,
}

impl Reserve {
    /// The units this reservation counts in the manager's totals.
    fn counted(&self) -> i32 {
        match self.hold {
            Hold::Pending | Hold::Held => self.units as i32,
            _ => 0,
        }
    }
}

impl Actor {
    /// Ask the area's budget for `units` before a throw (0x7100de9960): true when granted.
    pub(crate) fn budget_request(&mut self, units: u8) -> bool {
        let r = &mut self.budget;
        match r.hold {
            Hold::Held => true,
            Hold::Pending => {
                r.hold = Hold::Held;
                true
            }
            Hold::Queued => false,
            Hold::None => {
                r.units = units;
                if r.waiting || r.used + units as i32 > LIMIT {
                    r.hold = Hold::Queued;
                    false
                } else {
                    r.hold = Hold::Held;
                    r.used += units as i32;
                    true
                }
            }
        }
    }

    /// Let the reservation go (0x7100de9a60).
    pub(crate) fn budget_release(&mut self) {
        self.budget.units = 0;
        self.budget.hold = Hold::None;
    }
}

/// The area's manager: the request ring (actor ids) and the held + pending total.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThrowBudget {
    queue: VecDeque<u32>,
    used: i32,
}

impl ThrowBudget {
    /// Before the actors' updates: knocked-out or gone throwers let their units go
    /// (0x71008d8530), and the area update (0x7100e443c0) moves waiting requests to pending
    /// while held + pending + their units stay within the limit.
    pub fn begin_frame(&mut self, actors: &mut [Actor]) {
        for a in actors.iter_mut() {
            if !a.alive() && a.budget.hold != Hold::None {
                a.budget_release();
            }
        }
        self.used = actors.iter().map(|a| a.budget.counted()).sum();
        self.queue.retain(|&id| {
            actors
                .iter()
                .any(|a| a.id == id && a.budget.hold == Hold::Queued)
        });
        while let Some(&id) = self.queue.front() {
            let Some(a) = actors.iter_mut().find(|a| a.id == id) else {
                self.queue.pop_front();
                continue;
            };
            let units = a.budget.units as i32;
            if self.used + units > LIMIT {
                break;
            }
            self.used += units;
            a.budget.hold = Hold::Pending;
            self.queue.pop_front();
        }
    }

    /// Show an actor the manager's totals before it may ask.
    pub fn show(&self, a: &mut Actor) {
        a.budget.used = self.used;
        a.budget.waiting = !self.queue.is_empty();
    }

    /// Take in what the actor's step did to its reservation (`before`: as shown).
    pub fn settle(&mut self, a: &Actor, before: Reserve) {
        self.used += a.budget.counted() - before.counted();
        let (was, now) = (before.hold == Hold::Queued, a.budget.hold == Hold::Queued);
        if now && !was && self.queue.len() < QUEUE {
            self.queue.push_back(a.id);
        } else if was && !now {
            self.queue.retain(|&id| id != a.id);
        }
    }

    /// Ask for `units` on behalf of the actor at `i` (a request made outside its own step).
    pub fn request(&mut self, actors: &mut [Actor], i: usize, units: u8) -> bool {
        self.show(&mut actors[i]);
        let before = actors[i].budget;
        let ok = actors[i].budget_request(units);
        self.settle(&actors[i], before);
        ok
    }

    /// Held + pending units.
    pub fn used(&self) -> i32 {
        self.used
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Mode};
    use super::*;

    fn throwers(n: usize) -> Vec<Actor> {
        (0..n)
            .map(|i| {
                let mut a = Actor::new(Kind::HammerBro, 100.0 + i as f32, 32.0);
                a.id = i as u32;
                a
            })
            .collect()
    }

    #[test]
    fn a_hundred_units_are_granted_then_requests_wait_in_turn() {
        let mut b = ThrowBudget::default();
        let mut a = throwers(103);
        b.begin_frame(&mut a);
        let granted = (0..103).filter(|&i| b.request(&mut a, i, 1)).count();
        assert_eq!(granted, 100);
        assert_eq!(b.used(), 100);
        assert_eq!(a[100].budget.hold, Hold::Queued);
        // held: asked again, granted at once
        assert!(b.request(&mut a, 7, 1));
        // nothing frees: the queue stays
        b.begin_frame(&mut a);
        assert!(!b.request(&mut a, 101, 1));
        // two throwers go: the first two waiting are promoted, in order
        a[3].mode = Mode::DieFall;
        a[4].budget_release();
        b.begin_frame(&mut a);
        assert_eq!(a[100].budget.hold, Hold::Pending);
        assert_eq!(a[101].budget.hold, Hold::Pending);
        assert_eq!(a[102].budget.hold, Hold::Queued);
        assert_eq!(b.used(), 100);
        // a pending one's next request takes it
        assert!(b.request(&mut a, 101, 1));
        assert_eq!(a[101].budget.hold, Hold::Held);
        // a newcomer queues behind the one waiting, even with room left
        a[0].budget_release();
        let mut c = Actor::new(Kind::HammerBro, 0.0, 32.0);
        c.id = 500;
        a.push(c);
        assert!(!b.request(&mut a, 103, 1));
        b.begin_frame(&mut a);
        assert_eq!(a[102].budget.hold, Hold::Pending);
        assert_eq!(a[103].budget.hold, Hold::Queued);
        // gone from the area: out of the queue
        a.pop();
        b.begin_frame(&mut a);
        assert_eq!(b.used(), 100);
    }

    #[test]
    fn a_request_that_would_pass_the_limit_waits_even_alone() {
        let mut b = ThrowBudget::default();
        let mut a = throwers(26);
        b.begin_frame(&mut a);
        // 25 four-unit throwers (the winged Spiny's volley) fill it
        assert!((0..25).all(|i| b.request(&mut a, i, 4)));
        assert!(!b.request(&mut a, 25, 1));
        assert_eq!(a[25].budget.hold, Hold::Queued);
        a[0].budget_release();
        b.begin_frame(&mut a);
        assert!(b.request(&mut a, 25, 1));
    }
}
