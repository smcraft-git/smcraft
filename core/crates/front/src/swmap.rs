//! The Super World map drawn the game's way (private spec `research/specs/super-worlds.md` §7,
//! v3.0.3 addresses): the theme's backdrop model, the field built from the world's cells, the
//! road pieces, the point models (by icon and state), pipes and Toad Houses, and the walking
//! player, all from the UI import's `ui/myworld/map.json` and its textures (`mmk_import::
//! smm2_myworld`).
//!
//! The game's map models are flat textured quads at small depths seen from straight above, so
//! the map is drawn as layout quads (DECISIONS.md "Super World map: flat quads in the UI pack"):
//! every front end already draws those. World units are the game's (1 unit = 1 texel, a cell is
//! 48 [obs: cell centres x·48 + 24, y·48 + 24 at 0x710146bbe0]), y up; the world's row 0 is the
//! bottom row [obs: the field builder 0x71014519a0 takes (x, y + 48) as the neighbour its atlas
//! pieces draw at their top; the art (cliffs under land, cloud undersides) is upright then].

use std::collections::HashMap;

use mmk_formats::smm2::world::{self, Cell, EAST, GRID_H, GRID_W, NORTH, SOUTH, WEST, World, road};
use mmk_modes::superworld::{self as sw, Progress, Spot, SuperWorld};
use mmk_ui::Quad;
use serde_json::Value;

use crate::layout::Ui;

/// Where the import puts the map's models and tables.
pub const MAP_FILE: &str = "ui/myworld/map.json";
/// One cell of the map (world units) [obs 0x7102318b28].
pub const CELL: f32 = 48.0;
/// The field grid: two columns beyond the map each side, one row below and above
/// [obs 0x71014519a0: 11 × 6, centres (48c − 72, 48r − 24)].
pub const FIELD_W: usize = 11;
pub const FIELD_H: usize = 6;
/// Field piece: full land (atlas row 2, column 15).
pub const FULL_LAND: u8 = 47;
/// Field piece: none drawn.
pub const NO_PIECE: u8 = 0xff;
/// The "nothing area" set follows the water set in the atlas [obs: table value + 0x30].
pub const MAIN_SET: u8 = 48;
/// The map camera: a fixed orthographic view of 460.8 × 259.2 world units (16:9) over (168,
/// 110) — the map's middle across, 14 above it (room under the HUD) —, onto 1280 × 720; no
/// following, zoom or lead [obs v3.0.3 0x710170b610: the map scene's update hands the game
/// camera 0x7101332290 the size 460.8 × 259.2 and the place (168, 110 + a shift + a quake),
/// and records the view as x −62.4 … 398.4, y ± 129.6; 0x710132ee70 makes the projection
/// from them].
pub const VIEW_W: f32 = 460.8;
pub const VIEW_H: f32 = 259.2;
pub const VIEW_CENTER: [f32; 2] = [168.0, 110.0];
pub const SCALE: f32 = 720.0 / VIEW_H;
/// The camera's shift while a course is entered with the button (and while a world opens):
/// 16 down, eased in over 15 frames, and back up the same way after [obs 0x710170b610: the
/// view's +0x38 between 0 and −16 over 15 frames, demo states 1 / 2 (`cOpening`,
/// `cCourseIn_Normal`); inf: the ease is drawn linear, its curve (0x7102c04a48) not read].
pub const COURSE_IN_SHIFT: f32 = -16.0;
pub const SHIFT_FRAMES: i32 = 15;
/// The map's middle (world units): where the backdrop stands [obs 0x710170b610: the DV model at
/// (168, 96)].
pub const MAP_CENTER: [f32; 2] = [GRID_W as f32 * CELL / 2.0, GRID_H as f32 * CELL / 2.0];

/// The scene skins in theme order [obs: the field textures registered by theme at
/// 0x71017065xx, the BGM table 0x710294bb50, the point variants of 0x710143d170 (snow = 4,
/// magma = 6, night = 7)] — the same as `world::THEMES`.
pub const SKINS: [&str; 8] = world::THEMES;

/// The world centre of cell (x, y) (y = the world's row, 0 at the bottom).
pub fn cell_world(x: f32, y: f32) -> [f32; 2] {
    [x * CELL + CELL / 2.0, y * CELL + CELL / 2.0]
}

/// World → layout units (1280 × 720, origin centre, y up) through the map camera.
pub fn to_layout(p: [f32; 2]) -> [f32; 2] {
    [
        (p[0] - VIEW_CENTER[0]) * SCALE,
        (p[1] - VIEW_CENTER[1]) * SCALE,
    ]
}

/// The camera's shift `since` frames after a course-in began (`entering`) or ended.
pub fn camera_shift(entering: bool, since: i32) -> f32 {
    let t = (since as f32 / SHIFT_FRAMES as f32).clamp(0.0, 1.0);
    if entering {
        COURSE_IN_SHIFT * t
    } else {
        COURSE_IN_SHIFT * (1.0 - t)
    }
}

// MARK: the field

/// The water pieces of a field atlas in their order (16 to a row of 48-texel pieces): which
/// sides of the water cell stay open — their neighbour is water too —, the corners after `+`
/// [obs: the atlases; the game's 256-entry lookup main+0x23cfe48 agrees with this order for
/// every neighbour mask].
const PIECES: [&str; 47] = [
    "",
    "E",
    "EW",
    "W",
    "S",
    "NS",
    "N",
    "NW",
    "NE",
    "SW",
    "ES",
    "NEW",
    "ESW",
    "NSW",
    "NES",
    "NESW",
    "NES+SE",
    "NSW+SW",
    "NES+NE",
    "NSW+NW",
    "ESW+SE",
    "ESW+SW",
    "NEW+NE",
    "NEW+NW",
    "NESW+SE",
    "NESW+SW",
    "NESW+NE",
    "NESW+NW",
    "NESW+SE+SW",
    "NESW+NE+NW",
    "NESW+NE+SE",
    "NESW+SW+NW",
    "NESW+SE+NW",
    "NESW+NE+SW",
    "ES+SE",
    "ESW+SE+SW",
    "SW+SW",
    "NES+NE+SE",
    "NESW+NE+SE+SW+NW",
    "NSW+SW+NW",
    "NE+NE",
    "NEW+NE+NW",
    "NW+NW",
    "NESW+NE+SE+SW",
    "NESW+SE+SW+NW",
    "NESW+NE+SE+NW",
    "NESW+NE+SW+NW",
];

/// Neighbour bits of a field mask, in the game's order [obs 0x71014519a0: samples (x − 48,
/// y + 48), (x, y + 48), (x + 48, y + 48), (x − 48, y), (x + 48, y), (x − 48, y − 48), (x, y − 48),
/// (x + 48, y − 48)]: name, dx, dy (cells, y up).
pub const NEIGHBOURS: [(&str, i32, i32); 8] = [
    ("NW", -1, 1),
    ("N", 0, 1),
    ("NE", 1, 1),
    ("W", -1, 0),
    ("E", 1, 0),
    ("SW", -1, -1),
    ("S", 0, -1),
    ("SE", 1, -1),
];

fn bit_of(name: &str) -> u8 {
    1 << NEIGHBOURS
        .iter()
        .position(|n| n.0 == name)
        .expect("a direction")
}

/// A piece's open sides as neighbour bits.
fn open_bits(piece: &str) -> u8 {
    let mut parts = piece.split('+');
    let sides = parts.next().unwrap_or("");
    let mut out = 0;
    for c in sides.chars() {
        out |= bit_of(&c.to_string());
    }
    for corner in parts {
        out |= bit_of(corner);
    }
    out
}

/// The piece for every mask of land neighbours: a side is open when its neighbour is not land,
/// a corner when both its sides and the corner are open.
pub fn piece_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    for (mask, slot) in t.iter_mut().enumerate() {
        let land = mask as u8;
        let mut open = 0u8;
        for s in ["N", "E", "S", "W"] {
            if land & bit_of(s) == 0 {
                open |= bit_of(s);
            }
        }
        for (c, a, b) in [
            ("NE", "N", "E"),
            ("SE", "S", "E"),
            ("SW", "S", "W"),
            ("NW", "N", "W"),
        ] {
            if open & bit_of(a) != 0 && open & bit_of(b) != 0 && land & bit_of(c) == 0 {
                open |= bit_of(c);
            }
        }
        *slot = PIECES
            .iter()
            .position(|p| open_bits(p) == open)
            .expect("every mask has a piece") as u8;
    }
    t
}

/// The cell holds land for the field: a road piece that is not a water route (flag 1) or a
/// bridge, a ledge, or a decoration [obs 0x71014519a0: cell actors of types other than 7 whose
/// code's low nibble is not 1; a ledge's own flags at +0xbd4 / +0xbd5 are not read (inf: land)].
fn land_cell(c: &Cell) -> bool {
    match c.kind {
        road::NONE | road::BRIDGE => false,
        road::LEDGE => true,
        _ => c.flag & 0xf != 1,
    }
}

