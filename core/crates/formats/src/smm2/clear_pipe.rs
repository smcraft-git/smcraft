//! The shape a clear-pipe record (part 93, the area's slot-indexed clear-pipe array) encodes.
//!
//! A clear pipe is two tiles wide: straight runs joined by 2 × 2 bends. The record lists one
//! node per piece in order: the first run (type 0), bends (types 3–10) and the runs between
//! them (type 11, left out when two bends touch), the last run (type 1); a pipe of one run is a
//! single node of type 2. A run's node holds its direction (0 right, 1 left, 2 up, 3 down), its
//! length in `height`, `width` 2, and its base tile, which turns with the direction as a pipe's
//! does: the bottom-left tile of an up run, top-right of a down run, bottom-right of a left run,
//! top-left of a right run. A bend's node holds its bottom-left tile, 2 × 2, direction 2. Byte 6
//! of every node is 1 and the record's third byte (`unk_2`) is 1; node `index` counts from 0.
//!
//! Checked on the 550 sample courses: all 208 clear pipes decode to runs and bends and encode
//! back to the same bytes; a run between two bends may be empty, the end runs are at least one
//! tile long [corp]. The bend codes name the turn (`BENDS`) [corp]; the base tiles agree with
//! the game's node geometry (direction vectors at 0x710233a3f4, the side each run's second row
//! lies on at 0x7102320590, v3.0.3) [obs].

use crate::record::Record;
use crate::smm2::course::{ClearPipe, ClearPipeNode};

/// Most nodes a record holds.
pub const MAX_NODES: usize = 36;

/// A run's direction: the node's code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Right, Dir::Left, Dir::Up, Dir::Down];

    pub fn from_code(c: u8) -> Option<Dir> {
        Dir::ALL.get(c as usize).copied()
    }

    /// Unit step (x right, y up).
    pub fn step(self) -> (i32, i32) {
        match self {
            Dir::Right => (1, 0),
            Dir::Left => (-1, 0),
            Dir::Up => (0, 1),
            Dir::Down => (0, -1),
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Right => Dir::Left,
            Dir::Left => Dir::Right,
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Dir::Right | Dir::Left)
    }

    /// Quarter turns counter-clockwise from up (the clear-pipe models' axis).
    pub fn quarter_turns(self) -> u8 {
        match self {
            Dir::Up => 0,
            Dir::Left => 1,
            Dir::Down => 2,
            Dir::Right => 3,
        }
    }
}

/// Bend node codes: (type, direction before, direction after) [corp].
pub const BENDS: [(u8, Dir, Dir); 8] = [
    (3, Dir::Right, Dir::Down),
    (4, Dir::Right, Dir::Up),
    (5, Dir::Left, Dir::Down),
    (6, Dir::Left, Dir::Up),
    (7, Dir::Up, Dir::Left),
    (8, Dir::Up, Dir::Right),
    (9, Dir::Down, Dir::Left),
    (10, Dir::Down, Dir::Right),
];

fn bend_code(from: Dir, to: Dir) -> Option<u8> {
    BENDS.iter().find(|b| b.1 == from && b.2 == to).map(|b| b.0)
}

fn bend_of(code: u8) -> Option<(Dir, Dir)> {
    BENDS.iter().find(|b| b.0 == code).map(|b| (b.1, b.2))
}

/// A tile rectangle: bottom-left (x, y), width, height.
pub type TileRect = (i32, i32, i32, i32);

/// The tiles a run of `len` from base tile (x, y) covers.
pub fn run_rect(dir: Dir, x: i32, y: i32, len: i32) -> TileRect {
    match dir {
        Dir::Right => (x, y - 1, len, 2),
        Dir::Left => (x - len + 1, y, len, 2),
        Dir::Up => (x, y, 2, len),
        Dir::Down => (x - 1, y - len + 1, 2, len),
    }
}

