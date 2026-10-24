//! SMM2 Super World map data (v3.0.0+): the 0x500-byte world-map blob that a Super World is
//! shared as (TGRCode `super_world/<id>` → `unk1`, base64) and that the game ships as its new-world
//! default (`MyWorld/new_layout_data.ldt`), and the map it holds (worlds, the 7 × 4 grid of each,
//! the course / Toad House / pipe points on it, progress). Layout: docs/formats/smm2-super-world.md.
//!
//! The blob is encrypted like a course file, with its own 64-word key table, which is NOT part of
//! this project: [`find_table_in_nso`] finds it in the user's own executable by decrypting the
//! `.ldt` of the same dump.

use crate::aes_modes::{cbc_decrypt, cbc_encrypt, cmac};
use crate::error::{Error, Result};
use crate::nso::{Nso, SegmentKind};

use super::crypto::{KeyTable, TABLE_WORDS, derive_keys};

/// The encrypted blob: body, IV, random state, CMAC.
pub const BLOB_SIZE: usize = 0x500;
/// The decrypted map.
pub const MAP_SIZE: usize = 0x4D0;

/// Decrypt a world-map blob; `verify` checks its CMAC.
pub fn decrypt_blob(data: &[u8], table: &KeyTable, verify: bool) -> Result<Vec<u8>> {
    if data.len() != BLOB_SIZE {
        return Err(Error::Size {
            what: "world map",
            expected: BLOB_SIZE,
            got: data.len(),
        });
    }
    let iv: [u8; 16] = data[MAP_SIZE..MAP_SIZE + 16].try_into().unwrap();
    let state = read_state(data, MAP_SIZE + 16);
    let mac: [u8; 16] = data[MAP_SIZE + 32..MAP_SIZE + 48].try_into().unwrap();
    let (aes_key, cmac_key) = derive_keys(state, table);
    let body = cbc_decrypt(&aes_key, &iv, &data[..MAP_SIZE]);
    if verify && cmac(&cmac_key, &body) != mac {
        return Err(Error::Cmac);
    }
    Ok(body)
}

/// Encrypt a decrypted map with the given IV and random state.
pub fn encrypt_blob(
    map: &[u8],
    table: &KeyTable,
    iv: [u8; 16],
    random_state: [u32; 4],
) -> Result<Vec<u8>> {
    if map.len() != MAP_SIZE {
        return Err(Error::Size {
            what: "world map body",
            expected: MAP_SIZE,
            got: map.len(),
        });
    }
    let (aes_key, cmac_key) = derive_keys(random_state, table);
    let mut out = cbc_encrypt(&aes_key, &iv, map);
    out.extend_from_slice(&iv);
    for w in random_state {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&cmac(&cmac_key, map));
    Ok(out)
}

fn read_state(data: &[u8], off: usize) -> [u32; 4] {
    std::array::from_fn(|i| {
        u32::from_le_bytes(data[off + i * 4..off + i * 4 + 4].try_into().unwrap())
    })
}

/// Search `region` (4-byte aligned words) for the 64-word table that decrypts `blob` with a valid
/// CMAC. Returns (byte offset in region, table).
pub fn find_table_in(region: &[u8], blob: &[u8]) -> Option<(usize, KeyTable)> {
    if blob.len() != BLOB_SIZE {
        return None;
    }
    let state = read_state(blob, MAP_SIZE + 16);
    let picks = super::crypto::key_schedule(state, 2);
    let words: Vec<u32> = region
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    if words.len() < TABLE_WORDS {
        return None;
    }
    for start in 0..=words.len() - TABLE_WORDS {
        let window: &[u32; TABLE_WORDS] = words[start..start + TABLE_WORDS].try_into().unwrap();
        // the game's table has no zero words where the picks land
        if picks.iter().flatten().any(|&(idx, _)| window[idx] == 0) {
            continue;
        }
        if decrypt_blob(blob, window, true).is_ok() {
            return Some((start * 4, *window));
        }
    }
    None
}

/// Search an NSO's read-only and data segments for the world-map table.
pub fn find_table_in_nso(nso: &Nso, blob: &[u8]) -> Option<KeyTable> {
    [SegmentKind::Ro, SegmentKind::Data]
        .into_iter()
        .filter_map(|k| nso.segment(k))
        .find_map(|seg| find_table_in(seg, blob).map(|(_, t)| t))
}

/// Worlds in a Super World.
pub const MAX_WORLDS: usize = 8;
/// The grid of one world.
pub const GRID_W: usize = 7;
pub const GRID_H: usize = 4;
pub const CELLS: usize = GRID_W * GRID_H;
/// Point slots of one world (start, courses, goal).
pub const POINTS: usize = 6;
/// Courses of one world, the goal included.
pub const MAX_COURSES: usize = 5;
pub const TOAD_HOUSES: usize = 3;

