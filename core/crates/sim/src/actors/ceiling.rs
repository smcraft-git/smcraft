//! Ceiling Spinies and Buzzy Beetles (25-spiny.md "Ceiling mode", 28-buzzy-beetle.md "Ceiling
//! Buzzy"). Togezo's Basic action (v3.0.3 vtable 0x7102913c70; Met's derives from it) enters its
//! ceiling mode at its start (0x71012825c0) when the part was placed upside down (facing field
//! (flags >> 25) & 3 = 2, i.e. flags & 0x06000000 = 0x04000000) or comes out of a down-facing
//! pipe at normal size (Dokan context, pipe direction 3, flags & 0x6000 = 0): body turned 180°,
//! the fall cap +4.0 with the collision flipped — it is held against the ceiling and walks along
//! it at 0.5 px/f (Walk exe 0x7101283060) [obs v3.0.3].
//!
//! Each Walk frame once its 2-frame lock (+0x53c) is out: a player inside the box centred 96 px
//! below it, half-extents (8·k + 48, 96) (k 1, or 1.5 / 2 by model size class: 56 / 60 / 64 px),
//! or no ceiling contact, drops it — it faces the player, an upward speed is zeroed and it enters
//! the Shell context, falling as its shell; otherwise a ceiling edge ahead turns it (ledge probe
//! 0x710129af40), as a wall does [obs v3.0.3]. The Spiny's shell keeps its spikes and wakes as a
//! Spiny, the Buzzy's is the Buzzy shell.

use super::{Actor, ENEMY_GRAVITY, Kind, TURN_LOCK, WALK_SPEED};
use crate::level::Level;

/// The ceiling fall cap (+0x27c = +4.0) [obs v3.0.3 0x71012825c0].
const CEILING_FALL: f32 = 4.0;
/// The drop box: centre 96 px below, half-height 96, half-width 8·k + 48 [obs v3.0.3: DAT
/// 0x7102319600 = −96, 0x7102318b28 = 48].
const DROP_BELOW: f32 = 96.0;
const DROP_HALF_H: f32 = 96.0;
const DROP_HALF_W: f32 = 48.0;
/// Frames before the first drop check (+0x53c = 2 at the start).
const START_LOCK: i32 = 2;

impl Actor {
    /// Parts that can walk on a ceiling.
    pub fn ceiling_walker(kind: Kind) -> bool {
        matches!(kind, Kind::Spiny | Kind::Buzzy)
    }

    /// Enter ceiling mode (the Basic start's rule): placed upside down, or out of a down pipe.
    pub fn enter_ceiling(&mut self) {
        self.ceiling = true;
        self.vy = self.vy.max(0.0);
        self.turn_lock = START_LOCK;
    }

    /// One Walk frame on the ceiling.
    pub(super) fn step_ceiling(&mut self, level: &Level) {
        let s = if self.big { 2.0 } else { 1.0 };
        self.vx = WALK_SPEED * self.dir as f32;
        // gravity −0.1875 against the flipped collision: toward the ceiling
        self.vy = (self.vy - ENEMY_GRAVITY).min(CEILING_FALL);
        self.collide(level);
        let h = self.height();
        let fx = (self.half_w() - 3.0).max(1.0);
        let top = self.y + h + 0.5;
        let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
        let on_ceiling = solid(self.x - fx, top) || solid(self.x + fx, top);
        if self.wall && self.turn_lock == 0 {
            self.turn();
            return;
        }
        if self.turn_lock > 0 {
            return;
        }
        let (px, py) = self.target;
        let half_w = 8.0 * s + DROP_HALF_W;
        let seen = (px - self.x).abs() < half_w && (py - (self.y - DROP_BELOW)).abs() < DROP_HALF_H;
        if seen || !on_ceiling {
            self.ceiling_drop();
            return;
        }
        // a ceiling edge ahead: turn back [inf: probed as a red Koopa's floor edge, 3.8 px
        // (big ×2) + 0.1 ahead]
        let ahead = self.x + self.dir as f32 * (3.8 * s + 0.1);
        if !solid(ahead, top) {
            self.dir = -self.dir;
            self.turn_lock = TURN_LOCK;
        }
    }

