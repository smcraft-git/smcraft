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
/// The view: the whole map's height (4 cells; its width, 7 cells, nearly fills 16:9) over its
/// middle [inf], onto 1280 × 720.
pub const VIEW_H: f32 = 192.0;
pub const SCALE: f32 = 720.0 / VIEW_H;
/// The map's middle (world units).
pub const MAP_CENTER: [f32; 2] = [GRID_W as f32 * CELL / 2.0, GRID_H as f32 * CELL / 2.0];

/// The scene skins in theme order [obs: the field textures registered by theme at
/// 0x71017065xx, the BGM table 0x710294bb50, the point variants of 0x710143d170 (snow = 4,
/// magma = 6, night = 7)] — the same as `world::THEMES`.
pub const SKINS: [&str; 8] = world::THEMES;

/// The world centre of cell (x, y) (y = the world's row, 0 at the bottom).
pub fn cell_world(x: f32, y: f32) -> [f32; 2] {
    [x * CELL + CELL / 2.0, y * CELL + CELL / 2.0]
}

/// World → layout units (1280 × 720, origin centre, y up).
pub fn to_layout(p: [f32; 2]) -> [f32; 2] {
    [
        (p[0] - MAP_CENTER[0]) * SCALE,
        (p[1] - MAP_CENTER[1]) * SCALE,
    ]
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
    // the cell a ledge's ladder climbs to stands on land too [inf: the ledge actor reaches
    // into it, and the builder asks its flags +0xbd4 / +0xbd5 there]
    let ladder_top = |x: i32, y: i32| {
        cell(x, y - 1).is_some_and(|c| c.kind == road::LEDGE) && w.links(x, y - 1) & SOUTH != 0
    };
    let land = |x: i32, y: i32| cell(x, y).is_some_and(land_cell) || ladder_top(x, y);
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

/// The goal's model: the last world's castle, an airship (icon 1) or a castle [inf: the six
/// goal models of 0x7101442b30, `_notset` ones for the maker].
pub fn goal_model(icon: u8, last_world: bool) -> &'static str {
    match (icon, last_world) {
        (1, _) => "MyWorld_Point_airship_castle",
        (_, true) => "MyWorld_Point_castle_end",
        _ => "MyWorld_Point_castle",
    }
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
        road::LEDGE => format!(
            "MyWorld_Road_2f{}_{skin}",
            if c.flag == 1 { "B" } else { "A" }
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

/// The road state frame of a cell's centre piece (the road models' texture patterns, frames
/// 0–4: none, the road, the road not reached, and both again small): the large pad under a
/// point, pipe or Toad House, the small one along the road [inf].
pub fn centre_frame(reached: bool, spot: bool) -> f32 {
    match (spot, reached) {
        (true, true) => 1.0,
        (true, false) => 2.0,
        (false, true) => 3.0,
        (false, false) => 4.0,
    }
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
                    let (u0, v0) = ((piece % 16) as f32 / 16.0, (piece / 16) as f32 / 8.0);
                    let (u1, v1) = (u0 + 1.0 / 16.0, v0 + 1.0 / 8.0);
                    let h = CELL / 2.0;
                    let p = |dx: f32, dy: f32| to_layout([center[0] + dx, center[1] + dy]);
                    out.push(Drawn {
                        key: (1, layer as i32, 0, out.len()),
                        quad: Quad {
                            pos: [p(-h, h), p(h, h), p(-h, -h), p(h, -h)],
                            uv0: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]],
                            color: [[255; 4]; 4],
                            black: [0, 0, 0, 0],
                            white: [255; 4],
                            tex0: t.id,
                            tex1: mmk_ui::TEXTURE_NONE,
                            flags: quad_flags(),
                            ..Default::default()
                        },
                    });
                }
            }
        }
    }
}

// MARK: the scene of one frame

/// What the map scene shows besides the world: the player and its walk.
#[derive(Clone, Debug)]
pub struct MapView {
    /// The map's tick (frames).
    pub tick: i32,
    /// The player's cell position (fractional while walking).
    pub player: [f32; 2],
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
}