const HEADER: usize = 0x78;
const RECORD: usize = 0x8B;
const REC_CELLS: usize = 1;
const REC_POINTS: usize = REC_CELLS + CELLS * 3;
const REC_PIPES: usize = REC_POINTS + POINTS * 6;
const REC_TOADS: usize = REC_PIPES + 6;

/// Road directions (bit masks, the game's mask bits): x+1, x−1, y+1, y−1. The names are the
/// data's; the game draws row y + 1 above row y (the world's y points up, row 0 is the bottom
/// row: research spec super-worlds.md §7), so `SOUTH` leads up the screen.
pub const EAST: u8 = 1;
pub const WEST: u8 = 2;
pub const SOUTH: u8 = 4;
pub const NORTH: u8 = 8;

/// The four directions with their grid steps.
pub const DIRECTIONS: [(u8, i32, i32); 4] =
    [(EAST, 1, 0), (WEST, -1, 0), (SOUTH, 0, 1), (NORTH, 0, -1)];

/// The direction back.
pub fn opposite(d: u8) -> u8 {
    match d {
        EAST => WEST,
        WEST => EAST,
        SOUTH => NORTH,
        _ => SOUTH,
    }
}

/// The scene skins, in theme-value order [obs v3.0.3: the field textures registered by theme,
/// the map BGM table main+0x294bb50, the point variants by theme in 0x710143d170; SceneDB lists
/// its rows in another order].
pub const THEMES: [&str; 8] = [
    "plain",
    "underground",
    "desert",
    "woods",
    "snow",
    "athletic",
    "magma",
    "night",
];

/// One grid cell: a road piece (type 0 = none) and its rotation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub rotation: u8,
    pub kind: u8,
    pub flag: u8,
}

/// Road piece types.
pub mod road {
    pub const NONE: u8 = 0;
    pub const END: u8 = 2;
    pub const STRAIGHT: u8 = 3;
    pub const CORNER: u8 = 4;
    pub const TEE: u8 = 5;
    pub const CURVE: u8 = 6;
    pub const BRIDGE: u8 = 7;
    /// A road along a ledge: E-W, joined to the cells above and below (see the format doc).
    pub const LEDGE: u8 = 8;
}

impl Cell {
    /// The directions this piece's own rotation opens (type 8: E and W only; its vertical links
    /// depend on the neighbours, see [`World::links`]).
    pub fn mask(&self) -> u8 {
        let r = (self.rotation & 3) as usize;
        match self.kind {
            road::END => [NORTH, SOUTH, EAST, WEST][r],
            road::STRAIGHT | road::BRIDGE => {
                [NORTH | SOUTH, NORTH | SOUTH, EAST | WEST, EAST | WEST][r]
            }
            road::CORNER | road::CURVE => {
                [EAST | SOUTH, WEST | NORTH, WEST | SOUTH, EAST | NORTH][r]
            }
            road::TEE => [
                EAST | NORTH | SOUTH,
                WEST | NORTH | SOUTH,
                EAST | WEST | SOUTH,
                EAST | WEST | NORTH,
            ][r],
            road::LEDGE => EAST | WEST,
            _ => 0,
        }
    }
}

/// What stands on a point slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Course,
    Start,
    /// The world's last course (castle or airship).
    Goal,
    /// Any other value, kept for round trips.
    Other(u8),
}

impl PointKind {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => return None,
            1 => PointKind::Course,
            3 => PointKind::Start,
            4 => PointKind::Goal,
            v => PointKind::Other(v),
        })
    }

    fn byte(self) -> u8 {
        match self {
            PointKind::Course => 1,
            PointKind::Start => 3,
            PointKind::Goal => 4,
            PointKind::Other(v) => v,
        }
    }

    pub fn holds_course(self) -> bool {
        matches!(self, PointKind::Course | PointKind::Goal)
    }
}

/// A start, course or goal point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub kind: PointKind,
    /// The point model (normal, ghost house, fortress, airship, enemy icons…).
    pub icon: u8,
    pub x: u8,
    pub y: u8,
    /// The maker's Coursebot slot of the course (None: no course).
    pub course: Option<u8>,
    pub flags: u8,
}