    /// Drop off the ceiling as a shell, facing the player.
    fn ceiling_drop(&mut self) {
        self.ceiling = false;
        self.dir = if self.target.0 < self.x { -1 } else { 1 };
        self.vy = self.vy.min(0.0);
        // the Shell context: a Buzzy's shell, or a Spiny's (`alt`: spiky, wakes as a Spiny)
        self.alt = self.kind == Kind::Spiny;
        self.buzzy = self.kind == Kind::Buzzy;
        self.kind = Kind::Shell { red: false };
        self.enter_shell_wait();
        self.vy = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Mode;
    use crate::level::{Cell, TILE};

    /// A ceiling at tile row 10 over columns 2..20, ground at the bottom.
    fn roof() -> Level {
        let mut lvl = Level::flat(40);
        for tx in 2..20 {
            lvl.set(tx, 10, Cell::Solid);
        }
        lvl
    }

    #[test]
    fn walks_the_ceiling_turns_at_its_edge_and_drops_on_the_player() {
        let lvl = roof();
        let mut a = Actor::new(Kind::Buzzy, 17.0 * TILE, 9.0 * TILE);
        a.dir = 1;
        a.enter_ceiling();
        a.target = (2.0 * TILE, 2.0 * TILE); // far away
        let mut turned = false;
        for _ in 0..200 {
            a.step(&lvl, a.target.0);
            assert!(a.ceiling, "kept on the ceiling");
            assert!((a.y + a.height() - 10.0 * TILE).abs() < 0.01, "{}", a.y);
            turned |= a.dir < 0;
        }
        assert!(turned, "turned at the ceiling's right edge");
        assert!(a.x < 20.0 * TILE);
        // the player walks underneath
        a.target = (a.x + 20.0, 2.0 * TILE);
        a.step(&lvl, a.target.0);
        assert!(!a.ceiling);
        assert!(matches!(a.kind, Kind::Shell { .. }) && a.buzzy);
        assert_eq!(a.mode, Mode::ShellWait);
        assert_eq!(a.dir, 1);
        let y0 = a.y;
        for _ in 0..30 {
            a.step(&lvl, a.target.0);
        }
        assert!(a.y < y0 - 20.0, "falls as a shell");
    }

    #[test]
    fn a_spiny_drops_as_its_spiky_shell_and_wakes_as_a_spiny() {
        let lvl = roof();
        let mut a = Actor::new(Kind::Spiny, 10.0 * TILE, 9.0 * TILE);
        a.enter_ceiling();
        a.target = (10.0 * TILE, 2.0 * TILE);
        for _ in 0..3 {
            a.step(&lvl, a.target.0);
        }
        assert!(matches!(a.kind, Kind::Shell { .. }) && a.alt && !a.buzzy);
        for _ in 0..600 {
            a.step(&lvl, a.target.0);
        }
        assert_eq!(a.kind, Kind::Spiny);
        assert!(!a.ceiling);
    }

    #[test]
    fn out_of_a_down_pipe_onto_the_ceiling() {
        use crate::level::PipeDir;
        let mut lvl = Level::flat(40);
        // the pipe's mouth row at tile 10, its body above
        for ty in 10..14 {
            lvl.set(10, ty, Cell::Solid);
            lvl.set(11, ty, Cell::Solid);
        }
        for (kind, big, ceiling) in [
            (Kind::Buzzy, false, true),
            (Kind::Spiny, false, true),
            (Kind::Buzzy, true, false),
            (Kind::Goomba, false, false),
        ] {
            let mut a = Actor::new(kind, 0.0, 0.0);
            a.big = big;
            a.target = (30.0 * TILE, 2.0 * TILE);
            a.dokan_out(PipeDir::Down, 11.0 * TILE, 10.0 * TILE, false);
            let mut n = 0;
            while matches!(a.mode, Mode::DokanOut { .. }) && n < 200 {
                a.step(&lvl, a.target.0);
                n += 1;
            }
            assert_eq!(a.ceiling, ceiling, "{kind:?} big {big}");
            if ceiling {
                for _ in 0..40 {
                    a.step(&lvl, a.target.0);
                }
                assert!(a.ceiling, "{kind:?} sticks under the pipe");
                assert!((a.y + a.height() - 10.0 * TILE).abs() < 0.01, "{}", a.y);
            }
        }
    }

    #[test]
    fn no_ceiling_drops_at_once() {
        let lvl = Level::flat(40);
        let mut a = Actor::new(Kind::Spiny, 10.0 * TILE, 9.0 * TILE);
        a.enter_ceiling();
        a.target = (30.0 * TILE, 2.0 * TILE);
        for _ in 0..3 {
            a.step(&lvl, a.target.0);
        }
        assert!(matches!(a.kind, Kind::Shell { .. }));
    }
}