/// The cell is water under a road (a water route or a bridge).
pub fn water_route(c: &Cell) -> bool {
    match c.kind {
        road::NONE | road::LEDGE | 1 => false,
        road::BRIDGE => true,
        _ => c.flag & 0xf == 1,
    }
}

/// The field: per grid position (row 0 = below the map), the water layer's piece and the
/// "nothing area" layer's piece ([`NO_PIECE`]: none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub water: [[u8; FIELD_W]; FIELD_H],
    pub main: [[u8; FIELD_W]; FIELD_H],
}

/// Per ledge cell, whether land stands at the cell above it (`upper`) and at its own (`own`)
/// [obs v3.0.3 0x7101445230: the ledge actor's +0xbd4 / +0xbd5].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LedgeLand {
    pub upper: bool,
    pub own: bool,
}

/// The ledges' land flags [obs v3.0.3 0x7101445230]. A ledge covers its own cell and the one
/// above it. It sorts what stands beside it into land — a road piece (not a bridge or a water
/// route), out of the map or an empty cell in a "Main" theme (`sub` false), a ledge whose flag
/// for that cell is set — or not; its upper flag is land up-left or up-right of it, its own
/// flag land below, left, right or above it (its own upper cell included). The game sets them
/// once per ledge in build order; SMCraft takes them to their fixed point [inf: the order].
pub fn ledge_land(w: &World, sub: bool) -> [[Option<LedgeLand>; GRID_W]; GRID_H] {
    let ledge = |x: i32, y: i32| w.cell(x, y).is_some_and(|c| c.kind == road::LEDGE);
    let mut flags = [[None; GRID_W]; GRID_H];
    for (y, row) in flags.iter_mut().enumerate() {
        for (x, f) in row.iter_mut().enumerate() {
            if ledge(x as i32, y as i32) {
                *f = Some(LedgeLand::default());
            }
        }
    }
    let get = |flags: &[[Option<LedgeLand>; GRID_W]; GRID_H], x: i32, y: i32| {
        w.cell(x, y)
            .and_then(|_| flags[y as usize][x as usize])
            .unwrap_or_default()
    };
    // land at (x, y) as a ledge's neighbour sees it
    let land = |flags: &[[Option<LedgeLand>; GRID_W]; GRID_H], x: i32, y: i32| {
        let Some(c) = w.cell(x, y) else {
            return !sub;
        };
        if ledge(x, y) {
            return get(flags, x, y).own;
        }
        if ledge(x, y - 1) {
            return get(flags, x, y - 1).upper;
        }
        match c.kind {
            road::NONE => !sub && !w.occupied(x, y),
            road::BRIDGE => false,
            _ => c.flag & 0xf == 0,
        }
    };
    loop {
        let mut changed = false;
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                let Some(old) = flags[y as usize][x as usize] else {
                    continue;
                };
                // the upper flag first: the own one reads it through the cell above
                let upper = old.upper || land(&flags, x + 1, y + 1) || land(&flags, x - 1, y + 1);
                flags[y as usize][x as usize] = Some(LedgeLand { upper, ..old });
                let own = old.own
                    || [(0, -1), (1, 0), (-1, 0), (0, 1)]
                        .iter()
                        .any(|&(dx, dy)| land(&flags, x + dx, y + dy));
                let next = LedgeLand { upper, own };
                flags[y as usize][x as usize] = Some(next);
                changed |= next != old;
            }
        }
        if !changed {
            return flags;
        }
    }
}

/// The field counts (x, y) as land: a land cell; a ledge's own cell or the cell above it by
/// its flags [obs v3.0.3 0x7101445a70: a ledge answers +0xbd5 at its own place, +0xbd4 above].
fn is_land(w: &World, ledges: &[[Option<LedgeLand>; GRID_W]; GRID_H], x: i32, y: i32) -> bool {
    let flag = |x: i32, y: i32| w.cell(x, y).and_then(|_| ledges[y as usize][x as usize]);
    if let Some(f) = flag(x, y) {
        return f.own;
    }
    flag(x, y - 1).is_some_and(|f| f.upper) || w.cell(x, y).is_some_and(land_cell)
}

/// Build the field of world `w` (`sub`: the theme's empty area is water, SceneDB
/// `NothingAreaAttr` "Sub"), as the game's builder [obs 0x71014519a0]:
/// - water layer: in a "Sub" theme every position without land gets the water piece of its
///   land neighbours (out of the map counts as land); a land cell (a road piece, or the point a
///   ledge's ladder climbs to) gets the full land piece [inf: the game leaves it to the cell,
///   0xff]. In a "Main" theme everything is full land but
///   a water route or bridge cell, which gets its water piece;
/// - "nothing area" layer: every position without a road piece gets the piece of the
///   neighbours that have one, from the atlas rows 3–5 (forest, rocks… the "Main" themes draw).
pub fn field(w: &World, sub: bool) -> Field {
    let table = piece_table();
    let cell = |x: i32, y: i32| w.cell(x, y);
    // a ledge's cells by its flags
    let ledges = ledge_land(w, sub);
    let land = |x: i32, y: i32| is_land(w, &ledges, x, y);
    let outside = |x: i32, y: i32| cell(x, y).is_none();
    let has_piece = |x: i32, y: i32| cell(x, y).is_some_and(|c| c.kind != road::NONE);
    let mut f = Field {
        water: [[NO_PIECE; FIELD_W]; FIELD_H],
        main: [[NO_PIECE; FIELD_W]; FIELD_H],
    };
    for r in 0..FIELD_H {
        for c in 0..FIELD_W {
            let (x, y) = (c as i32 - 2, r as i32 - 1);
            let water_mask = || {
                let mut m = 0u8;
                for (i, (_, dx, dy)) in NEIGHBOURS.iter().enumerate() {
                    let (nx, ny) = (x + dx, y + dy);
                    if outside(nx, ny) || land(nx, ny) {
                        m |= 1 << i;
                    }
                }
                m
            };
            f.water[r][c] = if sub {
                if land(x, y) {
                    FULL_LAND
                } else {
                    table[water_mask() as usize]
                }
            } else if cell(x, y).is_some_and(water_route) {
                table[water_mask() as usize]
            } else {
                FULL_LAND
            };
            if !has_piece(x, y) {
                let mut m = 0u8;
                for (i, (_, dx, dy)) in NEIGHBOURS.iter().enumerate() {
                    if has_piece(x + dx, y + dy) {
                        m |= 1 << i;
                    }
                }
                f.main[r][c] = table[m as usize] + MAIN_SET;
            }
        }
    }
    f
}

// MARK: the ledges' cliffs

/// Field atlas pieces of the ledges' cliffs, rows 6–7, each set a single piece then a left,
/// middle and right one [obs: the atlases]: the plateau's top, the cliff face over land, the
/// face meeting water (then a water and a land tile). Row 7 (+16: clouds, bushes, water blobs)
/// is the same set for the "B" ledges [obs v3.0.3 0x7101444b20: + 0x10 when the cell's flag & 3
/// is 1].
pub const CLIFF_TOP: u8 = 96;
pub const CLIFF_FACE: u8 = 100;
pub const CLIFF_FACE_WATER: u8 = 104;
pub const CLIFF_ROW_B: u8 = 16;

/// One cliff piece: the atlas piece and the cell it covers (the world's x, y).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliffPiece {
    pub piece: u8,
    pub x: i32,
    pub y: i32,
}

/// The cliffs of the ledges, as the ledge actor draws them [obs v3.0.3 0x7101444b20, with the
/// flags of [`ledge_land`]]: per ledge, full land under its own cell when that is land, the face
/// over it (over land, else meeting the water), full land over the cell above when that is
/// land, and the plateau's top there; single, or the left / middle / right piece of a row by
/// the ledges of the same kind beside it on its row; the "B" ledges (flag & 3 = 1) from row 7.
pub fn cliffs(w: &World, sub: bool) -> Vec<CliffPiece> {
    let flags = ledge_land(w, sub);
    let kind = |x: i32, y: i32| {
        w.cell(x, y)
            .filter(|c| c.kind == road::LEDGE)
            .map(|c| c.flag & 3 == 1)
    };
    let mut out = Vec::new();
    for y in 0..GRID_H as i32 {
        for x in 0..GRID_W as i32 {
            let (Some(b), Some(land)) = (kind(x, y), flags[y as usize][x as usize]) else {
                continue;
            };
            let joined = |dx: i32| kind(x + dx, y) == Some(b);
            let at = match (joined(-1), joined(1)) {
                (false, false) => 0,
                (false, true) => 1,
                (true, true) => 2,
                (true, false) => 3,
            };
            let row = if b { CLIFF_ROW_B } else { 0 };
            if land.own {
                out.push(CliffPiece {
                    piece: FULL_LAND,
                    x,
                    y,
                });
            }
            let face = if land.own {
                CLIFF_FACE
            } else {
                CLIFF_FACE_WATER
            };
            out.push(CliffPiece {
                piece: face + at + row,
                x,
                y,
            });
            if land.upper {
                out.push(CliffPiece {
                    piece: FULL_LAND,
                    x,
                    y: y + 1,
                });
            }
            out.push(CliffPiece {
                piece: CLIFF_TOP + at + row,
                x,
                y: y + 1,
            });
        }
    }
    out
}

