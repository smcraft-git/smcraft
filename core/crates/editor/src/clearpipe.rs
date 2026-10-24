//! Clear pipes (part 93, 3D World): drawn, not placed. They live in the area's clear-pipe
//! records (`mmk_formats::smm2::clear_pipe` has the encoding), not among the objects.
//!
//! The game's gesture (`EditState::cDrawPutDokan`, v3.0.3 enter 0x7100fa4d00, per frame
//! 0x7100fa5770, research/specs/maker.md C14) [obs]: a press with the Clear Pipe on free space
//! starts a drawing at the grid corner under the cursor; once the cursor is 24 px from it, a
//! straight pipe two tiles long appears, its mouth centred on that corner, running the way the
//! cursor went (the quarter of the circle it is in), and while the press stays within 16 px of
//! the corner again it is taken back (0x7100fa9090). Then the open end follows the cursor: on
//! along its direction, back along it (the pipe gets shorter; past its last bend the bend goes
//! too), or round a bend to the side. A press on a pipe's mouth draws on from that end
//! (0x7100ec1e90 picks it); elsewhere on the pipe it is carried. A pipe takes no tile that
//! ground, a part or another clear pipe has [corp: none of the 208 sample pipes overlaps
//! ground or a solid part]; the game counts at most 100 clear pipes in an area (the counter
//! at +0x3E4 of maker.md §1) [obs]. How the cursor's path chooses between going on and
//! turning is ours [inf]: on while the cursor is in line with the open end, round a bend once
//! it is beside the two tiles past the end.

use mmk_formats::record::Record;
use mmk_formats::smm2::clear_pipe::{Dir, Piece, Shape};

use crate::{EditError, Editor, rules};

/// The Clear Pipe's part id.
pub const CLEAR_PIPE: i16 = 93;
/// Clear pipes per area the editor allows (the file has 200 slots).
pub const CLEAR_PIPE_MAX: usize = 100;
/// Record slots of an area.
const SLOTS: usize = 200;
/// A new pipe's length.
pub const NEW_LEN: i32 = 2;
/// Steps one `draw_clear_pipe` call may take toward the cursor.
const MAX_STEPS: usize = 256;

/// The end of a clear pipe being drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipeEnd {
    /// The first node's end.
    Start,
    /// The last node's end.
    End,
}

/// The run a new pipe starts with from grid corner `c` (the corner at the bottom-left of tile
/// c) going `dir`: its base tile, so that the mouth's two tiles sit either side of the corner
/// (0x7100fa9090 offsets the corner by (±8, ±8) px by quadrant) [obs].
pub fn new_origin(c: (i32, i32), dir: Dir) -> (i32, i32) {
    let (cx, cy) = c;
    match dir {
        Dir::Right => (cx, cy),
        Dir::Up => (cx - 1, cy),
        Dir::Left => (cx - 1, cy - 1),
        Dir::Down => (cx, cy - 1),
    }
}

/// The way a drag from a corner goes: the quarter of the circle the cursor is in (right for
/// −45°…45°, up for 45°…135° …) [obs 0x7100fa9090: atan2 against ±π/4, ±3π/4].
pub fn drag_dir(dx: f32, dy: f32) -> Dir {
    let a = dy.atan2(dx);
    let q = std::f32::consts::FRAC_PI_4;
    if a > q && a <= 3.0 * q {
        Dir::Up
    } else if a > 3.0 * q || a <= -3.0 * q {
        Dir::Left
    } else if a > -3.0 * q && a <= -q {
        Dir::Down
    } else {
        Dir::Right
    }
}

/// One step of the open (last) end toward tile `t`, or None when it is there.
fn step_toward(s: &Shape, t: (i32, i32)) -> Option<Shape> {
    let &(d, len) = s.runs.last()?;
    let Some(Piece::Run { rect, .. }) = s.pieces().last().copied() else {
        return None;
    };
    let (x, y, w, h) = rect;
    // how far past the end the cursor is, and which side of the pipe's lane
    let (ahead, side) = match d {
        Dir::Right | Dir::Left => {
            let tip = if d == Dir::Right { x + w - 1 } else { x };
            let a = (t.0 - tip) * d.step().0;
            let side = if t.1 < y {
                Some(Dir::Down)
            } else if t.1 > y + 1 {
                Some(Dir::Up)
            } else {
                None
            };
            (a, side)
        }
        Dir::Up | Dir::Down => {
            let tip = if d == Dir::Up { y + h - 1 } else { y };
            let a = (t.1 - tip) * d.step().1;
            let side = if t.0 < x {
                Some(Dir::Left)
            } else if t.0 > x + 1 {
                Some(Dir::Right)
            } else {
                None
            };
            (a, side)
        }
    };
    let mut n = s.clone();
    // a single run is at least a new pipe long, an end run after a bend a tile
    let min = if n.runs.len() == 1 { NEW_LEN } else { 1 };
    let last = n.runs.last_mut().expect("a run");
    if side.is_none() && ahead == 0 {
        return None;
    } else if ahead > 2 || (side.is_none() && ahead > 0) {
        // on along the end's direction
        last.1 += 1;
    } else if let (Some(side), 1 | 2) = (side, ahead) {
        // beside the two tiles past the end: a bend that way and a tile after it
        n.runs.push((side, 1));
    } else {
        // back: shorter, and past the last bend the bend goes too
        if len > min {
            last.1 -= 1;
        } else {
            n.runs.pop();
            while n.runs.len() > 1 && n.runs.last().is_some_and(|r| r.1 == 0) {
                n.runs.pop();
            }
        }
    }
    n.valid().then_some(n)
}