impl Point {
    /// The course was deleted from the maker's Coursebot.
    pub fn missing(&self) -> bool {
        self.flags & 1 != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToadHouse {
    pub x: u8,
    pub y: u8,
    /// Minigame kind (0–2).
    pub kind: u8,
}

/// One world of the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    /// Index into [`THEMES`].
    pub theme: u8,
    pub cells: [Cell; CELLS],
    /// The six point slots in stored order (None: empty slot).
    pub points: [Option<Point>; POINTS],
    pub pipes: [Option<(u8, u8)>; 2],
    pub toad_houses: [Option<ToadHouse>; TOAD_HOUSES],
}

impl Default for World {
    fn default() -> Self {
        Self {
            theme: 0,
            cells: [Cell::default(); CELLS],
            points: [None; POINTS],
            pipes: [None; 2],
            toad_houses: [None; TOAD_HOUSES],
        }
    }
}

impl World {
    pub fn cell(&self, x: i32, y: i32) -> Option<&Cell> {
        if (0..GRID_W as i32).contains(&x) && (0..GRID_H as i32).contains(&y) {
            Some(&self.cells[y as usize * GRID_W + x as usize])
        } else {
            None
        }
    }

    pub fn cell_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        &mut self.cells[y * GRID_W + x]
    }

    /// The point standing at (x, y).
    pub fn point_at(&self, x: i32, y: i32) -> Option<&Point> {
        self.points
            .iter()
            .flatten()
            .find(|p| p.x as i32 == x && p.y as i32 == y)
    }

    /// Something stands at (x, y): a point, a pipe or a Toad House.
    pub fn occupied(&self, x: i32, y: i32) -> bool {
        self.point_at(x, y).is_some()
            || self
                .pipes
                .iter()
                .flatten()
                .any(|&(px, py)| (px as i32, py as i32) == (x, y))
            || self
                .toad_houses
                .iter()
                .flatten()
                .any(|t| (t.x as i32, t.y as i32) == (x, y))
    }

    pub fn start(&self) -> Option<&Point> {
        self.points
            .iter()
            .flatten()
            .find(|p| p.kind == PointKind::Start)
    }

    /// The world's courses in the game's order: the course points as stored, then the goal.
    /// Indices into [`World::points`].
    pub fn course_order(&self) -> Vec<usize> {
        let has = |i: usize, k: PointKind| {
            self.points[i].is_some_and(|p| p.kind == k && p.course.is_some())
        };
        let mut out: Vec<usize> = (0..POINTS).filter(|&i| has(i, PointKind::Course)).collect();
        out.truncate(MAX_COURSES - 1);
        out.extend((0..POINTS).filter(|&i| has(i, PointKind::Goal)).take(1));
        out
    }

    /// The directions Mario can leave (x, y) by: the road piece's own openings, a ledge's links
    /// up and down, and the links neighbours open into this cell when it holds only a point.
    pub fn links(&self, x: i32, y: i32) -> u8 {
        let Some(c) = self.cell(x, y) else {
            return 0;
        };
        let here_used = c.kind != road::NONE || self.occupied(x, y);
        if !here_used {
            return 0;
        }
        let mut out = 0;
        for (d, dx, dy) in DIRECTIONS {
            let (nx, ny) = (x + dx, y + dy);
            let Some(n) = self.cell(nx, ny) else {
                continue;
            };
            let back = opposite(d);
            let mine = c.mask() & d != 0;
            let theirs = n.mask() & back != 0;
            let linked = match (c.kind, n.kind) {
                // a ledge row: along the row, down to a point below it, and from any piece
                // that opens into it (the road coming down from above)
                (road::LEDGE, road::LEDGE) => d & (EAST | WEST) != 0,
                (road::LEDGE, road::NONE) => d == SOUTH && self.occupied(nx, ny),
                (road::LEDGE, _) => theirs,
                // a point under a ledge, with no road piece of its own
                (road::NONE, road::LEDGE) => d == NORTH,
                (road::NONE, road::NONE) => false,
                (road::NONE, _) => theirs,
                (_, road::LEDGE) => mine,
                (_, road::NONE) => mine && self.occupied(nx, ny),
                _ => mine && theirs,
            };
            if linked {
                out |= d;
            }
        }
        out
    }
}

/// A Super World's map: the decrypted 0x4D0-byte blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldMap {
    /// Byte 0x75 (3 in every map seen).
    pub version: u8,
    /// Lives at the start (1–99).
    pub lives: u8,
    pub worlds: Vec<World>,
    /// Bytes 0..0x75 (zero in every map seen), kept for round trips.
    pub reserved: Vec<u8>,
}

