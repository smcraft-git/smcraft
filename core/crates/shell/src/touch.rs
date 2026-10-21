//! On-screen controls for touch screens (Android; the Swift app has its own on iOS): a d-pad on
//! the left, Jump / Run / Spin on the right, + (pause) at the top right, laid out like the iOS
//! controls. Touches are tracked by id so several fingers (move + jump) work at once, and a
//! finger sliding from one button to another changes the button it holds.
//!
//! A touch that starts away from every control is a tap for the menus (`FrontApp::click`).

use std::collections::HashMap;

use mmk_sim::input;

/// One control: centre and radius as fractions of the window height (x measured from the left
/// or, with `right`, from the right edge), and the buttons it holds.
struct Control {
    x: f32,
    y: f32,
    r: f32,
    right: bool,
    buttons: u32,
}

/// The d-pad's centre and reach (fractions of the window height, from the bottom left).
const PAD: (f32, f32, f32) = (0.24, 0.74, 0.2);
/// Inside this fraction of the d-pad's reach a finger holds no direction.
const PAD_DEAD: f32 = 0.25;

const CONTROLS: &[Control] = &[
    Control {
        x: 0.2,
        y: 0.78,
        r: 0.1,
        right: true,
        buttons: input::JUMP,
    },
    Control {
        x: 0.42,
        y: 0.84,
        r: 0.09,
        right: true,
        buttons: input::RUN,
    },
    Control {
        x: 0.3,
        y: 0.58,
        r: 0.08,
        right: true,
        buttons: input::SPIN,
    },
    Control {
        x: 0.08,
        y: 0.1,
        r: 0.06,
        right: true,
        buttons: input::START,
    },
];

/// The fingers on the screen and what they hold.
#[derive(Debug, Default)]
pub struct TouchPad {
    fingers: HashMap<u64, (f32, f32)>,
    size: (f32, f32),
    /// Shown once the screen was touched, hidden again while a controller is used.
    pub visible: bool,
}

impl TouchPad {
    pub fn resize(&mut self, w: u32, h: u32) {
        self.size = (w as f32, h as f32);
    }

    /// A finger came down at window pixel (x, y). True when it hit a control (else it is a tap
    /// for the menus).
    pub fn down(&mut self, id: u64, x: f64, y: f64) -> bool {
        self.visible = true;
        let p = (x as f32, y as f32);
        self.fingers.insert(id, p);
        self.buttons_at(p) != 0
    }

    pub fn moved(&mut self, id: u64, x: f64, y: f64) {
        if let Some(f) = self.fingers.get_mut(&id) {
            *f = (x as f32, y as f32);
        }
    }

    pub fn up(&mut self, id: u64) {
        self.fingers.remove(&id);
    }

    /// A controller or keyboard was used: hide the controls (they come back with a touch).
    pub fn hide(&mut self) {
        self.visible = false;
        self.fingers.clear();
    }

    /// The engine buttons all fingers hold.
    pub fn buttons(&self) -> u32 {
        self.fingers
            .values()
            .fold(0, |b, &p| b | self.buttons_at(p))
    }

    fn buttons_at(&self, (x, y): (f32, f32)) -> u32 {
        let (w, h) = self.size;
        if h <= 0.0 {
            return 0;
        }
        let (px, py, pr) = (PAD.0 * h, PAD.1 * h, PAD.2 * h);
        let (dx, dy) = (x - px, y - py);
        let d = (dx * dx + dy * dy).sqrt();
        if d <= pr * 1.3 {
            if d < pr * PAD_DEAD {
                return 0;
            }
            // eight directions: a diagonal holds both of its neighbours
            let a = dy.atan2(dx).to_degrees();
            let mut b = 0;
            if a.abs() < 67.5 {
                b |= input::RIGHT;
            }
            if a.abs() > 112.5 {
                b |= input::LEFT;
            }
            if a > 22.5 && a < 157.5 {
                b |= input::DOWN;
            }
            if a < -22.5 && a > -157.5 {
                b |= input::UP;
            }
            return b;
        }
        for c in CONTROLS {
            let cx = if c.right { w - c.x * h } else { c.x * h };
            let (dx, dy) = (x - cx, y - c.y * h);
            if (dx * dx + dy * dy).sqrt() <= c.r * h * 1.2 {
                return c.buttons;
            }
        }
        0
    }

    /// The controls to draw: centre (window px), radius (px), held. The d-pad comes first.
    pub fn shapes(&self) -> Vec<([f32; 2], f32, bool)> {
        let (w, h) = self.size;
        let held = self.buttons();
        let mut out = vec![(
            [PAD.0 * h, PAD.1 * h],
            PAD.2 * h,
            held & (input::LEFT | input::RIGHT | input::UP | input::DOWN) != 0,
        )];
        for c in CONTROLS {
            let cx = if c.right { w - c.x * h } else { c.x * h };
            out.push(([cx, c.y * h], c.r * h, held & c.buttons != 0));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingers_hold_the_buttons_under_them() {
        let mut t = TouchPad::default();
        t.resize(1920, 1080);
        let h = 1080.0;
        // right on the d-pad, jump at the same time
        assert!(t.down(1, (PAD.0 * h + PAD.2 * h * 0.8) as f64, (PAD.1 * h) as f64));
        assert!(t.down(2, (1920.0 - 0.2 * h) as f64, (0.78 * h) as f64));
        assert_eq!(t.buttons(), input::RIGHT | input::JUMP);
        // sliding the jump finger onto Run
        t.moved(2, (1920.0 - 0.42 * h) as f64, (0.84 * h) as f64);
        assert_eq!(t.buttons(), input::RIGHT | input::RUN);
        t.up(1);
        t.up(2);
        assert_eq!(t.buttons(), 0);
        // the middle of the screen is a tap for the menus
        assert!(!t.down(3, 960.0, 300.0));
        // a diagonal holds both directions
        t.up(3);
        let d = PAD.2 * h * 0.7;
        t.down(4, (PAD.0 * h + d) as f64, (PAD.1 * h - d) as f64);
        assert_eq!(t.buttons(), input::RIGHT | input::UP);
    }
}
