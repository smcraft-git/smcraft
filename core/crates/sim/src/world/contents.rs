//! What blocks release beyond the simple items (4-brick.md B9): any part comes out as its own
//! actor, dressed as it was put in (wings, parachute, big), and a vine grows out of the block
//! (Ivy, 64-vine.md).

use super::{Event, World};
use crate::actors::{Actor, Mode};
use crate::level::TILE;

/// Ivy's growth: 1 px a frame, from 16 px to 512 px (64-vine.md).
const IVY_SPEED: f32 = 1.0;
const IVY_START: f32 = 16.0;
const IVY_MAX: f32 = 512.0;
/// It stops 32 px past the area's top (or bottom, growing down).
const IVY_PAST_EDGE: f32 = 32.0;

/// A vine growing out of a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ivy {
    /// Centre x and the block edge it grows from, in px.
    pub x: f32,
    pub base: f32,
    pub len: f32,
    pub down: bool,
    pub growing: bool,
    /// Its climb area in `Level::vines`.
    slot: usize,
}

impl Ivy {
    /// The vine's bottom and top, in px.
    pub fn span(&self) -> (f32, f32) {
        if self.down {
            (self.base - self.len, self.base)
        } else {
            (self.base, self.base + self.len)
        }
    }
}

impl World {
    /// The actor a block's `Contents::Part` comes out as, at (x, y), dressed as placed; and
    /// whether it carries a parachute (the caller opens it once the actor is out).
    pub(super) fn part_out(&self, id: i16, flags: u32, x: f32, y: f32) -> Option<(Actor, bool)> {
        let (kind, wings, parachute, big) = self.level.part_actor(id, flags)?;
        let mut a = Actor::new(kind, x, y);
        a.dir = 1;
        a.wings = wings;
        a.big = big;
        Some((a, parachute))
    }

    /// Open a released actor's parachute at once (a broken crate's, an attached block's
    /// contents); blocks and trees shoot theirs out first (`Actor::block_out`).
    pub(super) fn open_parachute(a: &mut Actor) {
        a.vx = 0.0;
        a.vy = 0.0;
        a.mode = Mode::parachute(false);
    }

    /// A vine sprouts from the block at (x, by) (centre, bottom): up from its top, or down
    /// from its bottom when the block was hit from above.
    pub(super) fn sprout_ivy(&mut self, x: f32, by: f32, down: bool) {
        let base = if down { by } else { by + TILE };
        let mut ivy = Ivy {
            x,
            base,
            len: IVY_START,
            down,
            growing: true,
            slot: self.level.vines.len(),
        };
        let (bottom, top) = ivy.span();
        self.level.vines.push((x, bottom, top));
        // a vine that has no room stops at once
        ivy.growing = !self.ivy_blocked(&ivy);
        self.ivy.push(ivy);
        self.events.push(Event::VineSprout);
    }

    /// The tip has reached terrain or gone past the area's edge.
    fn ivy_blocked(&self, ivy: &Ivy) -> bool {
        let (bottom, top) = ivy.span();
        let tip = if ivy.down { bottom } else { top };
        let area_top = self.level.height as f32 * TILE;
        if ivy.down && tip < -IVY_PAST_EDGE || !ivy.down && tip > area_top + IVY_PAST_EDGE {
            return true;
        }
        let probe = if ivy.down { tip - 0.5 } else { tip + 0.5 };
        self.level.cell_at(ivy.x, probe).is_solid()
    }

    /// Each frame a growing vine gets 1 px longer, until it is 512 px long, its tip meets
    /// terrain (then its length snaps to whole tiles, at least one) or passes the area's edge.
    pub(super) fn grow_ivy(&mut self) {
        for i in 0..self.ivy.len() {
            let mut ivy = self.ivy[i];
            if !ivy.growing {
                continue;
            }
            ivy.len = (ivy.len + IVY_SPEED).min(IVY_MAX);
            if ivy.len >= IVY_MAX {
                ivy.growing = false;
            } else if self.ivy_blocked(&ivy) {
                ivy.growing = false;
                ivy.len = ((ivy.len / TILE).floor() * TILE).max(TILE);
            }
            let (bottom, top) = ivy.span();
            self.level.vines[ivy.slot] = (ivy.x, bottom, top);
            self.ivy[i] = ivy;
        }
    }

    /// An enemy holding a Key lets it go when it is defeated, where it was (95-key.md: the
    /// generic child spawn) [inf: the key's own start motion from the child spawn].
    pub(super) fn drop_keys(&mut self) {
        let mut keys = Vec::new();
        for a in &mut self.actors {
            if matches!(a.holds, Some((95, _))) && !a.alive() {
                a.holds = None;
                keys.push((a.x, a.y + a.height() / 2.0 - 8.0));
            }
        }
        for (x, y) in keys {
            self.add_actor(Actor::new(crate::actors::Kind::Key, x, y));
        }
    }
}