const PLAYERS: [&str; 4] = [
    "MyWorld_Player_MarioMdl",
    "MyWorld_Player_LuigiMdl",
    "MyWorld_Player_KinopioMdl",
    "MyWorld_Player_KinopicoMdl",
];

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

    /// Every quad of the map of the current world.
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
        self.emit_field(ui, theme, &field(w, self.sub_theme(theme)), &mut out);
        // the road pieces and decorations: arms where the cell links, the road shown where it
        // can be reached [inf]
        let cleared = p.clears.get(p.world).copied().unwrap_or(0);
        let reach = sw::reachable(w, cleared);
        let reached = |x: i32, y: i32| {
            (0..GRID_W as i32).contains(&x)
                && (0..GRID_H as i32).contains(&y)
                && reach & (1 << (y as usize * GRID_W + x as usize)) != 0
        };
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                let Some(c) = w.cell(x, y) else { continue };
                let Some(model) = cell_model(c, skin) else {
                    continue;
                };
                if c.kind == 1 {
                    self.emit(
                        ui,
                        &Instance {
                            model: &model,
                            anim: Some("wait"),
                            time: t,
                            at: cell_world(x as f32, y as f32),
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
                let open = reached(x, y);
                let turn = c.kind == road::LEDGE;
                // shown and hidden arms: the road's state frame 1 where both ends are reached
                for (bit, dx, dy) in [(1u8, 1, 0), (2, -1, 0), (4, 0, 1), (8, 0, -1)] {
                    if road_arms(links) & bit == 0 {
                        continue;
                    }
                    let lit = open && reached(x + dx, y + dy);
                    self.emit(
                        ui,
                        &Instance {
                            model: &model,
                            anim: None,
                            time: 0.0,
                            at: cell_world(x as f32, y as f32),
                            flip: false,
                            turn,
                            layer: 2,
                            arms: Some(if turn { ledge_shape(bit) } else { bit }),
                            pattern_frame: Some(if lit { 1.0 } else { 2.0 }),
                        },
                        &mut out,
                    );
                }
                self.emit(
                    ui,
                    &Instance {
                        model: &model,
                        anim: None,
                        time: 0.0,
                        at: cell_world(x as f32, y as f32),
                        flip: false,
                        turn,
                        layer: 2,
                        arms: Some(ARM_CENTRE),
                        pattern_frame: Some(centre_frame(open, sw::spot(w, x, y) != Spot::Road)),
                    },
                    &mut out,
                );
            }
        }
        // points, pipes, Toad Houses
        let last_world = p.world + 1 >= sw.world_count();
        for y in 0..GRID_H as i32 {
            for x in 0..GRID_W as i32 {
                let spot = sw::spot(w, x, y);
                let kind = if w.cell(x, y).is_some_and(water_route) {
                    "Sub"
                } else {
                    "Main"
                };
                let (model, state, started) = match spot {
                    Spot::Road => continue,
                    Spot::Start => ("MyWorld_Point_start".to_string(), None, 0),
                    Spot::Course { order, goal } => {
                        let pt = w.point_at(x, y).copied();
                        let icon = pt.map_or(0, |p| p.icon);
                        let model = if goal {
                            goal_model(icon, last_world).to_string()
                        } else {
                            point_model(icon, theme)
                        };
                        let done = cleared & (1 << order) != 0;
                        let just = view.just_cleared.iter().find(|j| j.0 == order);
                        let state = match (done, just) {
                            (true, Some(_)) => "ToCleared",
                            (true, None) => "Cleared",
                            _ => "Wait",
                        };
                        (model, Some(state), just.map_or(0, |j| j.1))
                    }
                    Spot::Pipe(_) => ("MyWorld_Point_dokan".to_string(), Some("Wait"), 0),
                    Spot::ToadHouse(k) => {
                        let used = p.toads.get(p.world).is_some_and(|t| t & (1 << k) != 0);
                        (
                            "MyWorld_Point_kinokohouse_ingame".to_string(),
                            Some(if used { "Cleared" } else { "Wait" }),
                            0,
                        )
                    }
                };
                let anim = state.and_then(|s| {
                    self.anim_name("WM_PointPartsAnime", "kind", theme, &format!("{s}_{kind}"))
                        .filter(|a| self.has_anim(&model, a))
                        .or_else(|| {
                            let plain = s.to_ascii_lowercase();
                            self.has_anim(&model, &plain).then_some(plain)
                        })
                });
                let anim = anim.or_else(|| self.has_anim(&model, "wait").then(|| "wait".into()));
                self.emit(
                    ui,
                    &Instance {
                        model: &model,
                        anim: anim.as_deref(),
                        time: t - started as f32,
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
        // the player
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
        let (name, clock) = if view.walking.is_some() {
            (format!("{walk}{dir}"), t)
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
            &mut out,
        );
        out.sort_by_key(|d| d.key);
        out.into_iter().map(|d| d.quad).collect()
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
        let fl = field(&l, true);
        assert_eq!((fl.water[2][5], fl.water[3][5]), (FULL_LAND, FULL_LAND));
        assert_ne!(fl.water[4][5], FULL_LAND);
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
        assert_eq!(goal_model(0, false), "MyWorld_Point_castle");
        assert_eq!(goal_model(0, true), "MyWorld_Point_castle_end");
        assert_eq!(goal_model(1, false), "MyWorld_Point_airship_castle");
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
        // the map's middle is the screen's; a cell is 180 layout units
        assert_eq!(to_layout(MAP_CENTER), [0.0, 0.0]);
        let a = to_layout(cell_world(0.0, 0.0));
        let b = to_layout(cell_world(1.0, 1.0));
        assert!((b[0] - a[0] - 180.0).abs() < 1e-3 && (b[1] - a[1] - 180.0).abs() < 1e-3);
        // row 0 is the bottom one
        assert!(a[1] < 0.0);
    }
}