// MARK: models chosen

/// The course point model of `icon` in theme `theme` [obs 0x710143d170: the models in icon
/// order; enemy B and D have a magma variant (theme 6), E a night one (7), G a snow one (4)].
pub fn point_model(icon: u8, theme: u8) -> String {
    let base = match icon {
        0 => "normal",
        1 => "water",
        2 => "desert",
        3 => "snow",
        4 => "woods",
        5 => "hauntedhouse",
        6 => "tower",
        7 => "airship",
        8 => "enemyA",
        9 => "enemyB",
        10 => "enemyC",
        11 => "enemyD",
        12 => "enemyE",
        13 => "enemyF",
        14 => "enemyG",
        15 => "enemyH",
        _ => "normal",
    };
    let variant = match (icon, theme) {
        (9 | 11, 6) => "_magma",
        (12, 7) => "_night",
        (14, 4) => "_snow",
        _ => "",
    };
    format!("MyWorld_Point_{base}{variant}")
}

/// The goal's model [obs 0x71014437e0, the goal's model index; the models in the order of
/// 0x7101442b30]: in the last world the end castle, else on a water cell the airship castle,
/// else the castle; each its `_notset` form while no course is set (the maker).
pub fn goal_model(last_world: bool, water: bool, has_course: bool) -> &'static str {
    match (last_world, water, has_course) {
        (true, _, true) => "MyWorld_Point_castle_end",
        (true, _, false) => "MyWorld_Point_castle_end_notset",
        (false, true, true) => "MyWorld_Point_airship_castle",
        (false, true, false) => "MyWorld_Point_airship_castle_notset",
        (false, false, true) => "MyWorld_Point_castle",
        (false, false, false) => "MyWorld_Point_castle_notset",
    }
}

/// The cannon that stands at a cleared goal and shoots the player to the next world (model 6 of
/// 0x7101442b30; its `fire_st` / `fire` are the `CannonPrepareLaunch` / `CannonLaunch` rows).
pub const CANNON: &str = "MyWorld_Point_dokancannon";

/// The course point kinds the map's sounds switch on (`@point`), in icon order [obs: SLink user
/// `MyWorldCoursePoint`'s `tocleared` switch; inf: icon order — the enemy points' order matches
/// G's snow form (Sambo) and E's night one (Wanwan)].
pub const POINT_KINDS: [&str; 16] = [
    "Normal",
    "Water",
    "Desert",
    "Snow",
    "Woods",
    "HauntedHouse",
    "Tower",
    "Airship",
    "Kuribo",
    "Nokonoko",
    "Pakkun",
    "Pukupuku",
    "Wanwan",
    "Bros",
    "Sambo",
    "Kinokopoison",
];

/// The `@point` value of a course point's icon.
pub fn point_kind(icon: u8) -> &'static str {
    POINT_KINDS[(icon as usize).min(15)]
}

/// The model a cell's piece is drawn with in skin `skin`: roads (on water the `_s` ones),
/// curves, bridges, ledges (2fA; 2fB with flag 1 [inf]) and the decorations (A / B / C by the
/// flag [inf]).
pub fn cell_model(c: &Cell, skin: &str) -> Option<String> {
    let s = if water_route(c) { "_s" } else { "" };
    Some(match c.kind {
        1 => format!(
            "MyWorld_Object_{skin}{}",
            ["A", "B", "C"][(c.flag as usize).min(2)]
        ),
        road::END | road::STRAIGHT | road::CORNER | road::TEE => format!("MyWorld_Road_{skin}{s}"),
        road::CURVE => format!("MyWorld_Road_Curve_{skin}{s}"),
        road::BRIDGE => format!("MyWorld_Road_Bridge_{skin}"),
        // the ledge's kind: its flag & 3 (bits 11–12 of the build code) [obs v3.0.3
        // 0x7101445230 / 0x7101444b20; inf: kind 1 is the 2fB model]
        road::LEDGE => format!(
            "MyWorld_Road_2f{}_{skin}",
            if c.flag & 3 == 1 { "B" } else { "A" }
        ),
        _ => return None,
    })
}

/// The road arms of a cell to show, as screen directions: bit 0 east, 1 west, 2 up (the world's
/// row + 1, `world::SOUTH`), 3 down (row − 1, `world::NORTH`).
pub fn road_arms(links: u8) -> u8 {
    let mut out = 0;
    for (i, d) in [EAST, WEST, SOUTH, NORTH].into_iter().enumerate() {
        if links & d != 0 {
            out |= 1 << i;
        }
    }
    out
}

/// Frames a road a clear opened shows its appearing state, and a point it reached its
/// `Passable` pop [inf: the pop's length, 16 frames; the game plays the arms' patterns on from
/// the appearing frame 0x71014812b0].
pub const REVEAL_FRAMES: i32 = 16;

/// The road state frame (the road models' texture patterns) of an arm or a cell's centre
/// piece [obs 0x710144e030: an arm between two reached cells takes 0x710144e390's frame, 1 (3
/// capped), else 0x710144e160's: 0 while it waits for a reveal, 2 / 4 (the ground's shade) as a
/// reveal sets it 0x71014807f0]: None while hidden; the road (1); appearing (2). A centre piece
/// takes the large pad (1 / 2) under a point, pipe or Toad House, the small one (3 / 4) along the
/// road [inf].
pub fn road_frame(shown: bool, appearing: bool, centre_road: bool) -> Option<f32> {
    if !shown {
        return None;
    }
    let base = if centre_road { 3.0 } else { 1.0 };
    Some(if appearing { base + 1.0 } else { base })
}

/// The shape of a ledge model (drawn turned half round) that draws the arm of link `bit`
/// ([`road_arms`]): the long `S` arm, turned, climbs to the row above, where the data puts the
/// points a ledge leads to without road pieces of their own; the ledge models have no arm
/// for the row below [inf: the models' arms are C, E, S (72 long), W; every ledge in the samples
/// has rotation 2].
pub fn ledge_shape(bit: u8) -> u8 {
    match bit {
        1 => 2,
        2 => 1,
        4 => 8,
        _ => 0,
    }
}

/// A road model's centre piece, next to the arm bits of [`road_arms`].
pub const ARM_CENTRE: u8 = 0x10;

/// The piece a road shape draws (by its material `road_E`, `bridge_N`, …): an arm bit as
/// [`road_arms`], or [`ARM_CENTRE`].
fn shape_arm(material: &str) -> Option<u8> {
    Some(match material.rsplit('_').next()? {
        "C" => ARM_CENTRE,
        "E" => 1,
        "W" => 2,
        "N" => 4,
        "S" => 8,
        _ => return None,
    })
}

// MARK: the models of the import

#[derive(Clone, Debug, Default)]
struct Shape {
    mat: String,
    tex: Option<String>,
    blend: u8,
    bone: usize,
    joints: Vec<usize>,
    /// x, y, z, u, v.
    v: Vec<[f32; 5]>,
    i: Vec<u32>,
    /// The texture transform: scale u, v, rotation (radians), translate u, v.
    srt: [f32; 5],
    /// Wrap modes u, v as the quads take them (0 clamp, 1 repeat, 2 mirror).
    wrap: [u32; 2],
}

#[derive(Clone, Debug, Default)]
struct Anim {
    frames: f32,
    looping: bool,
    tex: HashMap<String, Vec<(f32, String)>>,
    skin: HashMap<usize, Vec<(f32, [f32; 6])>>,
    vis: HashMap<usize, Vec<(f32, bool)>>,
}

impl Anim {
    fn local(&self, t: f32) -> f32 {
        if self.frames <= 0.0 {
            0.0
        } else if self.looping {
            t.rem_euclid(self.frames)
        } else {
            t.clamp(0.0, self.frames)
        }
    }

    /// The last key at or before `f` (else the first).
    fn key<T: Clone>(keys: &[(f32, T)], f: f32) -> Option<T> {
        keys.iter()
            .take_while(|(k, _)| *k <= f)
            .last()
            .or(keys.first())
            .map(|(_, v)| v.clone())
    }
}

#[derive(Clone, Debug, Default)]
struct Model {
    shapes: Vec<Shape>,
    anims: HashMap<String, Anim>,
}

fn f32s(v: &Value) -> Vec<f32> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|x| x.as_f64().unwrap_or(0.0) as f32)
        .collect()
}

