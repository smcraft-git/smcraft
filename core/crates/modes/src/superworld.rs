//! Super Worlds (SMM2 v3.0.0+): up to 8 worlds of up to 5 courses on 7 × 4 maps, played with
//! lives (private spec `research/specs/super-worlds.md`). This module holds what the screens
//! don't draw: the world and its courses ([`SuperWorld`]: a local test world assembled from
//! library courses, or one downloaded from TGRCode), the walk on the map (which way Mario can
//! go, where a walk stops, the pipes), the rules (a course clear, a death, a game over, a Toad
//! House, the world clear and the end) and the saved progress ([`Progress`], JSON).

use mmk_formats::smm2::world::{
    self, Cell, DIRECTIONS, GRID_H, GRID_W, Point, PointKind, ToadHouse, World, road,
};
pub use mmk_formats::smm2::world::{EAST, NORTH, SOUTH, THEMES, WEST, WorldMap};
use serde_json::{Value, json};

/// One course of a Super World.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwCourse {
    /// A library file (its path) or, when `online`, a course code.
    pub id: String,
    pub online: bool,
    pub name: String,
    pub maker: Option<String>,
    /// "M1", "M3", "MW", "WU", "3W".
    pub style: String,
}

/// A Super World to play.
#[derive(Clone, Debug, PartialEq)]
pub struct SuperWorld {
    /// "local" for the test world, else the TGRCode Super World ID. Progress is saved under it.
    pub id: String,
    /// The maker's name (the test world: None).
    pub maker: Option<String>,
    /// The planet shown in Course World (0 Earth … 7 Spiral).
    pub planet: u8,
    pub map: WorldMap,
    /// Per world, its courses in the game's course order (the goal last).
    pub courses: Vec<Vec<SwCourse>>,
}

/// The id of the world assembled from the library.
pub const LOCAL_ID: &str = "local";
/// Lives a new test world starts with (the game's new-world template has 30).
pub const TEST_LIVES: u8 = 30;
/// Lives never go above this.
pub const MAX_LIVES: u32 = 99;

impl SuperWorld {
    /// A test world from up to 40 library courses, five to a world (the fifth is the goal), on
    /// one fixed map shape per world: start, three courses along the top row with a Toad House
    /// below the second, down the right side, the fourth course and the goal along the third
    /// row; a pipe at the bottom right leads to an island with a second Toad House. None
    /// without courses.
    pub fn test_world(courses: &[SwCourse]) -> Option<Self> {
        if courses.is_empty() {
            return None;
        }
        let chunks: Vec<Vec<SwCourse>> = courses
            .chunks(world::MAX_COURSES)
            .take(world::MAX_WORLDS)
            .map(<[SwCourse]>::to_vec)
            .collect();
        let mut slot = 0u8;
        let worlds = chunks
            .iter()
            .enumerate()
            .map(|(w, c)| {
                let world = test_layout((w % THEMES.len()) as u8, c.len(), slot);
                slot += c.len() as u8;
                world
            })
            .collect();
        let map = WorldMap {
            version: 3,
            lives: TEST_LIVES,
            worlds,
            reserved: vec![0; 0x75],
        };
        Some(Self {
            id: LOCAL_ID.into(),
            maker: None,
            planet: 0,
            map,
            courses: chunks,
        })
    }

