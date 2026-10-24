//! Keyboard (winit key codes) and gamepads (gilrs) → `mmk_sim::Input`, with the Swift app's
//! default bindings: arrows/WASD move, Z/Space jump, X/Shift run, Esc/P pause (Start), R restart.

use std::collections::HashSet;

use mmk_sim::{Input, input};
use winit::keyboard::KeyCode;

use crate::session::Actions;

/// Keys currently held.
#[derive(Debug, Default)]
pub struct Keyboard {
    down: HashSet<KeyCode>,
    restart: bool,
}

/// Engine button of a key (None: not a game key).
pub fn key_button(k: KeyCode) -> Option<u32> {
    Some(match k {
        KeyCode::ArrowLeft | KeyCode::KeyA => input::LEFT,
        KeyCode::ArrowRight | KeyCode::KeyD => input::RIGHT,
        KeyCode::ArrowUp | KeyCode::KeyW => input::UP,
        KeyCode::ArrowDown | KeyCode::KeyS => input::DOWN,
        KeyCode::KeyZ | KeyCode::Space => input::JUMP,
        KeyCode::KeyX | KeyCode::ShiftLeft | KeyCode::ShiftRight => input::RUN,
        KeyCode::Escape | KeyCode::KeyP | KeyCode::Enter => input::START,
        _ => return None,
    })
}

/// The keyboard split in two for local co-op: the engine button of a key of its left half
/// (player 1: WASD, Z / Space jump, X / left Shift run) or its right half (player 2: arrows,
/// / or Numpad 0 jump, . or right Shift or Numpad . run), and which half (false: left).
pub fn half_button(k: KeyCode) -> Option<(bool, u32)> {
    Some(match k {
        KeyCode::KeyA => (false, input::LEFT),
        KeyCode::KeyD => (false, input::RIGHT),
        KeyCode::KeyW => (false, input::UP),
        KeyCode::KeyS => (false, input::DOWN),
        KeyCode::KeyZ | KeyCode::Space => (false, input::JUMP),
        KeyCode::KeyX | KeyCode::ShiftLeft => (false, input::RUN),
        KeyCode::ArrowLeft => (true, input::LEFT),
        KeyCode::ArrowRight => (true, input::RIGHT),
        KeyCode::ArrowUp => (true, input::UP),
        KeyCode::ArrowDown => (true, input::DOWN),
        KeyCode::Slash | KeyCode::Numpad0 => (true, input::JUMP),
        KeyCode::Period | KeyCode::ShiftRight | KeyCode::NumpadDecimal => (true, input::RUN),
        _ => return None,
    })
}

/// A Bluetooth controller's button on Android (winit hands its key code over unnamed; the D-pad
/// already comes as the arrows) as the keyboard key that does the same: A / B jump, X / Y run,
/// L1 / R1 / L2 / R2 the menus' shoulders (Q / E / 1 / 3), Start pause, Select the skip key.
#[cfg(target_os = "android")]
pub fn android_pad_key(code: u32) -> Option<KeyCode> {
    Some(match code {
        96 => KeyCode::Space,
        97 => KeyCode::KeyZ,
        99 => KeyCode::KeyX,
        100 => KeyCode::ShiftLeft,
        102 => KeyCode::KeyQ,
        103 => KeyCode::KeyE,
        104 => KeyCode::Digit1,
        105 => KeyCode::Digit3,
        108 => KeyCode::KeyP,
        109 => KeyCode::Tab,
        _ => return None,
    })
}

impl Keyboard {
    pub fn key(&mut self, k: KeyCode, pressed: bool) {
        if pressed {
            if k == KeyCode::KeyR && !self.down.contains(&k) {
                self.restart = true;
            }
            self.down.insert(k);
        } else {
            self.down.remove(&k);
        }
    }

    /// Key `k` is held.
    pub fn held(&self, k: KeyCode) -> bool {
        self.down.contains(&k)
    }

    /// Forget everything held (the window lost focus: the key-up events will never arrive).
    pub fn clear(&mut self) {
        self.down.clear();
    }

    pub fn buttons(&self) -> u32 {
        self.down
            .iter()
            .filter_map(|&k| key_button(k))
            .fold(0, |b, k| b | k)
    }

    /// The buttons held on each half of the keyboard ([`half_button`]): (left, right).
    pub fn halves(&self) -> (u32, u32) {
        let (mut left, mut right) = (0, 0);
        for (r, b) in self.down.iter().filter_map(|&k| half_button(k)) {
            if r {
                right |= b;
            } else {
                left |= b;
            }
        }
        (left, right)
    }

    /// Actions pressed since the last call.
    pub fn take_actions(&mut self) -> Actions {
        Actions {
            restart: std::mem::take(&mut self.restart),
        }
    }
}

/// Stick deflection that counts as a direction (as in the Swift app).
const STICK: f32 = 0.4;