impl WorldMap {
    /// Parse a decrypted map ([`MAP_SIZE`] bytes).
    pub fn parse(b: &[u8]) -> Result<Self> {
        if b.len() != MAP_SIZE {
            return Err(Error::Size {
                what: "world map",
                expected: MAP_SIZE,
                got: b.len(),
            });
        }
        let count = b[0x76] as usize;
        if !(1..=MAX_WORLDS).contains(&count) {
            return Err(Error::Format {
                what: "world map",
                detail: format!("{count} worlds"),
            });
        }
        let worlds = (0..count)
            .map(|w| {
                let r = &b[HEADER + w * RECORD..HEADER + (w + 1) * RECORD];
                let cells = std::array::from_fn(|i| {
                    let c = &r[REC_CELLS + i * 3..REC_CELLS + i * 3 + 3];
                    Cell {
                        rotation: c[0],
                        kind: c[1],
                        flag: c[2],
                    }
                });
                let points = std::array::from_fn(|i| {
                    let p = &r[REC_POINTS + i * 6..REC_POINTS + i * 6 + 6];
                    PointKind::from_byte(p[0]).map(|kind| Point {
                        kind,
                        icon: p[1],
                        x: p[2],
                        y: p[3],
                        course: (p[4] < 0x80).then_some(p[4]),
                        flags: p[5],
                    })
                });
                let pipe = |o: usize| (r[o + 2] != 0).then(|| (r[o], r[o + 1]));
                let toad = |o: usize| {
                    (r[o + 3] != 0).then(|| ToadHouse {
                        x: r[o],
                        y: r[o + 1],
                        kind: r[o + 2],
                    })
                };
                World {
                    theme: r[0],
                    cells,
                    points,
                    pipes: [pipe(REC_PIPES), pipe(REC_PIPES + 3)],
                    toad_houses: std::array::from_fn(|k| toad(REC_TOADS + k * 4)),
                }
            })
            .collect();
        Ok(Self {
            version: b[0x75],
            lives: b[0x77],
            worlds,
            reserved: b[..0x75].to_vec(),
        })
    }

    /// The decrypted blob back ([`MAP_SIZE`] bytes).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; MAP_SIZE];
        let n = self.reserved.len().min(0x75);
        b[..n].copy_from_slice(&self.reserved[..n]);
        b[0x75] = self.version;
        b[0x76] = self.worlds.len().min(MAX_WORLDS) as u8;
        b[0x77] = self.lives;
        // unused records keep their empty point slots (course byte 0xFF)
        for w in 0..MAX_WORLDS {
            for i in 0..POINTS {
                b[HEADER + w * RECORD + REC_POINTS + i * 6 + 4] = 0xFF;
            }
        }
        for (w, world) in self.worlds.iter().take(MAX_WORLDS).enumerate() {
            let r = &mut b[HEADER + w * RECORD..HEADER + (w + 1) * RECORD];
            r[0] = world.theme;
            for (i, c) in world.cells.iter().enumerate() {
                r[REC_CELLS + i * 3..REC_CELLS + i * 3 + 3]
                    .copy_from_slice(&[c.rotation, c.kind, c.flag]);
            }
            for (i, p) in world.points.iter().enumerate() {
                let o = REC_POINTS + i * 6;
                r[o..o + 6].copy_from_slice(&match p {
                    Some(p) => [
                        p.kind.byte(),
                        p.icon,
                        p.x,
                        p.y,
                        p.course.unwrap_or(0xFF),
                        p.flags,
                    ],
                    None => [0, 0, 0, 0, 0xFF, 0],
                });
            }
            for (k, p) in world.pipes.iter().enumerate() {
                if let Some((x, y)) = p {
                    r[REC_PIPES + k * 3..REC_PIPES + k * 3 + 3].copy_from_slice(&[*x, *y, 1]);
                }
            }
            for (k, t) in world.toad_houses.iter().enumerate() {
                if let Some(t) = t {
                    r[REC_TOADS + k * 4..REC_TOADS + k * 4 + 4]
                        .copy_from_slice(&[t.x, t.y, t.kind, 1]);
                }
            }
        }
        b
    }

    /// Courses in the whole map, world by world in [`World::course_order`].
    pub fn course_count(&self) -> usize {
        self.worlds.iter().map(|w| w.course_order().len()).sum()
    }
}

