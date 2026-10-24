//! Local co-op in the front end (research/specs/local-coop.md, private): SMM2 offers Play
//! Together in the Coursebot and on Course World courses and starts it with the system's
//! controller set-up (2–4 players, one per controller); the challenges have no co-op in SMM2,
//! so a New Game with two or more controllers connected asks how many play [departure: the
//! owner's co-op 100 Mario / Endless, one shared life counter as in SMM2's Super World co-op].

use crate::Cx;
use crate::menu::{Buttons, UiInput};

/// "How many players?" before a challenge run (← / → change it, confirm starts, back cancels).
#[derive(Clone, Debug, PartialEq)]
pub struct PlayersAsk {
    pub players: u32,
    max: u32,
    /// The run to start once answered: difficulty, Continue.
    pub difficulty: usize,
    pub resume: bool,
}

/// The answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Players(u32),
    Cancel,
}

impl PlayersAsk {
    /// Ask when two or more controllers are connected (None: one player, nothing to ask).
    pub fn open(cx: &mut Cx, difficulty: usize, resume: bool) -> Option<Self> {
        let max = cx.host.controllers().min(4);
        (max >= 2).then_some(Self {
            players: max,
            max,
            difficulty,
            resume,
        })
    }

    pub fn step(&mut self, ui: &UiInput, cx: &mut Cx) -> Option<Asked> {
        let p = ui.pressed;
        if p.has(Buttons::BACK) {
            return Some(Asked::Cancel);
        }
        if p.has(Buttons::CONFIRM) {
            return Some(Asked::Players(self.players));
        }
        if p.has(Buttons::LEFT) || p.has(Buttons::DOWN) {
            self.players = (self.players - 1).max(1);
        } else if p.has(Buttons::RIGHT) || p.has(Buttons::UP) {
            self.players = (self.players + 1).min(self.max);
        }
        cx.toast(&self.question());
        None
    }

    /// The question as shown.
    pub fn question(&self) -> String {
        let who = if self.players == 1 {
            "1 player".to_string()
        } else {
            format!("{} players (Play Together)", self.players)
        };
        format!("How many players?  ◀ {who} ▶  — confirm to start, back to cancel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::tests::{MockHost, with};

    #[test]
    fn asks_only_with_two_controllers_and_counts_between_1_and_them() {
        let mut host = MockHost {
            controllers: 1,
            ..MockHost::default()
        };
        let mut saves = crate::Saves::default();
        with(&mut host, &mut saves, |cx| {
            assert!(PlayersAsk::open(cx, 0, false).is_none());
        });
        host.controllers = 3;
        with(&mut host, &mut saves, |cx| {
            let mut a = PlayersAsk::open(cx, 2, false).expect("asks");
            assert_eq!(a.players, 3);
            let press = |b: Buttons| UiInput {
                pressed: b,
                ..UiInput::default()
            };
            assert_eq!(a.step(&press(Buttons::RIGHT), cx), None);
            assert_eq!(a.players, 3, "no more than the controllers");
            for _ in 0..5 {
                a.step(&press(Buttons::LEFT), cx);
            }
            assert_eq!(a.players, 1);
            a.step(&press(Buttons::RIGHT), cx);
            assert_eq!(
                a.step(&press(Buttons::CONFIRM), cx),
                Some(Asked::Players(2))
            );
            assert_eq!(a.step(&press(Buttons::BACK), cx), Some(Asked::Cancel));
        });
    }

    #[test]
    fn play_together_takes_one_player_per_controller() {
        let mut host = MockHost::default();
        let mut saves = crate::Saves::default();
        with(&mut host, &mut saves, |cx| assert_eq!(cx.together(), None));
        host.controllers = 2;
        with(&mut host, &mut saves, |cx| {
            assert_eq!(cx.together(), Some(2))
        });
        host.controllers = 6;
        with(&mut host, &mut saves, |cx| {
            assert_eq!(cx.together(), Some(4))
        });
    }
}