impl Editor {
    /// The clear pipe in record slot `slot` of the current area.
    pub fn clear_pipe(&self, slot: usize) -> Option<Shape> {
        self.area()
            .clear_pipes
            .slots
            .get(&slot)
            .and_then(Shape::decode)
    }

    /// The clear pipes of the current area: (slot, shape).
    pub fn clear_pipes(&self) -> Vec<(usize, Shape)> {
        self.area()
            .clear_pipes
            .slots
            .iter()
            .filter_map(|(&k, p)| Shape::decode(p).map(|s| (k, s)))
            .collect()
    }

    /// The clear pipe covering a tile (its slot).
    pub fn clear_pipe_at(&self, tx: i32, ty: i32) -> Option<usize> {
        self.clear_pipes()
            .into_iter()
            .find(|(_, s)| s.tiles().contains(&(tx, ty)))
            .map(|(k, _)| k)
    }

    /// The clear pipe whose mouth is at a tile, and which end (0x7100ec1e90 tests the press
    /// against a box at each open end) [obs].
    pub fn clear_pipe_end_at(&self, tx: i32, ty: i32) -> Option<(usize, PipeEnd)> {
        self.clear_pipes().into_iter().find_map(|(k, s)| {
            let [start, end] = s.mouths();
            let hit = |((x, y, w, h), _): (mmk_formats::smm2::clear_pipe::TileRect, Dir)| {
                (x..x + w).contains(&tx) && (y..y + h).contains(&ty)
            };
            if hit(end) {
                Some((k, PipeEnd::End))
            } else if hit(start) {
                Some((k, PipeEnd::Start))
            } else {
                None
            }
        })
    }

    /// Whether a clear pipe (other than `skip`) covers a tile.
    pub(crate) fn clear_pipe_covers(&self, x: i32, y: i32, skip: Option<usize>) -> bool {
        self.clear_pipes()
            .iter()
            .any(|(k, s)| Some(*k) != skip && s.tiles().contains(&(x, y)))
    }

    /// Whether `s` may be where it is (as the pipe in `skip`, or a new one).
    fn check_clear_pipe(&self, s: &Shape, skip: Option<usize>) -> Result<(), EditError> {
        if !s.valid() {
            return Err(EditError::NotAllowed("clear pipe shape"));
        }
        let tiles = s.tiles();
        let mut seen = std::collections::HashSet::new();
        for &(x, y) in &tiles {
            if !self.in_bounds(x, y) {
                return Err(EditError::OutOfBounds);
            }
            if !seen.insert((x, y)) {
                return Err(EditError::Occupied);
            }
            if self.protected(x, y) {
                return Err(EditError::Protected);
            }
            if self.has_ground(x, y)
                || self.solid_part_at(x, y, None, false)
                || self.clear_pipe_covers(x, y, skip)
            {
                return Err(EditError::Occupied);
            }
        }
        Ok(())
    }

    fn write_clear_pipe(&mut self, slot: usize, s: &Shape) -> Result<(), EditError> {
        let r = s
            .encode(slot)
            .ok_or(EditError::NotAllowed("clear pipe shape"))?;
        self.area_mut().clear_pipes.slots.insert(slot, r);
        Ok(())
    }