/// A hand-built test map (no game data): one world, start → two courses → goal along the top
/// row, a branch down to a ledge row with a Toad House under it, and a pipe pair.
pub fn sample_map() -> WorldMap {
    let mut w = World {
        theme: 3,
        ..World::default()
    };
    let set = |w: &mut World, x: usize, y: usize, rotation: u8, kind: u8| {
        *w.cell_mut(x, y) = Cell {
            rotation,
            kind,
            flag: 0,
        };
    };
    set(&mut w, 0, 0, 2, road::END); // start, opens east
    set(&mut w, 1, 0, 2, road::STRAIGHT);
    set(&mut w, 2, 0, 2, road::TEE); // E+W+S
    set(&mut w, 3, 0, 2, road::STRAIGHT);
    set(&mut w, 4, 0, 3, road::END); // goal, opens west
    set(&mut w, 2, 1, 0, road::STRAIGHT); // N-S
    set(&mut w, 2, 2, 0, road::LEDGE);
    set(&mut w, 3, 2, 0, road::LEDGE);
    set(&mut w, 6, 3, 3, road::END); // the far pipe, alone
    let pt = |kind, x, y, course| {
        Some(Point {
            kind,
            icon: 0,
            x,
            y,
            course,
            flags: 0,
        })
    };
    w.points[0] = pt(PointKind::Goal, 4, 0, Some(7));
    w.points[1] = pt(PointKind::Start, 0, 0, None);
    w.points[2] = pt(PointKind::Course, 1, 0, Some(3));
    w.points[3] = pt(PointKind::Course, 3, 0, Some(5));
    w.pipes = [Some((3, 2)), Some((6, 3))];
    w.toad_houses[0] = Some(ToadHouse {
        x: 2,
        y: 3,
        kind: 1,
    });
    WorldMap {
        version: 3,
        lives: 5,
        worlds: vec![w],
        reserved: vec![0; 0x75],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stand_in_table() -> KeyTable {
        // not the game's table: a deterministic stand-in
        let mut x = 0x9e37_79b9u32;
        std::array::from_fn(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x | 1
        })
    }

    #[test]
    fn map_round_trip_and_fields() {
        let m = sample_map();
        let b = m.to_bytes();
        assert_eq!(b.len(), MAP_SIZE);
        assert_eq!((b[0x75], b[0x76], b[0x77], b[0x78]), (3, 1, 5, 3));
        // world 0's first cell at 0x79, its points at 0xCD, unused records keep 0xFF slots
        assert_eq!(&b[0x79..0x7C], &[2, road::END, 0]);
        assert_eq!(&b[0xCD..0xD3], &[4, 0, 4, 0, 7, 0]);
        assert_eq!(b[0x78 + RECORD + REC_POINTS + 4], 0xFF);
        let back = WorldMap::parse(&b).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.to_bytes(), b);
    }

    #[test]
    fn course_order_puts_the_goal_last() {
        let w = &sample_map().worlds[0];
        assert_eq!(w.course_order(), vec![2, 3, 0]);
        assert_eq!(sample_map().course_count(), 3);
        assert_eq!(w.start().map(|p| (p.x, p.y)), Some((0, 0)));
    }

    #[test]
    fn links_follow_the_road_and_the_ledge() {
        let w = &sample_map().worlds[0];
        assert_eq!(w.links(0, 0), EAST);
        assert_eq!(w.links(2, 0), EAST | WEST | SOUTH);
        assert_eq!(w.links(4, 0), WEST);
        // the ledge: up the straight, along the row, down to the Toad House under it
        assert_eq!(w.links(2, 2), NORTH | EAST | SOUTH);
        assert_eq!(w.links(3, 2), WEST);
        assert_eq!(w.links(2, 3), NORTH);
        assert_eq!(w.links(5, 0), 0);
        // every link is two-way
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                for (d, dx, dy) in DIRECTIONS {
                    if w.links(x, y) & d != 0 {
                        assert_ne!(w.links(x + dx, y + dy) & opposite(d), 0, "({x},{y}) {d}");
                    }
                }
            }
        }
    }

    #[test]
    fn blob_crypto_and_table_search() {
        let table = stand_in_table();
        let map = sample_map().to_bytes();
        let blob = encrypt_blob(&map, &table, [7; 16], [11, 22, 33, 44]).unwrap();
        assert_eq!(blob.len(), BLOB_SIZE);
        assert_eq!(decrypt_blob(&blob, &table, true).unwrap(), map);
        let mut bad = blob.clone();
        bad[5] ^= 1;
        assert!(decrypt_blob(&bad, &table, true).is_err());
        // the table hidden in a region of other words
        let mut region: Vec<u8> = (0..300u32)
            .flat_map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes())
            .collect();
        let at = region.len();
        region.extend(table.iter().flat_map(|w| w.to_le_bytes()));
        region.extend([0u8; 64]);
        assert_eq!(find_table_in(&region, &blob), Some((at, table)));
        assert!(WorldMap::parse(&vec![0u8; MAP_SIZE]).is_err());
    }
}