    /// A downloaded Super World: `{id, planet_type, maker: {name}, map: <hex of the decrypted
    /// 0x4D0-byte map>, courses: [listing]}` (the hosts' `super_world` answer). The courses are
    /// handed out world by world in course order.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let hex = v["map"].as_str().ok_or("no map in the Super World")?;
        let bytes = from_hex(hex).ok_or("the Super World's map isn't hex")?;
        let map = WorldMap::parse(&bytes).map_err(|e| e.to_string())?;
        let list = v["courses"].as_array().cloned().unwrap_or_default();
        let mut it = list.iter().map(|c| SwCourse {
            id: c["code"].as_str().unwrap_or_default().to_string(),
            online: true,
            name: c["name"].as_str().unwrap_or_default().to_string(),
            maker: c["maker"].as_str().map(str::to_string),
            style: c["style"].as_str().unwrap_or("M1").to_string(),
        });
        let courses = map
            .worlds
            .iter()
            .map(|w| {
                (0..w.course_order().len())
                    .filter_map(|_| it.next())
                    .collect()
            })
            .collect();
        Ok(Self {
            id: v["id"].as_str().unwrap_or_default().to_string(),
            maker: v["maker"]["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            planet: v["planet_type"].as_u64().unwrap_or(0).min(7) as u8,
            map,
            courses,
        })
    }

    pub fn world_count(&self) -> usize {
        self.map.worlds.len()
    }

    pub fn course_count(&self) -> usize {
        self.courses.iter().map(Vec::len).sum()
    }

    /// The save-data key of its progress.
    pub fn save_key(&self) -> String {
        format!("superworld.{}", self.id)
    }

    /// The course at order index `k` of world `w`.
    pub fn course(&self, w: usize, k: usize) -> Option<&SwCourse> {
        self.courses.get(w)?.get(k)
    }
}

fn test_layout(theme: u8, courses: usize, first_slot: u8) -> World {
    let mut w = World {
        theme,
        ..World::default()
    };
    let mut set = |x: usize, y: usize, rotation: u8, kind: u8| {
        *w.cell_mut(x, y) = Cell {
            rotation,
            kind,
            flag: 0,
        };
    };
    set(0, 0, 2, road::END);
    set(1, 0, 2, road::STRAIGHT);
    set(2, 0, 2, road::STRAIGHT);
    set(3, 0, 2, road::TEE);
    set(4, 0, 2, road::STRAIGHT);
    set(5, 0, 2, road::STRAIGHT);
    set(6, 0, 2, road::CORNER);
    set(3, 1, 0, road::END);
    set(6, 1, 0, road::STRAIGHT);
    set(6, 2, 1, road::TEE); // W+N+S: down to pipe A
    set(6, 3, 0, road::END);
    set(5, 2, 2, road::STRAIGHT);
    set(4, 2, 2, road::STRAIGHT);
    set(3, 2, 2, road::END);
    // pipe B's island with a second Toad House
    set(0, 3, 2, road::END);
    set(1, 3, 2, road::STRAIGHT);
    set(2, 3, 3, road::END);
    let point = |kind, x, y, course: Option<u8>| {
        Some(Point {
            kind,
            icon: 0,
            x,
            y,
            course,
            flags: 0,
        })
    };
    w.points[0] = point(PointKind::Start, 0, 0, None);
    // the course spots in walking order; the last course of the world is the goal
    let spots = [(1, 0), (3, 0), (5, 0), (5, 2)];
    let regular = courses.saturating_sub(1).min(spots.len());
    for (i, &(x, y)) in spots.iter().take(regular).enumerate() {
        w.points[1 + i] = point(PointKind::Course, x, y, Some(first_slot + i as u8));
    }
    w.points[5] = point(PointKind::Goal, 3, 2, Some(first_slot + regular as u8));
    w.toad_houses[0] = Some(ToadHouse {
        x: 3,
        y: 1,
        kind: 0,
    });
    w.toad_houses[1] = Some(ToadHouse {
        x: 2,
        y: 3,
        kind: 1,
    });
    w.pipes = [Some((6, 3)), Some((0, 3))];
    w
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Lower-case hex of `b` (how hosts hand the decrypted map over).
pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// MARK: the map

/// What stands on a map cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Road,
    Start,
    /// A course: its index in the world's course order (`goal`: the last one).
    Course {
        order: usize,
        goal: bool,
    },
    /// Warp pipe 0 or 1.
    Pipe(usize),
    ToadHouse(usize),
}