/// Connected gamepads (gilrs). Missing gamepad support (no backend, no permission) leaves it
/// empty rather than failing.
pub struct Pads {
    gilrs: Option<gilrs::Gilrs>,
    /// The shoulder buttons held on any pad at the last poll ([`SHOULDER_L`] …), for the menus.
    pub shoulders: u32,
}

/// Shoulder buttons in [`Pads::shoulders`] (not engine buttons: only the menus use them).
pub const SHOULDER_L: u32 = 1;
pub const SHOULDER_R: u32 = 2;
pub const SHOULDER_ZL: u32 = 4;
pub const SHOULDER_ZR: u32 = 8;

impl Pads {
    pub fn new() -> Self {
        Self {
            gilrs: gilrs::Gilrs::new().ok(),
            shoulders: 0,
        }
    }

    /// Process pending gamepad events and return the buttons held on any pad.
    pub fn poll(&mut self) -> u32 {
        self.poll_each().iter().fold(0, |a, b| a | b)
    }

    /// Process pending gamepad events and return each connected pad's buttons (in id order).
    pub fn poll_each(&mut self) -> Vec<u32> {
        use gilrs::{Axis, Button};
        let Some(g) = &mut self.gilrs else {
            return Vec::new();
        };
        while g.next_event().is_some() {}
        let mut out = Vec::new();
        self.shoulders = 0;
        for (_, pad) in g.gamepads() {
            let mut b = 0;
            let x = pad.value(Axis::LeftStickX);
            let y = pad.value(Axis::LeftStickY);
            let held = |btn: Button| pad.is_pressed(btn);
            if held(Button::DPadLeft) || x < -STICK {
                b |= input::LEFT;
            }
            if held(Button::DPadRight) || x > STICK {
                b |= input::RIGHT;
            }
            if held(Button::DPadUp) || y > STICK {
                b |= input::UP;
            }
            if held(Button::DPadDown) || y < -STICK {
                b |= input::DOWN;
            }
            // positions, not labels: south/east jump, west/north run (Nintendo B/A, Y/X)
            if held(Button::South) || held(Button::East) {
                b |= input::JUMP;
            }
            if held(Button::West) || held(Button::North) {
                b |= input::RUN;
            }
            if held(Button::Start) {
                b |= input::START;
            }
            for (btn, bit) in [
                (Button::LeftTrigger, SHOULDER_L),
                (Button::RightTrigger, SHOULDER_R),
                (Button::LeftTrigger2, SHOULDER_ZL),
                (Button::RightTrigger2, SHOULDER_ZR),
            ] {
                if held(btn) {
                    self.shoulders |= bit;
                }
            }
            out.push(b);
        }
        out
    }
}

impl Default for Pads {
    fn default() -> Self {
        Self::new()
    }
}

pub use mmk_front::coop::assign_pads;

/// Local co-op with the keyboard split ([`Keyboard::halves`]): the right half is player 2, pad
/// k player k + 3 (the left half is player 1 on its own). Players 2…'s buttons.
pub fn assign_split(right: u32, pads: &[u32], players: usize) -> Vec<u32> {
    let mut v: Vec<u32> = std::iter::once(right)
        .chain(pads.iter().copied())
        .take(players.saturating_sub(1))
        .collect();
    v.resize(players.saturating_sub(1), 0);
    v
}

/// Keyboard and pads together.
pub fn combine(keys: &Keyboard, pads: u32) -> Input {
    Input {
        buttons: keys.buttons() | pads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_buttons_and_restart_is_an_edge() {
        let mut k = Keyboard::default();
        k.key(KeyCode::ArrowRight, true);
        k.key(KeyCode::ShiftLeft, true);
        k.key(KeyCode::KeyZ, true);
        assert_eq!(k.buttons(), input::RIGHT | input::RUN | input::JUMP);
        k.key(KeyCode::KeyZ, false);
        assert_eq!(k.buttons(), input::RIGHT | input::RUN);
        k.key(KeyCode::KeyR, true);
        k.key(KeyCode::KeyR, true); // key repeat
        assert!(k.take_actions().restart);
        assert!(!k.take_actions().restart);
        k.clear();
        assert_eq!(k.buttons(), 0);
    }

    #[test]
    fn the_split_keyboard_is_two_players() {
        let mut k = Keyboard::default();
        for c in [
            KeyCode::KeyD,
            KeyCode::Space,
            KeyCode::ArrowLeft,
            KeyCode::ShiftRight,
        ] {
            k.key(c, true);
        }
        assert_eq!(
            k.halves(),
            (input::RIGHT | input::JUMP, input::LEFT | input::RUN)
        );
        // every key still plays player 1 when there is no split
        assert_eq!(
            k.buttons(),
            input::RIGHT | input::JUMP | input::LEFT | input::RUN
        );
        // the right half is player 2, the pads after it; missing players get nothing
        assert_eq!(assign_split(4, &[8, 16], 4), [4, 8, 16]);
        assert_eq!(assign_split(4, &[8, 16], 2), [4]);
        assert_eq!(assign_split(4, &[], 3), [4, 0]);
        assert!(assign_split(4, &[8], 1).is_empty());
    }
}