/// A run's base tile from its rectangle.
pub fn run_base(dir: Dir, r: TileRect) -> (i32, i32) {
    let (x, y, w, h) = r;
    match dir {
        Dir::Right => (x, y + 1),
        Dir::Left => (x + w - 1, y),
        Dir::Up => (x, y),
        Dir::Down => (x + 1, y + h - 1),
    }
}

/// The 2 × 2 cell (bottom-left) just past the end of a run going `dir`.
fn past(dir: Dir, r: TileRect) -> (i32, i32) {
    let (x, y, w, h) = r;
    match dir {
        Dir::Right => (x + w, y),
        Dir::Left => (x - 2, y),
        Dir::Up => (x, y + h),
        Dir::Down => (x, y - 2),
    }
}

/// The run of `len` going `dir` out of the bend at `b` (bottom-left).
fn run_after(dir: Dir, b: (i32, i32), len: i32) -> TileRect {
    let (bx, by) = b;
    match dir {
        Dir::Right => (bx + 2, by, len, 2),
        Dir::Left => (bx - len, by, len, 2),
        Dir::Up => (bx, by + 2, 2, len),
        Dir::Down => (bx, by - len, 2, len),
    }
}

/// One piece of a clear pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// A straight run (`len` may be 0 between two bends: no tiles).
    Run { dir: Dir, rect: TileRect },
    /// A 2 × 2 bend from `from` to `to`, bottom-left at `at`.
    Bend { from: Dir, to: Dir, at: (i32, i32) },
}

impl Piece {
    pub fn rect(&self) -> TileRect {
        match *self {
            Piece::Run { rect, .. } => rect,
            Piece::Bend { at, .. } => (at.0, at.1, 2, 2),
        }
    }
}

/// A clear pipe's shape: the first run's base tile and its runs, a bend between each two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub origin: (i32, i32),
    pub runs: Vec<(Dir, i32)>,
}

impl Shape {
    /// A straight pipe.
    pub fn straight(origin: (i32, i32), dir: Dir, len: i32) -> Self {
        Self {
            origin,
            runs: vec![(dir, len)],
        }
    }