fn parse_model(v: &Value) -> Model {
    let shapes = v["shapes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let flat = f32s(&s["v"]);
            Shape {
                mat: s["mat"].as_str().unwrap_or_default().to_string(),
                tex: s["tex"].as_str().map(str::to_string),
                blend: s["blend"].as_u64().unwrap_or(0) as u8,
                bone: s["bone"].as_u64().unwrap_or(0) as usize,
                joints: s["joints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|j| j.as_u64().unwrap_or(0) as usize)
                    .collect(),
                v: flat.as_chunks::<5>().0.to_vec(),
                i: s["i"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| x.as_u64().unwrap_or(0) as u32)
                    .collect(),
                srt: {
                    let v = f32s(&s["srt"]);
                    if v.len() == 5 {
                        [v[0], v[1], v[2], v[3], v[4]]
                    } else {
                        [1.0, 1.0, 0.0, 0.0, 0.0]
                    }
                },
                // the import's codes (0 repeat, 1 mirror, 2 clamp) → the quads'
                wrap: {
                    let w = f32s(&s["wrap"]);
                    let code = |i: usize| match w.get(i).copied().unwrap_or(0.0) as u32 {
                        1 => 2,
                        2 => 0,
                        _ => 1,
                    };
                    [code(0), code(1)]
                },
            }
        })
        .collect();
    let mut anims = HashMap::new();
    for (name, a) in v["anims"].as_object().into_iter().flatten() {
        let mut an = Anim {
            frames: a["frames"].as_f64().unwrap_or(0.0) as f32,
            looping: a["loop"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        for (mat, keys) in a["tex"].as_object().into_iter().flatten() {
            let k = keys
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| Some((e[0].as_f64()? as f32, e[1].as_str()?.to_string())))
                .collect();
            an.tex.insert(mat.clone(), k);
        }
        for (bone, keys) in a["skin"].as_object().into_iter().flatten() {
            let k = keys
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    let n = f32s(e);
                    let m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                    let mut out = m;
                    for (o, x) in out.iter_mut().zip(n.iter().skip(1)) {
                        *o = *x;
                    }
                    (n.first().copied().unwrap_or(0.0), out)
                })
                .collect();
            an.skin.insert(bone.parse().unwrap_or(0), k);
        }
        for (bone, keys) in a["vis"].as_object().into_iter().flatten() {
            let k = keys
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| Some((e[0].as_f64()? as f32, e[1].as_u64()? != 0)))
                .collect();
            an.vis.insert(bone.parse().unwrap_or(0), k);
        }
        anims.insert(name.clone(), an);
    }
    Model { shapes, anims }
}

/// One model drawn: where, which animation and how far into it.
#[derive(Clone, Debug)]
pub struct Instance<'a> {
    pub model: &'a str,
    /// Animation (the import's name without `_ftp`), None: the rest pose.
    pub anim: Option<&'a str>,
    /// Frames since the animation started.
    pub time: f32,
    /// World position of the model's origin.
    pub at: [f32; 2],
    /// Mirrored left–right (the player walking west).
    pub flip: bool,
    /// Turned half round (the ledges, see [`ledge_shape`]).
    pub turn: bool,
    /// Draw order: layer first (0 backdrop, 1 field, 2 roads, 3 points and objects, 4 the
    /// player), then higher on the map first, then the model's own depth.
    pub layer: u8,
    /// Only the shapes whose piece bit (see [`road_arms`], [`ARM_CENTRE`]) is set: roads.
    pub arms: Option<u8>,
    /// The texture-pattern frame for road arms (0 hidden, 1 the road, 2 …), by material.
    pub pattern_frame: Option<f32>,
}

/// A quad waiting for the sort.
struct Drawn {
    key: (u8, i32, i32, usize),
    quad: Quad,
}

/// The Super World map's models and tables from the UI import.
pub struct MapArt {
    models: HashMap<String, Model>,
    tables: Value,
}

fn quad_flags() -> u32 {
    mmk_ui::BLEND_NORMAL
        | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_COLOR)
        | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_ALPHA)
}

/// One 48-unit field atlas piece (16 × 8 of them) centred on `center` (world units).
fn atlas_quad(tex: u32, piece: u8, center: [f32; 2]) -> Quad {
    let (u0, v0) = ((piece % 16) as f32 / 16.0, (piece / 16) as f32 / 8.0);
    let (u1, v1) = (u0 + 1.0 / 16.0, v0 + 1.0 / 8.0);
    let h = CELL / 2.0;
    let p = |dx: f32, dy: f32| to_layout([center[0] + dx, center[1] + dy]);
    Quad {
        pos: [p(-h, h), p(h, h), p(-h, -h), p(h, -h)],
        uv0: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]],
        color: [[255; 4]; 4],
        black: [0, 0, 0, 0],
        white: [255; 4],
        tex0: tex,
        tex1: mmk_ui::TEXTURE_NONE,
        flags: quad_flags(),
        ..Default::default()
    }
}

impl MapArt {
    /// The import's map description; None for an import without it (the flat map is drawn).
    pub fn load(ui: &Ui) -> Option<Self> {
        let v = ui.cache().json_file(MAP_FILE)?;
        let models = v["models"]
            .as_object()?
            .iter()
            .map(|(k, m)| (k.clone(), parse_model(m)))
            .collect();
        Some(Self {
            models,
            tables: v["tables"].clone(),
        })
    }

    pub fn has(&self, model: &str) -> bool {
        self.models.contains_key(model)
    }

    pub fn has_anim(&self, model: &str, anim: &str) -> bool {
        self.models
            .get(model)
            .is_some_and(|m| m.anims.contains_key(anim))
    }

    /// The SceneDB row of a theme, by its field model [obs: the field textures are kept in
    /// theme order, SceneDB lists them in another].
    fn scene_row(&self, theme: u8) -> Option<&Value> {
        let skin = SKINS.get(theme as usize)?;
        let want = format!("MyWorld_Field_{skin}");
        self.tables["SceneDB_MyWorld"]
            .as_array()?
            .iter()
            .find(|r| r["FieldModel"].as_str() == Some(want.as_str()))
    }

    /// The theme's empty area is water (`NothingAreaAttr` "Sub").
    pub fn sub_theme(&self, theme: u8) -> bool {
        self.scene_row(theme)
            .and_then(|r| r["NothingAreaAttr"].as_str())
            .is_none_or(|s| s == "Sub")
    }

    /// An animation choice table's name for (skin, kind) (`WM_PointPartsAnime` / `WM_GhostAnime`
    /// column `kind`, `WM_PlayerAnime` column `route`; the import names the columns), the plain
    /// skin's when the skin has no row [inf: the tables leave rows out where the plain one
    /// applies].
    pub fn anim_name(&self, table: &str, kind_key: &str, theme: u8, kind: &str) -> Option<String> {
        let rows = self.tables[table].as_array()?;
        let skin = SKINS.get(theme as usize).copied().unwrap_or("plain");
        let find = |s: &str| {
            rows.iter()
                .find(|r| r["skin"].as_str() == Some(s) && r[kind_key].as_str() == Some(kind))
        };
        find(skin)
            .or_else(|| find("plain"))
            .and_then(|r| r["anim"].as_str())
            .map(str::to_string)
    }