    /// Start a clear pipe at grid corner `corner` going `dir`: a straight pipe two tiles long
    /// with its mouth on the corner. Ok(its slot).
    pub fn place_clear_pipe(&mut self, corner: (i32, i32), dir: Dir) -> Result<usize, EditError> {
        let style = self.course.header.game_style.as_str_lossy();
        if !rules::style_allows(&style, CLEAR_PIPE) {
            return Err(EditError::NotAllowed("not in this game style"));
        }
        if self.area().clear_pipes.len() >= CLEAR_PIPE_MAX {
            return Err(EditError::Limit("clear pipes"));
        }
        let slot = (0..SLOTS)
            .find(|k| !self.area().clear_pipes.slots.contains_key(k))
            .ok_or(EditError::Full)?;
        let s = Shape::straight(new_origin(corner, dir), dir, NEW_LEN);
        self.check_clear_pipe(&s, None)?;
        self.write_clear_pipe(slot, &s)?;
        Ok(slot)
    }

    /// Draw the clear pipe in `slot` from its `end` toward tile `target`: on, back (shorter,
    /// a bend at a time) or round a bend, a tile at a step, until the end is there or the next
    /// step is blocked. Ok(true) when the pipe changed.
    pub fn draw_clear_pipe(
        &mut self,
        slot: usize,
        end: PipeEnd,
        target: (i32, i32),
    ) -> Result<bool, EditError> {
        let mut s = self.clear_pipe(slot).ok_or(EditError::Missing)?;
        if end == PipeEnd::Start {
            s = s.reversed();
        }
        let mut changed = false;
        let mut refused = None;
        for _ in 0..MAX_STEPS {
            let Some(n) = step_toward(&s, target) else {
                break;
            };
            let n_fixed = if end == PipeEnd::Start {
                n.reversed()
            } else {
                n.clone()
            };
            match self.check_clear_pipe(&n_fixed, Some(slot)) {
                Ok(()) => {
                    s = n;
                    changed = true;
                }
                Err(e) => {
                    refused = Some(e);
                    break;
                }
            }
        }
        if !changed {
            return refused.map_or(Ok(false), Err);
        }
        if end == PipeEnd::Start {
            s = s.reversed();
        }
        self.write_clear_pipe(slot, &s)?;
        Ok(true)
    }

    /// Move the clear pipe in `slot` by (dx, dy) tiles.
    pub fn move_clear_pipe(&mut self, slot: usize, dx: i32, dy: i32) -> Result<(), EditError> {
        let mut s = self.clear_pipe(slot).ok_or(EditError::Missing)?;
        s.origin = (s.origin.0 + dx, s.origin.1 + dy);
        self.check_clear_pipe(&s, Some(slot))?;
        self.write_clear_pipe(slot, &s)
    }

    /// Erase the clear pipe in `slot` (the whole pipe).
    pub fn erase_clear_pipe(&mut self, slot: usize) -> bool {
        self.area_mut().clear_pipes.slots.remove(&slot).is_some()
    }