/// What is at (x, y) of world `w`.
pub fn spot(w: &World, x: i32, y: i32) -> Spot {
    let order = w.course_order();
    for (i, p) in w.points.iter().enumerate() {
        let Some(p) = p else { continue };
        if (p.x as i32, p.y as i32) != (x, y) {
            continue;
        }
        if let Some(k) = order.iter().position(|&o| o == i) {
            return Spot::Course {
                order: k,
                goal: p.kind == PointKind::Goal,
            };
        }
        if p.kind == PointKind::Start {
            return Spot::Start;
        }
    }
    for (k, p) in w.pipes.iter().enumerate() {
        if p.is_some_and(|(px, py)| (px as i32, py as i32) == (x, y)) {
            return Spot::Pipe(k);
        }
    }
    for (k, t) in w.toad_houses.iter().enumerate() {
        if t.is_some_and(|t| (t.x as i32, t.y as i32) == (x, y)) {
            return Spot::ToadHouse(k);
        }
    }
    Spot::Road
}

/// The cells Mario can reach in world `w` with the courses `cleared` (bit per course order):
/// everything joined to the start, not walking through an uncleared course (the course itself
/// is reachable), pipes joining their pair. Bit y·7 + x.
pub fn reachable(w: &World, cleared: u8) -> u32 {
    let Some(s) = w.start() else {
        return 0;
    };
    let mut seen = 0u32;
    let mut queue = vec![(s.x as i32, s.y as i32)];
    while let Some((x, y)) = queue.pop() {
        let bit = 1u32 << (y as usize * GRID_W + x as usize);
        if seen & bit != 0 {
            continue;
        }
        seen |= bit;
        match spot(w, x, y) {
            Spot::Course { order, .. } if cleared & (1 << order) == 0 => continue,
            Spot::Pipe(k) => {
                if let Some((px, py)) = w.pipes[1 - k] {
                    queue.push((px as i32, py as i32));
                }
            }
            _ => {}
        }
        let l = w.links(x, y);
        for (d, dx, dy) in DIRECTIONS {
            if l & d != 0 {
                queue.push((x + dx, y + dy));
            }
        }
    }
    seen
}

fn bit(x: i32, y: i32) -> u32 {
    1u32 << (y as usize * GRID_W + x as usize)
}

/// Where Mario stops walking from (x, y) in direction `d`: the cells walked through, ending at
/// the next point, pipe, Toad House, fork or dead end (an empty route: he can't go that way).
/// Walking away from an uncleared course only goes back the way into it.
pub fn route(w: &World, cleared: u8, x: i32, y: i32, d: u8) -> Vec<(i32, i32)> {
    let reach = reachable(w, cleared);
    let mut out = Vec::new();
    let (mut cx, mut cy, mut dir) = (x, y, d);
    for _ in 0..GRID_W * GRID_H {
        if w.links(cx, cy) & dir == 0 {
            break;
        }
        let (_, dx, dy) = DIRECTIONS.iter().copied().find(|t| t.0 == dir).unwrap();
        let (nx, ny) = (cx + dx, cy + dy);
        if reach & bit(nx, ny) == 0 {
            break;
        }
        out.push((nx, ny));
        let l = w.links(nx, ny);
        let back = world::opposite(dir);
        let onward = l & !back;
        if spot(w, nx, ny) != Spot::Road || onward.count_ones() != 1 {
            break;
        }
        (cx, cy, dir) = (nx, ny, onward);
    }
    out
}

// MARK: progress

/// A play of one Super World (the game's progress record, kept as JSON).
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub world: usize,
    pub x: u8,
    pub y: u8,
    pub lives: u32,
    pub score: u32,
    pub coins: u32,
    /// Per world, the cleared courses (bit per course order).
    pub clears: [u8; world::MAX_WORLDS],
    /// Per world, the Toad Houses used (bit per house).
    pub toads: [u8; world::MAX_WORLDS],
    /// The last world's goal is cleared.
    pub finished: bool,
}