    /// Quads of one model instance into `out`.
    fn emit(&self, ui: &Ui, inst: &Instance, out: &mut Vec<Drawn>) {
        let Some(m) = self.models.get(inst.model) else {
            return;
        };
        let anim = inst.anim.and_then(|a| m.anims.get(a));
        let frame = anim.map_or(0.0, |a| a.local(inst.time));
        for (si, s) in m.shapes.iter().enumerate() {
            if let Some(arms) = inst.arms
                && shape_arm(&s.mat).is_none_or(|bit| arms & bit == 0)
            {
                continue;
            }
            if let Some(a) = anim
                && let Some(keys) = a.vis.get(&s.bone)
                && Anim::key(keys, frame) == Some(false)
            {
                continue;
            }
            // the texture: the pattern of this animation, else of the road state, else the
            // material's own
            let tex = anim
                .and_then(|a| a.tex.get(&s.mat))
                .and_then(|k| Anim::key(k, frame))
                .or_else(|| {
                    let f = inst.pattern_frame?;
                    m.anims
                        .values()
                        .find_map(|a| a.tex.get(&s.mat))
                        .and_then(|k| Anim::key(k, f))
                })
                .or_else(|| s.tex.clone());
            let Some(tex) = tex else { continue };
            let Some(t) = ui.cache_mut().texture_file(inst.model, &tex) else {
                continue;
            };
            let skin_of = |bone: usize| {
                anim.and_then(|a| a.skin.get(&bone))
                    .and_then(|k| Anim::key(k, frame))
                    .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
            };
            let place = |vi: u32| {
                let v = s.v.get(vi as usize).copied().unwrap_or_default();
                let bone = s.joints.get(vi as usize).copied().unwrap_or(s.bone);
                let k = skin_of(bone);
                let x = k[0] * v[0] + k[1] * v[1] + k[4];
                let y = k[2] * v[0] + k[3] * v[1] + k[5];
                let (x, y) = if inst.turn { (-x, -y) } else { (x, y) };
                let x = if inst.flip { -x } else { x };
                let [su, sv, rot, tu, tv] = s.srt;
                let (u, w) = (v[3] * su, v[4] * sv);
                let (sn, cs) = rot.sin_cos();
                let uv = [cs * u - sn * w + tu, sn * u + cs * w + tv];
                (to_layout([inst.at[0] + x, inst.at[1] + y]), uv, v[2])
            };
            for tri in s.i.as_chunks::<3>().0 {
                let (a, b, c) = (place(tri[0]), place(tri[1]), place(tri[2]));
                let z = a.2.max(b.2).max(c.2);
                // a triangle as a quad with its last corner twice (TL-BL-TR, TR-BL-BR)
                let quad = Quad {
                    pos: [a.0, b.0, c.0, c.0],
                    uv0: [a.1, b.1, c.1, c.1],
                    color: [[255; 4]; 4],
                    black: [0, 0, 0, 0],
                    white: [255; 4],
                    tex0: t.id,
                    tex1: mmk_ui::TEXTURE_NONE,
                    flags: quad_flags()
                        | (s.wrap[0] << mmk_ui::SHIFT_WRAP_S0)
                        | (s.wrap[1] << mmk_ui::SHIFT_WRAP_T0)
                        | if s.blend == 3 {
                            (1 << mmk_ui::SHIFT_BLEND_OP)
                                | (4 << mmk_ui::SHIFT_BLEND_SRC)
                                | (1 << mmk_ui::SHIFT_BLEND_DST)
                        } else {
                            0
                        },
                    ..Default::default()
                };
                out.push(Drawn {
                    key: (
                        inst.layer,
                        -(inst.at[1] * 4.0) as i32,
                        (z * 100.0) as i32,
                        out.len() + si,
                    ),
                    quad,
                });
            }
        }
    }

    /// The cliff pieces of the ledges (over the field, under the roads).
    fn emit_cliffs(&self, ui: &Ui, theme: u8, pieces: &[CliffPiece], out: &mut Vec<Drawn>) {
        let skin = SKINS.get(theme as usize).copied().unwrap_or("plain");
        let name = format!("MyWorld_Field_{skin}");
        if pieces.is_empty() {
            return;
        }
        let Some(t) = ui.cache_mut().texture_file(&name, &name) else {
            return;
        };
        for c in pieces {
            let center = cell_world(c.x as f32, c.y as f32);
            out.push(Drawn {
                key: (1, 2, -c.y, out.len()),
                quad: atlas_quad(t.id, c.piece, center),
            });
        }
    }

    /// The field atlas pieces of `field` for theme `theme`.
    fn emit_field(&self, ui: &Ui, theme: u8, field: &Field, out: &mut Vec<Drawn>) {
        let skin = SKINS.get(theme as usize).copied().unwrap_or("plain");
        let name = format!("MyWorld_Field_{skin}");
        let Some(t) = ui.cache_mut().texture_file(&name, &name) else {
            return;
        };
        for (layer, grid) in [&field.water, &field.main].into_iter().enumerate() {
            for (r, row) in grid.iter().enumerate() {
                for (c, &piece) in row.iter().enumerate() {
                    if piece == NO_PIECE {
                        continue;
                    }
                    let center = [c as f32 * CELL - 72.0, r as f32 * CELL - 24.0];
                    out.push(Drawn {
                        key: (1, layer as i32, 0, out.len()),
                        quad: atlas_quad(t.id, piece, center),
                    });
                }
            }
        }
    }
}

// MARK: the scene of one frame

/// A point's state as the animation table names it (`WM_PointPartsAnime` kinds `<State>_Main` /
/// `<State>_Sub` and `AppearGoalPoint`) [obs: the kinds in the game's order, Wait_Main 0 …
/// AppearGoalPoint 10, Passable_Main 11, Passable_Sub 12, picked by index in 0x71014409b0,
/// 0x710143ff00, 0x71014830c0].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointState {
    Wait,
    ToCleared,
    Cleared,
    CannonPrepareLaunch,
    CannonLaunch,
    AppearGoalPoint,
    Passable,
}

impl PointState {
    /// The table's kind.
    fn kind(self, sub: bool) -> String {
        let s = match self {
            PointState::Wait => "Wait",
            PointState::ToCleared => "ToCleared",
            PointState::Cleared => "Cleared",
            PointState::CannonPrepareLaunch => "CannonPrepareLaunch",
            PointState::CannonLaunch => "CannonLaunch",
            PointState::AppearGoalPoint => return "AppearGoalPoint".into(),
            PointState::Passable => "Passable",
        };
        format!("{s}_{}", if sub { "Sub" } else { "Main" })
    }

    /// The animation's own name where the table has no row for the model.
    fn plain(self) -> &'static str {
        match self {
            PointState::Wait => "wait",
            PointState::ToCleared => "tocleared",
            PointState::Cleared => "cleared",
            PointState::CannonPrepareLaunch => "fire_st",
            PointState::CannonLaunch => "fire",
            PointState::AppearGoalPoint => "swim_appear",
            PointState::Passable => "passable",
        }
    }
}

/// What the map scene shows besides the world: the player and its walk, and the moments a clear,
/// a Toad House or a world's end play.
#[derive(Clone, Debug, Default)]
pub struct MapView {
    /// The map's tick (frames).
    pub tick: i32,
    /// The player's cell position (fractional while walking).
    pub player: [f32; 2],
    /// How far above the ground the player is (world units: 12 on a pipe, a hop's arc).
    pub lift: f32,
    /// The camera's shift down (world units, ≤ 0: [`camera_shift`]).
    pub camera_dy: f32,
    /// The walking direction (dx, dy; y up), None standing.
    pub walking: Option<(i32, i32)>,
    /// The last direction walked (which way the player faces standing).
    pub facing: (i32, i32),
    /// Ticks since the player stopped (the standing animation's clock).
    pub since: i32,
    /// The player model (`MyWorld_Player_<Mario|Luigi|Kinopio|Kinopico>Mdl`).
    pub character: usize,
    /// Course order indices cleared on this visit, with the tick their point's ToCleared
    /// started.
    pub just_cleared: Vec<(usize, i32)>,
    /// A clear's reveal: the cells reached before it and the tick its roads and points appear;
    /// before that tick the map stays as it was [obs 0x7101471160 states 5–7].
    pub reveal: Option<(u32, i32)>,
    /// Toad Houses used on this visit, with the tick their ToCleared started.
    pub houses_used: Vec<(usize, i32)>,
    /// The cleared goal's cannon and the tick it started to fire (its prepare, then its launch).
    pub cannon: Option<i32>,
    /// A one-off animation of the player (`tocleared`, `coursein`, `dokan_in`, `landing`; in its
    /// route's form, `swim_tocleared` …) and the tick it started.
    pub player_anim: Option<(&'static str, i32)>,
    /// The player is not drawn (inside the cannon, flying to the next world).
    pub hide_player: bool,
}

const PLAYERS: [&str; 4] = [
    "MyWorld_Player_MarioMdl",
    "MyWorld_Player_LuigiMdl",
    "MyWorld_Player_KinopioMdl",
    "MyWorld_Player_KinopicoMdl",
];

/// The player model of character `c` (0 Mario, 1 Luigi, 2 Toad, 3 Toadette).
pub fn player_model(c: usize) -> &'static str {
    PLAYERS[c.min(3)]
}

/// The SLink users of the map's actors [obs: the resource lists `MyWorld*.barslist`; spec §7.6].
pub const USER_SCENE: &str = "MyWorldScene";
pub const USER_PLAYER: &str = "MyWorldPlayer";
pub const USER_POINT: &str = "MyWorldCoursePoint";
pub const USER_GOAL: &str = "MyWorldGoalPoint";
pub const USER_HOUSE: &str = "MyWorldKinokoHouse";

/// Whether cell (x, y) is set in a [`sw::reachable`] mask.
pub fn in_mask(mask: u32, x: i32, y: i32) -> bool {
    (0..GRID_W as i32).contains(&x)
        && (0..GRID_H as i32).contains(&y)
        && mask & (1 << (y as usize * GRID_W + x as usize)) != 0
}

/// A point stands on water (its `_Sub` animations) [obs 0x7101440d60: the cell's water kind].
pub fn point_sub(w: &World, x: i32, y: i32) -> bool {
    w.cell(x, y).is_some_and(water_route)
}

/// What the map shows of world `w` at `tick`: the cells reached (the reveal's earlier set until
/// it starts) and those a running reveal just opened.
fn reach_at(w: &World, cleared: u8, view: &MapView) -> (u32, u32) {
    let now = sw::reachable(w, cleared);
    match view.reveal {
        Some((before, at)) if view.tick < at => (before, 0),
        Some((before, _)) => (now, now & !before),
        None => (now, 0),
    }
}