    /// The shape of a record (None when its nodes don't make one, which no sample course has).
    pub fn decode(p: &ClearPipe) -> Option<Self> {
        let nodes = &p.nodes.items;
        let first = nodes.first()?;
        if !matches!(first.r#type, 0 | 2) {
            return None;
        }
        let mut runs = vec![(Dir::from_code(first.direction)?, first.height as i32)];
        for n in &nodes[1..] {
            if let Some((from, to)) = bend_of(n.r#type) {
                if runs.last()?.0 != from {
                    return None;
                }
                runs.push((to, 0));
            } else if matches!(n.r#type, 1 | 11) {
                let last = runs.last_mut()?;
                if last.0 as u8 != n.direction || last.1 != 0 {
                    return None;
                }
                last.1 = n.height as i32;
            } else {
                return None;
            }
        }
        let s = Self {
            origin: (first.x as i32, first.y as i32),
            runs,
        };
        // the nodes must be the ones the shape makes
        let back = s.nodes()?;
        let same = back.len() == nodes.len()
            && back.iter().zip(nodes).all(|(a, b)| {
                (a.r#type, a.x, a.y, a.width, a.height, a.direction)
                    == (b.r#type, b.x, b.y, b.width, b.height, b.direction)
            });
        same.then_some(s)
    }

    /// Whether the shape can be stored: turns of 90°, end runs at least a tile (a straight
    /// pipe two), inner runs not negative, at most `MAX_NODES` nodes, tiles in 0…255.
    pub fn valid(&self) -> bool {
        let n = self.runs.len();
        if n == 0 || (n == 1 && self.runs[0].1 < 2) {
            return false;
        }
        for (k, &(d, len)) in self.runs.iter().enumerate() {
            let end = k == 0 || k == n - 1;
            if len < if end { 1 } else { 0 } {
                return false;
            }
            if k > 0 && (self.runs[k - 1].0.horizontal() == d.horizontal()) {
                return false;
            }
        }
        self.node_count() <= MAX_NODES
            && self.pieces().iter().all(|p| {
                let (x, y, w, h) = p.rect();
                x >= 0 && y >= 0 && x + w <= 256 && y + h <= 256
            })
    }

    /// Nodes the record needs.
    pub fn node_count(&self) -> usize {
        let n = self.runs.len();
        // every run but empty inner ones, and a bend between each two
        let runs = self
            .runs
            .iter()
            .enumerate()
            .filter(|&(k, r)| r.1 > 0 || k == 0 || k == n - 1)
            .count();
        runs + n - 1
    }

    /// The pieces in order, from the first run's start.
    pub fn pieces(&self) -> Vec<Piece> {
        let mut out = Vec::new();
        let Some(&(d0, l0)) = self.runs.first() else {
            return out;
        };
        let mut rect = run_rect(d0, self.origin.0, self.origin.1, l0);
        out.push(Piece::Run { dir: d0, rect });
        for w in self.runs.windows(2) {
            let ((from, _), (to, len)) = (w[0], w[1]);
            let at = past(from, rect);
            out.push(Piece::Bend { from, to, at });
            rect = run_after(to, at, len);
            out.push(Piece::Run { dir: to, rect });
        }
        out
    }

    /// The tiles the pipe covers.
    pub fn tiles(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for p in self.pieces() {
            let (x, y, w, h) = p.rect();
            for tx in x..x + w {
                for ty in y..y + h {
                    out.push((tx, ty));
                }
            }
        }
        out
    }

    /// The two mouths: the 2 × 1 / 1 × 2 tile pair at each open end and the way it faces
    /// (the start first).
    pub fn mouths(&self) -> [(TileRect, Dir); 2] {
        let pieces = self.pieces();
        let end_of = |p: &Piece, at_start: bool| -> (TileRect, Dir) {
            let Piece::Run { dir, rect } = *p else {
                unreachable!("a pipe starts and ends with a run")
            };
            let (x, y, w, h) = rect;
            // the row at the run's start (facing back) or its end (facing on)
            let (face, along) = if at_start {
                (dir.opposite(), dir.opposite())
            } else {
                (dir, dir)
            };
            let r = match along {
                Dir::Right => (x + w - 1, y, 1, 2),
                Dir::Left => (x, y, 1, 2),
                Dir::Up => (x, y + h - 1, 2, 1),
                Dir::Down => (x, y, 2, 1),
            };
            (r, face)
        };
        [
            end_of(pieces.first().expect("a run"), true),
            end_of(pieces.last().expect("a run"), false),
        ]
    }

    /// The same pipe drawn from its other end.
    pub fn reversed(&self) -> Self {
        let pieces = self.pieces();
        let Some(Piece::Run { dir, rect }) = pieces.last().copied() else {
            return self.clone();
        };
        // the last run's rectangle seen going the other way
        Self {
            origin: run_base(dir.opposite(), rect),
            runs: self
                .runs
                .iter()
                .rev()
                .map(|&(d, l)| (d.opposite(), l))
                .collect(),
        }
    }

    /// The record for slot `slot` (None when the shape can't be stored).
    pub fn encode(&self, slot: usize) -> Option<ClearPipe> {
        if !self.valid() || slot > u8::MAX as usize {
            return None;
        }
        let mut r = ClearPipe::blank();
        r.head.index = slot as u8;
        r.head.unk_2 = 1;
        r.nodes.items = self.nodes()?;
        Some(r)
    }

    /// The record's nodes.
    fn nodes(&self) -> Option<Vec<ClearPipeNode>> {
        let n = self.runs.len();
        let mut out: Vec<ClearPipeNode> = Vec::new();
        let byte = |v: i32| u8::try_from(v).ok();
        let mut push = |kind: u8, (x, y): (i32, i32), h: i32, dir: u8| -> Option<()> {
            let mut node = ClearPipeNode::blank();
            node.r#type = kind;
            node.index = u8::try_from(out.len()).ok()?;
            node.x = byte(x)?;
            node.y = byte(y)?;
            node.width = 2;
            node.height = byte(h)?;
            node.unk_6 = 1;
            node.direction = dir;
            out.push(node);
            Some(())
        };
        let mut run = 0;
        for p in self.pieces() {
            match p {
                Piece::Run { dir, rect } => {
                    let len = self.runs[run].1;
                    let kind = match (n, run) {
                        (1, _) => 2,
                        (_, 0) => 0,
                        (_, k) if k == n - 1 => 1,
                        _ => 11,
                    };
                    if len > 0 || kind != 11 {
                        push(kind, run_base(dir, rect), len, dir as u8)?;
                    }
                    run += 1;
                }
                Piece::Bend { from, to, at } => push(bend_code(from, to)?, at, 2, 2)?,
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(t: u8, x: u8, y: u8, h: u8, d: u8) -> ClearPipeNode {
        let mut n = ClearPipeNode::blank();
        (n.r#type, n.x, n.y, n.width, n.height, n.unk_6, n.direction) = (t, x, y, 2, h, 1, d);
        n
    }

    fn record(nodes: &[ClearPipeNode]) -> ClearPipe {
        let mut r = ClearPipe::blank();
        r.head.unk_2 = 1;
        r.nodes.items = nodes.to_vec();
        for (i, n) in r.nodes.items.iter_mut().enumerate() {
            n.index = i as u8;
        }
        r
    }

    #[test]
    fn a_course_pipe_decodes_and_encodes_back() {
        // down 5 from (73, 7), bend down→right, right 2, bend right→up, up 5 (a sample course's)
        let r = record(&[
            node(0, 73, 7, 5, 3),
            node(10, 72, 1, 2, 2),
            node(11, 74, 2, 2, 0),
            node(4, 76, 1, 2, 2),
            node(1, 76, 3, 5, 2),
        ]);
        let s = Shape::decode(&r).expect("decodes");
        assert_eq!(
            s.runs,
            [(Dir::Down, 5), (Dir::Right, 2), (Dir::Up, 5)],
            "three runs"
        );
        let back = s.encode(0).unwrap();
        assert_eq!(back.to_bytes().unwrap(), r.to_bytes().unwrap());
        assert_eq!(s.tiles().len(), 2 * 5 + 4 + 2 * 2 + 4 + 2 * 5);
        let [start, end] = s.mouths();
        assert_eq!(start, ((72, 7, 2, 1), Dir::Up), "the start opens upward");
        assert_eq!(end, ((76, 7, 2, 1), Dir::Up));
    }

    #[test]
    fn touching_bends_leave_the_run_out() {
        // left 2, bend left→down, bend down→right, right 1 (no run between the bends)
        let s = Shape {
            origin: (23, 2),
            runs: vec![(Dir::Left, 2), (Dir::Down, 0), (Dir::Right, 1)],
        };
        assert!(s.valid());
        let r = s.encode(3).unwrap();
        let kinds: Vec<u8> = r.nodes.items.iter().map(|n| n.r#type).collect();
        assert_eq!(kinds, [0, 5, 10, 1]);
        assert_eq!(r.head.index, 3);
        assert_eq!(Shape::decode(&r), Some(s));
    }

    #[test]
    fn a_reversed_pipe_covers_the_same_tiles() {
        let s = Shape {
            origin: (10, 4),
            runs: vec![(Dir::Right, 4), (Dir::Down, 1), (Dir::Right, 3)],
        };
        let mut a = s.tiles();
        let mut b = s.reversed().tiles();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(s.reversed().reversed(), s);
        assert_eq!(s.reversed().runs[0], (Dir::Left, 3));
    }

    #[test]
    fn invalid_shapes() {
        assert!(!Shape::straight((5, 5), Dir::Up, 1).valid(), "too short");
        let back = Shape {
            origin: (5, 5),
            runs: vec![(Dir::Up, 3), (Dir::Down, 3)],
        };
        assert!(!back.valid(), "no U-turn without a bend");
        let empty_end = Shape {
            origin: (5, 5),
            runs: vec![(Dir::Up, 3), (Dir::Right, 0)],
        };
        assert!(!empty_end.valid());
    }
}