/// What a course clear led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearResult {
    /// Back on the map of the same world.
    Map,
    /// The world's goal: on to the next world.
    WorldClear,
    /// The last world's goal: the Super World is cleared.
    Finished,
}

impl Progress {
    /// A new play: world 1's start, the map's lives.
    pub fn new(sw: &SuperWorld) -> Self {
        let (x, y) = start_of(sw, 0);
        Self {
            world: 0,
            x,
            y,
            lives: sw.map.lives.max(1) as u32,
            score: 0,
            coins: 0,
            clears: [0; world::MAX_WORLDS],
            toads: [0; world::MAX_WORLDS],
            finished: false,
        }
    }

    pub fn world_map<'a>(&self, sw: &'a SuperWorld) -> Option<&'a World> {
        sw.map.worlds.get(self.world)
    }

    pub fn cleared(&self, w: usize, order: usize) -> bool {
        self.clears.get(w).is_some_and(|c| c & (1 << order) != 0)
    }

    /// Courses cleared in the whole world.
    pub fn cleared_count(&self) -> usize {
        self.clears.iter().map(|c| c.count_ones() as usize).sum()
    }

    /// The course order index under Mario, if he stands on a course.
    pub fn course_here(&self, sw: &SuperWorld) -> Option<(usize, bool)> {
        match spot(self.world_map(sw)?, self.x as i32, self.y as i32) {
            Spot::Course { order, goal } => Some((order, goal)),
            _ => None,
        }
    }

    /// The course under Mario was cleared.
    pub fn clear(&mut self, sw: &SuperWorld, order: usize) -> ClearResult {
        let w = self.world;
        if let Some(c) = self.clears.get_mut(w) {
            *c |= 1 << order;
        }
        let goal = sw.courses.get(w).is_some_and(|c| order + 1 == c.len());
        if !goal {
            return ClearResult::Map;
        }
        if w + 1 >= sw.world_count() {
            self.finished = true;
            return ClearResult::Finished;
        }
        self.world = w + 1;
        (self.x, self.y) = start_of(sw, self.world);
        ClearResult::WorldClear
    }

    /// A death in a course: false when that was the last life (a game over).
    pub fn died(&mut self) -> bool {
        self.lives = self.lives.saturating_sub(1);
        self.lives > 0
    }

    /// After a game over: the lives refilled and Mario back at the world's start; the courses
    /// cleared stay cleared and the Toad Houses open again [inf].
    pub fn game_over(&mut self, sw: &SuperWorld) {
        self.lives = sw.map.lives.max(1) as u32;
        (self.x, self.y) = start_of(sw, self.world);
        if let Some(t) = self.toads.get_mut(self.world) {
            *t = 0;
        }
    }

    pub fn add_lives(&mut self, n: u32) {
        self.lives = (self.lives + n).min(MAX_LIVES);
    }

    /// A Toad House under Mario: the lives it gives (once per house until a game over), 0 when
    /// used. SMCraft has no minigames yet: a fixed gift by the house's kind [departure].
    pub fn toad_house(&mut self, k: usize) -> u32 {
        let Some(t) = self.toads.get_mut(self.world) else {
            return 0;
        };
        if *t & (1 << k) != 0 {
            return 0;
        }
        *t |= 1 << k;
        let n = 3;
        self.add_lives(n);
        n
    }

    pub fn to_json(&self) -> String {
        json!({
            "version": 1, "world": self.world, "x": self.x, "y": self.y, "lives": self.lives,
            "score": self.score, "coins": self.coins, "clears": self.clears.to_vec(),
            "toads": self.toads.to_vec(), "finished": self.finished,
        })
        .to_string()
    }

    /// A saved play of `sw`, if it still fits the map (the game's checks: every world before
    /// the current one has its goal cleared, none after has a clear, Mario stands somewhere
    /// reachable).
    pub fn from_json(s: &str, sw: &SuperWorld) -> Option<Self> {
        let v: Value = serde_json::from_str(s).ok()?;
        let bytes = |k: &str| -> [u8; world::MAX_WORLDS] {
            let mut out = [0u8; world::MAX_WORLDS];
            for (o, b) in out.iter_mut().zip(v[k].as_array().into_iter().flatten()) {
                *o = b.as_u64().unwrap_or(0) as u8;
            }
            out
        };
        let p = Self {
            world: v["world"].as_u64()? as usize,
            x: v["x"].as_u64()? as u8,
            y: v["y"].as_u64()? as u8,
            lives: (v["lives"].as_u64()? as u32).clamp(1, MAX_LIVES),
            score: v["score"].as_u64().unwrap_or(0) as u32,
            coins: v["coins"].as_u64().unwrap_or(0) as u32,
            clears: bytes("clears"),
            toads: bytes("toads"),
            finished: v["finished"].as_bool().unwrap_or(false),
        };
        p.valid(sw).then_some(p)
    }

    fn valid(&self, sw: &SuperWorld) -> bool {
        let Some(w) = sw.map.worlds.get(self.world) else {
            return false;
        };
        for (i, c) in sw.courses.iter().enumerate() {
            let n = c.len();
            let done = self.clears[i];
            if i < self.world && n > 0 && done & (1 << (n - 1)) == 0 {
                return false;
            }
            if i > self.world && done != 0 {
                return false;
            }
        }
        (self.x as usize) < GRID_W
            && (self.y as usize) < GRID_H
            && reachable(w, self.clears[self.world]) & bit(self.x as i32, self.y as i32) != 0
    }
}