impl MapArt {
    /// The route kind of a cell for the animation tables: "main", "sub" (water), "bridge"
    /// [obs 0x7101458350: bridge, then the water flag, else main].
    fn route(c: Option<&Cell>) -> &'static str {
        match c {
            Some(c) if c.kind == road::BRIDGE => "bridge",
            Some(c) if water_route(c) => "sub",
            _ => "main",
        }
    }

    /// The animation of `state` for `model` in theme `theme` (`sub`: the point stands on
    /// water): the table's row (the plain skin's where the theme has none), else the state's own
    /// name, else `wait`.
    pub fn state_anim(
        &self,
        model: &str,
        state: PointState,
        theme: u8,
        sub: bool,
    ) -> Option<String> {
        self.anim_name("WM_PointPartsAnime", "kind", theme, &state.kind(sub))
            .filter(|a| self.has_anim(model, a))
            .or_else(|| {
                let plain = state.plain();
                self.has_anim(model, plain).then(|| plain.to_string())
            })
            .or_else(|| self.has_anim(model, "wait").then(|| "wait".into()))
    }

    /// Frames of animation `anim` of `model` (0 without it).
    pub fn anim_frames(&self, model: &str, anim: &str) -> i32 {
        self.models
            .get(model)
            .and_then(|m| m.anims.get(anim))
            .map_or(0, |a| a.frames as i32)
    }

    /// The model of the point at (x, y) of the progress's world (None: a road or nothing).
    pub fn spot_model(&self, sw: &SuperWorld, p: &Progress, x: i32, y: i32) -> Option<String> {
        let w = p.world_map(sw)?;
        let theme = w.theme.min(7);
        Some(match sw::spot(w, x, y) {
            Spot::Road => return None,
            Spot::Start => "MyWorld_Point_start".into(),
            Spot::Course { goal: true, order } => {
                // (a course is set: the run has one for the goal's place in the order)
                let water = w.cell(x, y).is_some_and(|c| c.flag & 0xf == 1);
                goal_model(
                    p.world + 1 >= sw.world_count(),
                    water,
                    sw.course(p.world, order).is_some(),
                )
                .into()
            }
            Spot::Course { .. } => point_model(w.point_at(x, y).map_or(0, |p| p.icon), theme),
            Spot::Pipe(_) => "MyWorld_Point_dokan".into(),
            Spot::ToadHouse(_) => "MyWorld_Point_kinokohouse_ingame".into(),
        })
    }

    /// The player's form of a one-off animation `base` on its route (`swim_tocleared` on water,
    /// `fly_landing` in the sky…), else `base`.
    pub fn player_anim(
        &self,
        character: usize,
        theme: u8,
        here: Option<&Cell>,
        base: &str,
    ) -> String {
        let model = PLAYERS[character.min(3)];
        let walk = self
            .anim_name("WM_PlayerAnime", "route", theme, Self::route(here))
            .unwrap_or_else(|| "walk".into());
        let formed = format!("{walk}_{base}");
        if walk != "walk" && walk != "climb" && self.has_anim(model, &formed) {
            formed
        } else {
            base.to_string()
        }
    }

    /// Every quad of the map of the progress's world.
    pub fn draw(&self, ui: &Ui, sw: &SuperWorld, p: &Progress, view: &MapView) -> Vec<Quad> {
        let Some(w) = p.world_map(sw) else {
            return Vec::new();
        };
        let theme = w.theme.min(7);
        let skin = SKINS[theme as usize];
        let t = view.tick as f32;
        let mut out = Vec::new();
        // the backdrop: the theme's DV model over the map's middle
        let dv = format!("MyWorld_DV_{skin}");
        let dv_anim = format!("MyWorld_DV_{skin}");
        self.emit(
            ui,
            &Instance {
                model: &dv,
                anim: self.has_anim(&dv, &dv_anim).then_some(dv_anim.as_str()),
                time: t,
                at: MAP_CENTER,
                flip: false,
                turn: false,
                layer: 0,
                arms: None,
                pattern_frame: None,
            },
            &mut out,
        );
        let sub = self.sub_theme(theme);
        self.emit_field(ui, theme, &field(w, sub), &mut out);
        self.emit_cliffs(ui, theme, &cliffs(w, sub), &mut out);
        let cleared = p.clears.get(p.world).copied().unwrap_or(0);
        let (reach, fresh) = reach_at(w, cleared, view);
        let reveal_age = view.reveal.map_or(i32::MAX, |(_, at)| view.tick - at);
        let appearing = |x: i32, y: i32| in_mask(fresh, x, y) && reveal_age < REVEAL_FRAMES;
        // the road pieces and decorations: an arm shows between two reached cells, a centre
        // piece on a reached cell (spec §7.4)
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                let Some(c) = w.cell(x, y) else { continue };
                let Some(model) = cell_model(c, skin) else {
                    continue;
                };
                let at = cell_world(x as f32, y as f32);
                if c.kind == 1 {
                    self.emit(
                        ui,
                        &Instance {
                            model: &model,
                            anim: Some("wait"),
                            time: t,
                            at,
                            flip: false,
                            turn: false,
                            layer: 3,
                            arms: None,
                            pattern_frame: None,
                        },
                        &mut out,
                    );
                    continue;
                }
                let links = w.links(x, y);
                let turn = c.kind == road::LEDGE;
                let piece = |arms: u8, frame: f32| Instance {
                    model: &model,
                    anim: None,
                    time: 0.0,
                    at,
                    flip: false,
                    turn,
                    layer: 2,
                    arms: Some(arms),
                    pattern_frame: Some(frame),
                };
                for (bit, dx, dy) in [(1u8, 1, 0), (2, -1, 0), (4, 0, 1), (8, 0, -1)] {
                    if road_arms(links) & bit == 0 {
                        continue;
                    }
                    let shown = in_mask(reach, x, y) && in_mask(reach, x + dx, y + dy);
                    let new = appearing(x, y) || appearing(x + dx, y + dy);
                    if let Some(f) = road_frame(shown, new, false) {
                        let arms = if turn { ledge_shape(bit) } else { bit };
                        self.emit(ui, &piece(arms, f), &mut out);
                    }
                }
                let road_cell = sw::spot(w, x, y) == Spot::Road;
                if let Some(f) = road_frame(in_mask(reach, x, y), appearing(x, y), road_cell) {
                    self.emit(ui, &piece(ARM_CENTRE, f), &mut out);
                }
            }
        }
        // points, pipes, Toad Houses
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                let Some((model, state, started, frozen)) =
                    self.point_state(sw, p, view, (reach, fresh), x, y)
                else {
                    continue;
                };
                let anim =
                    state.and_then(|s| self.state_anim(&model, s, theme, point_sub(w, x, y)));
                self.emit(
                    ui,
                    &Instance {
                        model: &model,
                        anim: anim.as_deref(),
                        time: if frozen { 0.0 } else { t - started as f32 },
                        at: cell_world(x as f32, y as f32),
                        flip: false,
                        turn: false,
                        layer: 3,
                        arms: None,
                        pattern_frame: None,
                    },
                    &mut out,
                );
            }
        }
        if !view.hide_player {
            self.emit_player(ui, w, theme, view, &mut out);
        }
        out.sort_by_key(|d| d.key);
        // the camera lower: everything higher on the screen
        let dy = -view.camera_dy * SCALE;
        out.into_iter()
            .map(|d| {
                let mut q = d.quad;
                for c in &mut q.pos {
                    c[1] += dy;
                }
                q
            })
            .collect()
    }

    /// The point at (x, y): its model, state, the tick its animation started and whether it
    /// stands still; None where nothing is drawn.
    /// - A course point, pipe or Toad House not reached yet shows its Wait without moving
    ///   [obs 0x71014409b0: the state's animation runs at rate 0 unless the point is open,
    ///   +0x3fa]; reached by a reveal it pops (`Passable`) [obs 0x71014812b0: kinds 11 / 12 on
    ///   the points a reveal opens].
    /// - The goal is not shown until reached; a reveal that reaches it plays `AppearGoalPoint`
    ///   [obs 0x7101482e60 / 0x71014830c0 / 0x71014832d0, the reveal's last steps].
    /// - A course cleared on this visit plays ToCleared, then Cleared; a cleared goal turns into
    ///   the cannon when it fires (its prepare, then its launch).
    fn point_state(
        &self,
        sw: &SuperWorld,
        p: &Progress,
        view: &MapView,
        (reach, fresh): (u32, u32),
        x: i32,
        y: i32,
    ) -> Option<(String, Option<PointState>, i32, bool)> {
        let w = p.world_map(sw)?;
        let theme = w.theme.min(7);
        let sub = point_sub(w, x, y);
        let model = self.spot_model(sw, p, x, y)?;
        let reached = in_mask(reach, x, y);
        let reveal_at = view.reveal.map_or(0, |r| r.1);
        // newly reached: the pop (or the goal's appearance) while it lasts
        let m = model.clone();
        let fresh_state = |state: PointState| {
            let anim = self.state_anim(&m, state, theme, sub)?;
            (in_mask(fresh, x, y) && view.tick - reveal_at < self.anim_frames(&m, &anim))
                .then_some(state)
        };
        let open = |model: String| match fresh_state(PointState::Passable) {
            Some(s) => (model, Some(s), reveal_at, false),
            None => (model, Some(PointState::Wait), 0, !reached),
        };
        Some(match sw::spot(w, x, y) {
            Spot::Road => return None,
            Spot::Start => (model, None, 0, false),
            Spot::Course { order, goal } => {
                let done = p.cleared(p.world, order);
                let just = view.just_cleared.iter().find(|j| j.0 == order).map(|j| j.1);
                if goal
                    && done
                    && let Some(c) = view.cannon
                {
                    let prep = self
                        .state_anim(CANNON, PointState::CannonPrepareLaunch, theme, sub)
                        .map_or(0, |a| self.anim_frames(CANNON, &a));
                    return Some(if view.tick - c < prep {
                        (
                            CANNON.into(),
                            Some(PointState::CannonPrepareLaunch),
                            c,
                            false,
                        )
                    } else {
                        (
                            CANNON.into(),
                            Some(PointState::CannonLaunch),
                            c + prep,
                            false,
                        )
                    });
                }
                match (done, just) {
                    (true, Some(j)) => (model, Some(PointState::ToCleared), j, false),
                    (true, None) => (model, Some(PointState::Cleared), 0, false),
                    _ if goal => match fresh_state(PointState::AppearGoalPoint) {
                        Some(s) => (model, Some(s), reveal_at, false),
                        None if reached => (model, Some(PointState::Wait), 0, false),
                        None => return None,
                    },
                    _ => open(model),
                }
            }
            Spot::Pipe(_) => open(model),
            Spot::ToadHouse(k) => {
                let used = p.toads.get(p.world).is_some_and(|t| t & (1 << k) != 0);
                match (used, view.houses_used.iter().find(|h| h.0 == k)) {
                    (true, Some(h)) => (model, Some(PointState::ToCleared), h.1, false),
                    (true, None) => (model, Some(PointState::Cleared), 0, false),
                    _ => open(model),
                }
            }
        })
    }

    /// The player: walking, standing, or a one-off animation of [`MapView::player_anim`].
    fn emit_player(&self, ui: &Ui, w: &World, theme: u8, view: &MapView, out: &mut Vec<Drawn>) {
        let model = PLAYERS[view.character.min(3)];
        let (px, py) = (view.player[0], view.player[1]);
        let here = w.cell(px.round() as i32, py.round() as i32);
        // up or down a ledge: the ladder [obs: WM_PlayerAnime's "ladder" route; inf: which
        // walks take it]
        let ledge = |x: f32, y: f32| {
            w.cell(x as i32, y as i32)
                .is_some_and(|c| c.kind == road::LEDGE)
        };
        let climbing = matches!(view.walking, Some((0, _)))
            && (ledge(px.floor(), py.floor()) || ledge(px.ceil(), py.ceil()));
        let route = if climbing {
            "ladder"
        } else {
            Self::route(here)
        };
        let walk = self
            .anim_name("WM_PlayerAnime", "route", theme, route)
            .unwrap_or_else(|| "walk".into());
        let (dir, flip) = match view.walking.unwrap_or(view.facing) {
            (0, 1) => ("_N", false),
            (0, -1) => ("_S", false),
            (-1, 0) => ("", true),
            (1, 0) => ("", false),
            _ => ("_S", false),
        };
        let one_off = view.player_anim.and_then(|(base, at)| {
            let name = self.player_anim(view.character, theme, here, base);
            (self.has_anim(model, &name) && view.tick - at < self.anim_frames(model, &name))
                .then_some((name, (view.tick - at) as f32))
        });
        let (name, clock) = if let Some(o) = one_off {
            o
        } else if view.walking.is_some() {
            (format!("{walk}{dir}"), view.tick as f32)
        } else {
            // standing: `wait`, or the route's own (`swim_wait`, `fly_wait`, …) [inf]
            let base = if walk == "walk" || walk == "climb" {
                "wait".to_string()
            } else {
                format!("{walk}_wait")
            };
            (format!("{base}{dir}"), view.since as f32)
        };
        let name = [
            name.clone(),
            name.trim_end_matches(dir).to_string(),
            "wait".into(),
        ]
        .into_iter()
        .find(|n| self.has_anim(model, n))
        .unwrap_or(name);
        let at = cell_world(px, py);
        let at = [at[0], at[1] + view.lift];
        self.emit(
            ui,
            &Instance {
                model,
                anim: Some(&name),
                time: clock,
                at,
                flip,
                turn: false,
                layer: 4,
                arms: None,
                pattern_frame: None,
            },
            out,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmk_formats::smm2::world::Point;
    use mmk_formats::smm2::world::PointKind;

    #[test]
    fn the_piece_table_covers_every_mask() {
        let t = piece_table();
        // no land around: the empty piece; land all round: the pond
        assert_eq!(t[0], 38);
        assert_eq!(t[255], 0);
        // land above only: the piece with its top side closed
        assert_eq!(t[bit_of("N") as usize], 35);
        // a corner alone only matters with both its sides open
        assert_eq!(t[bit_of("NW") as usize], 43);
        assert_eq!(t[(bit_of("NW") | bit_of("N")) as usize], 35);
        // spot checks against the game's table (main+0x23cfe48)
        assert_eq!(t[8], 37);
        assert_eq!(t[16], 39);
        assert_eq!(t[0x5a], 0);
        // every piece is used
        let mut used = [false; 47];
        for &p in &t {
            used[p as usize] = true;
        }
        assert!(used.iter().all(|&u| u));
    }

    fn cell(kind: u8, rotation: u8, flag: u8) -> Cell {
        Cell {
            rotation,
            kind,
            flag,
        }
    }

    #[test]
    fn the_field_follows_the_land() {
        let mut w = World::default();
        // one road cell at (0, 0), a water route east of it
        *w.cell_mut(0, 0) = cell(road::END, 2, 0);
        *w.cell_mut(1, 0) = cell(road::STRAIGHT, 2, 1);
        let f = field(&w, true);
        // grid (2, 1) is cell (0, 0): land
        assert_eq!(f.water[1][2], FULL_LAND);
        // the water route (1, 0): land to its west and the map's edge below
        let t = piece_table();
        let m = bit_of("W") | bit_of("SW") | bit_of("S") | bit_of("SE");
        assert_eq!(f.water[1][3], t[m as usize]);
        // far from everything: open water; out of the map: land beside it
        assert_eq!(f.water[3][6], 38);
        assert_eq!(f.water[0][0], 0);
        // the "nothing area" layer leaves the road cells out
        assert_eq!(f.main[1][2], NO_PIECE);
        assert_eq!(f.main[3][6], 38 + MAIN_SET);
        // a ledge's ladder makes the point above it stand on land
        let mut l = World::default();
        *l.cell_mut(3, 1) = cell(road::LEDGE, 2, 0);
        l.points[0] = Some(Point {
            kind: PointKind::Start,
            icon: 0,
            x: 3,
            y: 2,
            course: None,
            flags: 0,
        });
        // alone in the water a ledge and the point above it stand in the water; a road beside
        // it makes its cell land, one up-right of it the cell above
        let fl = field(&l, true);
        assert_ne!(fl.water[2][5], FULL_LAND);
        assert_ne!(fl.water[3][5], FULL_LAND);
        *l.cell_mut(2, 1) = cell(road::STRAIGHT, 2, 0);
        let fl = field(&l, true);
        assert_eq!(
            fl.water[2][5], FULL_LAND,
            "a road beside it: its cell is land"
        );
        assert_ne!(fl.water[3][5], FULL_LAND, "the cell above isn't");
        let flags = ledge_land(&l, true);
        assert_eq!(
            flags[1][3],
            Some(LedgeLand {
                upper: false,
                own: true
            })
        );
        *l.cell_mut(4, 2) = cell(road::END, 2, 0);
        assert_eq!(
            ledge_land(&l, true)[1][3],
            Some(LedgeLand {
                upper: true,
                own: true
            })
        );
        assert_eq!(field(&l, true).water[3][5], FULL_LAND);
        // in a "Main" theme the empty cells round it are land
        *l.cell_mut(2, 1) = Cell::default();
        *l.cell_mut(4, 2) = Cell::default();
        assert_eq!(
            ledge_land(&l, false)[1][3],
            Some(LedgeLand {
                upper: true,
                own: true
            })
        );
        // a "Main" theme: land everywhere but the water route
        let g = field(&w, false);
        assert_eq!(g.water[3][6], FULL_LAND);
        assert_eq!(g.water[1][3], f.water[1][3]);
    }

    #[test]
    fn icons_pick_their_point_models() {
        assert_eq!(point_model(0, 0), "MyWorld_Point_normal");
        assert_eq!(point_model(5, 3), "MyWorld_Point_hauntedhouse");
        assert_eq!(point_model(7, 0), "MyWorld_Point_airship");
        assert_eq!(point_model(9, 6), "MyWorld_Point_enemyB_magma");
        assert_eq!(point_model(9, 0), "MyWorld_Point_enemyB");
        assert_eq!(point_model(12, 7), "MyWorld_Point_enemyE_night");
        assert_eq!(point_model(14, 4), "MyWorld_Point_enemyG_snow");
        assert_eq!(point_model(14, 3), "MyWorld_Point_enemyG");
        assert_eq!(point_model(15, 0), "MyWorld_Point_enemyH");
        // the goal: the last world's end castle, the airship on water, else the castle
        assert_eq!(goal_model(false, false, true), "MyWorld_Point_castle");
        assert_eq!(goal_model(true, true, true), "MyWorld_Point_castle_end");
        assert_eq!(
            goal_model(false, true, true),
            "MyWorld_Point_airship_castle"
        );
        assert_eq!(
            goal_model(false, false, false),
            "MyWorld_Point_castle_notset"
        );
        assert_eq!((point_kind(6), point_kind(14)), ("Tower", "Sambo"));
    }

    #[test]
    fn cells_pick_their_road_models_and_arms() {
        assert_eq!(
            cell_model(&cell(road::STRAIGHT, 0, 0), "snow").as_deref(),
            Some("MyWorld_Road_snow")
        );
        assert_eq!(
            cell_model(&cell(road::STRAIGHT, 0, 1), "snow").as_deref(),
            Some("MyWorld_Road_snow_s")
        );
        assert_eq!(
            cell_model(&cell(road::CURVE, 1, 1), "plain").as_deref(),
            Some("MyWorld_Road_Curve_plain_s")
        );
        assert_eq!(
            cell_model(&cell(road::BRIDGE, 0, 0), "magma").as_deref(),
            Some("MyWorld_Road_Bridge_magma")
        );
        assert_eq!(
            cell_model(&cell(road::LEDGE, 2, 0), "night").as_deref(),
            Some("MyWorld_Road_2fA_night")
        );
        assert_eq!(
            cell_model(&cell(1, 0, 2), "woods").as_deref(),
            Some("MyWorld_Object_woodsC")
        );
        assert_eq!(cell_model(&cell(road::NONE, 0, 0), "plain"), None);
        // links → arms: row + 1 is up the screen
        assert_eq!(road_arms(EAST | SOUTH), 0b0101);
        assert_eq!(road_arms(WEST | NORTH), 0b1010);
        // a straight road's links from the cells around it
        let mut w = World::default();
        *w.cell_mut(1, 1) = cell(road::STRAIGHT, 0, 0);
        // (END rotation 0 opens to row − 1, rotation 1 to row + 1)
        *w.cell_mut(1, 2) = cell(road::END, 0, 0);
        *w.cell_mut(1, 0) = cell(road::END, 1, 0);
        w.points[0] = Some(Point {
            kind: PointKind::Start,
            icon: 0,
            x: 1,
            y: 0,
            course: None,
            flags: 0,
        });
        assert_eq!(road_arms(w.links(1, 1)), 0b1100);
        assert_eq!(shape_arm("road_N"), Some(4));
        // a ledge, turned: its long arm leads up, east and west swap, nothing leads down
        assert_eq!(ledge_shape(4), 8);
        assert_eq!((ledge_shape(1), ledge_shape(2)), (2, 1));
        assert_eq!(ledge_shape(8), 0);
        assert_eq!(shape_arm("bridge_C"), Some(ARM_CENTRE));
        assert_eq!(shape_arm("mt_point"), None);
    }

    #[test]
    fn world_to_layout() {
        // the camera's middle is the screen's; a cell is 48 × 1280 / 460.8 layout units
        assert_eq!(to_layout(VIEW_CENTER), [0.0, 0.0]);
        let a = to_layout(cell_world(0.0, 0.0));
        let b = to_layout(cell_world(1.0, 1.0));
        let cell = CELL * 1280.0 / VIEW_W;
        assert!((b[0] - a[0] - cell).abs() < 1e-3 && (b[1] - a[1] - cell).abs() < 1e-3);
        assert!((SCALE - 1280.0 / VIEW_W).abs() < 1e-4, "16:9");
        // row 0 is the bottom one; the map sits centred across, 14 units low on the screen,
        // with the field's ring round it in view
        assert!(a[1] < 0.0);
        let left = to_layout([0.0, 0.0]);
        let right = to_layout([GRID_W as f32 * CELL, GRID_H as f32 * CELL]);
        assert!((left[0] + right[0]).abs() < 1e-3);
        assert!((left[1] + 110.0 * SCALE).abs() < 1e-3 && right[1] < 360.0);
        assert!(to_layout([-62.4, 0.0])[0] + 640.0 < 1e-2);
        // the course-in shift eases in over 15 frames and back
        assert_eq!(camera_shift(true, 0), 0.0);
        assert_eq!(camera_shift(true, 15), -16.0);
        assert_eq!(camera_shift(false, 15), 0.0);
        assert!((camera_shift(false, 5) + 16.0 * 2.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn roads_show_between_reached_cells_and_appear_after_a_clear() {
        assert_eq!(road_frame(false, false, false), None);
        assert_eq!(road_frame(true, false, false), Some(1.0));
        assert_eq!(road_frame(true, true, false), Some(2.0));
        assert_eq!(road_frame(true, false, true), Some(3.0));
        assert_eq!(road_frame(true, true, true), Some(4.0));
        // the test world: the start (0, 0), a course at (1, 0), the road on to (3, 0)
        let src: Vec<sw::SwCourse> = (0..7)
            .map(|i| sw::SwCourse {
                id: format!("c{i}"),
                ..Default::default()
            })
            .collect();
        let world = SuperWorld::test_world(&src).unwrap();
        let w = &world.map.worlds[0];
        let before = sw::reachable(w, 0);
        assert!(in_mask(before, 1, 0) && !in_mask(before, 2, 0));
        // before the reveal's tick the map stays as it was; then the new cells appear
        let view = MapView {
            tick: 10,
            reveal: Some((before, 20)),
            ..Default::default()
        };
        let (reach, fresh) = reach_at(w, 1, &view);
        assert_eq!((reach, fresh), (before, 0));
        let view = MapView { tick: 20, ..view };
        let (reach, fresh) = reach_at(w, 1, &view);
        assert!(in_mask(reach, 2, 0) && in_mask(fresh, 2, 0) && !in_mask(fresh, 1, 0));
    }

    #[test]
    fn ledges_draw_their_cliffs_in_rows() {
        let mut w = World::default();
        for x in 2..5 {
            *w.cell_mut(x, 1) = cell(road::LEDGE, 2, 0);
        }
        *w.cell_mut(6, 2) = cell(road::LEDGE, 2, 0);
        let c = cliffs(&w, false);
        let at = |x: i32, y: i32| {
            c.iter()
                .find(|p| (p.x, p.y) == (x, y) && p.piece != FULL_LAND)
                .map(|p| p.piece)
        };
        // a row of three: left, middle, right faces, the plateau's top above them
        assert_eq!(
            (at(2, 1), at(3, 1), at(4, 1)),
            (
                Some(CLIFF_FACE + 1),
                Some(CLIFF_FACE + 2),
                Some(CLIFF_FACE + 3)
            )
        );
        assert_eq!(at(3, 2), Some(CLIFF_TOP + 2));
        // land under them in a "Main" theme
        assert!(c.iter().any(|p| (p.x, p.y, p.piece) == (3, 1, FULL_LAND)));
        assert!(c.iter().any(|p| (p.x, p.y, p.piece) == (3, 2, FULL_LAND)));
        // alone; with no land round it in a "Sub" theme the face meets the water
        assert_eq!(at(6, 2), Some(CLIFF_FACE));
        let s = cliffs(&w, true);
        assert!(
            s.iter()
                .any(|p| (p.x, p.y, p.piece) == (6, 2, CLIFF_FACE_WATER))
        );
        assert!(!s.iter().any(|p| p.piece == FULL_LAND));
        // a "B" ledge (flag 1) beside them takes row 7's pieces and doesn't join the row
        *w.cell_mut(5, 1) = cell(road::LEDGE, 2, 1);
        let c = cliffs(&w, false);
        let at = |x: i32, y: i32| {
            c.iter()
                .find(|p| (p.x, p.y) == (x, y) && p.piece != FULL_LAND)
                .map(|p| p.piece)
        };
        assert_eq!(at(5, 1), Some(CLIFF_FACE + CLIFF_ROW_B));
        assert_eq!(at(5, 2), Some(CLIFF_TOP + CLIFF_ROW_B));
        assert_eq!(at(4, 1), Some(CLIFF_FACE + 3));
    }
}