    /// The record of a slot, raw (tests, captures).
    pub fn clear_pipe_record(&self, slot: usize) -> Option<Vec<u8>> {
        self.area()
            .clear_pipes
            .slots
            .get(&slot)
            .and_then(|r| r.to_bytes().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::*;

    #[test]
    fn a_drag_from_a_corner_picks_the_quarter() {
        assert_eq!(drag_dir(30.0, 5.0), Dir::Right);
        assert_eq!(drag_dir(-3.0, 30.0), Dir::Up);
        assert_eq!(drag_dir(-30.0, -29.0), Dir::Left);
        assert_eq!(drag_dir(10.0, -30.0), Dir::Down);
    }

    #[test]
    fn a_new_pipe_straddles_its_corner() {
        let mut e = flat("3W");
        let k = e.place_clear_pipe((20, 6), Dir::Right).unwrap();
        let mut t = e.clear_pipe(k).unwrap().tiles();
        t.sort();
        assert_eq!(t, [(20, 5), (20, 6), (21, 5), (21, 6)]);
        let k = e.place_clear_pipe((30, 6), Dir::Up).unwrap();
        let mut t = e.clear_pipe(k).unwrap().tiles();
        t.sort();
        assert_eq!(t, [(29, 6), (29, 7), (30, 6), (30, 7)]);
        // only 3D World has them
        let mut m = flat("M1");
        assert!(matches!(
            m.place_clear_pipe((20, 6), Dir::Up),
            Err(EditError::NotAllowed(_))
        ));
    }

    #[test]
    fn drawing_extends_bends_and_takes_back() {
        let mut e = flat("3W");
        let k = e.place_clear_pipe((20, 6), Dir::Right).unwrap();
        // on to the right
        assert_eq!(e.draw_clear_pipe(k, PipeEnd::End, (26, 5)), Ok(true));
        assert_eq!(e.clear_pipe(k).unwrap().runs, [(Dir::Right, 7)]);
        // up beside the two tiles past the end: a bend, then up to the cursor
        assert_eq!(e.draw_clear_pipe(k, PipeEnd::End, (27, 10)), Ok(true));
        let s = e.clear_pipe(k).unwrap();
        assert_eq!(s.runs, [(Dir::Right, 7), (Dir::Up, 4)]);
        assert_eq!(s.node_count(), 3, "start, bend, end");
        // back down: shorter, then the bend goes
        e.draw_clear_pipe(k, PipeEnd::End, (27, 7)).unwrap();
        assert_eq!(
            e.clear_pipe(k).unwrap().runs,
            [(Dir::Right, 7), (Dir::Up, 1)]
        );
        e.draw_clear_pipe(k, PipeEnd::End, (25, 5)).unwrap();
        assert_eq!(e.clear_pipe(k).unwrap().runs, [(Dir::Right, 6)]);
        // the other end draws too, leaving the record's order as it was
        e.draw_clear_pipe(k, PipeEnd::Start, (17, 5)).unwrap();
        let s = e.clear_pipe(k).unwrap();
        assert_eq!(s.runs, [(Dir::Right, 9)]);
        assert_eq!(s.origin, (17, 6));
        round_trip(&e);
    }

    #[test]
    fn a_pipe_stops_at_what_is_in_the_way() {
        let mut e = flat("3W");
        e.place(6, 26, 5).unwrap();
        let k = e.place_clear_pipe((20, 6), Dir::Right).unwrap();
        // it runs up to the block and no further
        assert_eq!(e.draw_clear_pipe(k, PipeEnd::End, (30, 5)), Ok(true));
        assert_eq!(e.clear_pipe(k).unwrap().runs, [(Dir::Right, 6)]);
        assert_eq!(
            e.draw_clear_pipe(k, PipeEnd::End, (30, 5)),
            Err(EditError::Occupied)
        );
        // nothing goes into it, and no other pipe crosses it
        assert_eq!(e.place(0, 22, 5), Err(EditError::Occupied));
        assert_eq!(e.paint_ground(22, 6), Err(EditError::Occupied));
        assert_eq!(
            e.place_clear_pipe((23, 5), Dir::Up),
            Err(EditError::Occupied)
        );
        // not into the ground either
        assert_eq!(
            e.place_clear_pipe((40, 2), Dir::Down),
            Err(EditError::Occupied)
        );
    }

    #[test]
    fn moving_and_erasing_a_pipe() {
        let mut e = flat("3W");
        let k = e.place_clear_pipe((20, 6), Dir::Up).unwrap();
        e.draw_clear_pipe(k, PipeEnd::End, (20, 12)).unwrap();
        assert_eq!(e.clear_pipe_at(19, 10), Some(k));
        assert_eq!(e.clear_pipe_end_at(20, 12), Some((k, PipeEnd::End)));
        assert_eq!(e.clear_pipe_end_at(19, 6), Some((k, PipeEnd::Start)));
        e.move_clear_pipe(k, 5, 1).unwrap();
        assert_eq!(e.clear_pipe_at(19, 10), None);
        assert_eq!(e.clear_pipe_at(24, 11), Some(k));
        assert!(e.erase(24, 11), "the eraser takes the whole pipe");
        assert!(e.area().clear_pipes.is_empty());
    }

    #[test]
    fn a_hundred_pipes_an_area() {
        let mut e = flat("3W");
        for i in 0..CLEAR_PIPE_MAX as i32 {
            let (x, y) = (10 + (i % 50) * 4, 6 + (i / 50) * 6);
            e.place_clear_pipe((x, y), Dir::Up).unwrap();
        }
        assert_eq!(
            e.place_clear_pipe((10, 20), Dir::Up),
            Err(EditError::Limit("clear pipes"))
        );
    }

    #[test]
    fn drawn_pipes_round_trip_and_play() {
        let mut e = flat("3W");
        e.checkpoint();
        let k = e.place_clear_pipe((30, 4), Dir::Up).unwrap();
        e.draw_clear_pipe(k, PipeEnd::End, (30, 9)).unwrap();
        e.draw_clear_pipe(k, PipeEnd::End, (36, 10)).unwrap();
        e.draw_clear_pipe(k, PipeEnd::End, (37, 4)).unwrap();
        let s = e.clear_pipe(k).unwrap();
        assert_eq!(s.runs.len(), 3, "up, right, down: {:?}", s.runs);
        let back = round_trip(&e);
        assert_eq!(back.main.clear_pipes.len(), 1);
        let lvl = mmk_sim::level::Level::from_course(&back, 0);
        assert!(!lvl.clear_pipes.is_empty(), "the sim builds its pieces");
        // one undo step takes it all back
        assert!(e.undo());
        assert!(e.area().clear_pipes.is_empty());
    }
}