fn start_of(sw: &SuperWorld, w: usize) -> (u8, u8) {
    sw.map
        .worlds
        .get(w)
        .and_then(World::start)
        .map_or((0, 0), |p| (p.x, p.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn courses(n: usize) -> Vec<SwCourse> {
        (0..n)
            .map(|i| SwCourse {
                id: format!("lib/{i}.course"),
                name: format!("Course {i}"),
                style: "M1".into(),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn test_world_shape() {
        let sw = SuperWorld::test_world(&courses(12)).unwrap();
        assert_eq!(sw.world_count(), 3);
        assert_eq!(sw.course_count(), 12);
        assert_eq!(sw.courses[2].len(), 2);
        assert_eq!(sw.map.course_count(), 12);
        // the map round-trips through the game's format and keeps its course order
        let back = WorldMap::parse(&sw.map.to_bytes()).unwrap();
        assert_eq!(back, sw.map);
        let w = &sw.map.worlds[0];
        assert_eq!(
            spot(w, 3, 2),
            Spot::Course {
                order: 4,
                goal: true
            }
        );
        assert_eq!(
            spot(w, 5, 2),
            Spot::Course {
                order: 3,
                goal: false
            }
        );
        assert_eq!(spot(w, 3, 1), Spot::ToadHouse(0));
        assert_eq!(spot(w, 6, 3), Spot::Pipe(0));
        assert_eq!(spot(w, 2, 3), Spot::ToadHouse(1));
        // a two-course world: one course, then the goal
        let w2 = &sw.map.worlds[2];
        assert_eq!(w2.course_order().len(), 2);
        assert_eq!(
            spot(w2, 1, 0),
            Spot::Course {
                order: 0,
                goal: false
            }
        );
        assert!(SuperWorld::test_world(&[]).is_none());
    }

    #[test]
    fn walking_stops_at_points_and_uncleared_courses_block() {
        let sw = SuperWorld::test_world(&courses(5)).unwrap();
        let w = &sw.map.worlds[0];
        // from the start east: the first course is the next stop
        assert_eq!(route(w, 0, 0, 0, EAST), vec![(1, 0)]);
        // past an uncleared course: no way on, back west is fine
        assert!(route(w, 0, 1, 0, EAST).is_empty());
        assert_eq!(route(w, 0, 1, 0, WEST), vec![(0, 0)]);
        // cleared: on to the fork at the second course
        assert_eq!(route(w, 1, 1, 0, EAST), vec![(2, 0), (3, 0)]);
        // the third course round the corner to the fork, then the fourth, then the goal
        assert_eq!(route(w, 0b0111, 5, 0, EAST), vec![(6, 0), (6, 1), (6, 2)]);
        assert_eq!(route(w, 0b0111, 6, 2, WEST), vec![(5, 2)]);
        assert_eq!(route(w, 0b1111, 5, 2, WEST), vec![(4, 2), (3, 2)]);
        assert_eq!(route(w, 0b0111, 6, 2, SOUTH), vec![(6, 3)]);
        // the island is reached through the pipes only, once the third course is cleared
        assert_eq!(reachable(w, 0b0011) & bit(1, 3), 0);
        assert_ne!(reachable(w, 0b0111) & bit(1, 3), 0);
        assert_eq!(route(w, 0b0111, 0, 3, EAST), vec![(1, 3), (2, 3)]);
        assert!(route(w, 0, 0, 0, NORTH).is_empty());
    }

    #[test]
    fn rules_and_progress() {
        let sw = SuperWorld::test_world(&courses(7)).unwrap();
        let mut p = Progress::new(&sw);
        assert_eq!((p.world, p.x, p.y, p.lives), (0, 0, 0, 30));
        for k in 0..4 {
            assert_eq!(p.clear(&sw, k), ClearResult::Map);
        }
        assert_eq!(p.clear(&sw, 4), ClearResult::WorldClear);
        assert_eq!((p.world, p.x, p.y), (1, 0, 0));
        assert_eq!(p.toad_house(0), 3);
        assert_eq!(p.toad_house(0), 0);
        assert_eq!(p.lives, 33);
        p.lives = 1;
        assert!(!p.died());
        p.game_over(&sw);
        assert_eq!((p.lives, p.toads[1], p.clears[0]), (30, 0, 0b11111));
        // saved and loaded back
        let back = Progress::from_json(&p.to_json(), &sw).unwrap();
        assert_eq!(back, p);
        // a record that doesn't fit the map is dropped
        let mut bad = p.clone();
        bad.clears[0] = 0;
        assert!(Progress::from_json(&bad.to_json(), &sw).is_none());
        bad = p.clone();
        bad.x = 6;
        bad.y = 3;
        assert!(Progress::from_json(&bad.to_json(), &sw).is_none());
        // world 2's goal ends it
        p.clear(&sw, 0);
        assert_eq!(p.clear(&sw, 1), ClearResult::Finished);
        assert!(p.finished);
    }

    #[test]
    fn downloaded_world_from_json() {
        let src = SuperWorld::test_world(&courses(6)).unwrap();
        let list: Vec<Value> = (0..6)
            .map(|i| json!({"code": format!("AAA-BBB-00{i}"), "name": format!("N{i}"), "maker": "M", "style": "MW"}))
            .collect();
        let v = json!({
            "id": "0123456789abcdef_20260101000000000000", "planet_type": 6,
            "maker": {"name": "Ada"}, "map": to_hex(&src.map.to_bytes()), "courses": list,
        });
        let sw = SuperWorld::from_json(&v).unwrap();
        assert_eq!(sw.map, src.map);
        assert_eq!((sw.planet, sw.maker.as_deref()), (6, Some("Ada")));
        assert_eq!(sw.courses[0].len(), 5);
        assert_eq!(sw.courses[1][0].id, "AAA-BBB-005");
        assert!(sw.courses[1][0].online);
        assert!(SuperWorld::from_json(&json!({"map": "zz"})).is_err());
    }
}
