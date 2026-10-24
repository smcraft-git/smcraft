//! Clear pipes (part 93, 3D World only): the pieces the course's clear-pipe records draw.
//!
//! Each record node becomes one BgUnit segment of group 24 (course-loading.md §1.4) [obs]; the
//! look is 3D World's `3W_Object_dokan_tomei` in every place (the only clear-pipe model in the
//! game: `dokan_tomei_unit_model` one tile of a straight run, `dokan_tomei_top_model` the same
//! with the mouth's rim, `dokan_tomei_curve00_model` a 2 × 2 bend) [obs RomFS]. The models
//! stand along y: the unit and the mouth are 32 px across (x ±16) and a tile long (y ±8), the
//! mouth opening toward +y; the bend's openings face −x and −y (x, y ±16) [obs: their
//! vertices]. Which piece each tile gets and its turn are ours from that geometry [inf]; the
//! walls' collision and the ride through the pipe are not built yet.

use mmk_formats::smm2::clear_pipe::{Dir, Piece, Shape};
use mmk_formats::smm2::course::Area;

/// The model a piece draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearPipeModel {
    /// `dokan_tomei_unit_model`: a tile of straight pipe.
    Unit,
    /// `dokan_tomei_top_model`: the tile at an open end.
    Mouth,
    /// `dokan_tomei_curve00_model`: a bend.
    Curve,
}

impl ClearPipeModel {
    /// The model's name in `3W_Object_dokan_tomei`.
    pub fn name(self) -> &'static str {
        match self {
            ClearPipeModel::Unit => "dokan_tomei_unit_model",
            ClearPipeModel::Mouth => "dokan_tomei_top_model",
            ClearPipeModel::Curve => "dokan_tomei_curve00_model",
        }
    }
}

/// The models' file.
pub const CLEAR_PIPE_MESH: &str = "3W_Object_dokan_tomei";

/// The sprite frame names a piece is drawn with: its model and quarter turns (`<model>.<turns>`;
/// the renderer turns the model, `ClearPipeDraw::from_frame`).
const FRAMES: [[&str; 4]; 3] = [
    [
        "dokan_tomei_unit_model.0",
        "dokan_tomei_unit_model.1",
        "dokan_tomei_unit_model.2",
        "dokan_tomei_unit_model.3",
    ],
    [
        "dokan_tomei_top_model.0",
        "dokan_tomei_top_model.1",
        "dokan_tomei_top_model.2",
        "dokan_tomei_top_model.3",
    ],
    [
        "dokan_tomei_curve00_model.0",
        "dokan_tomei_curve00_model.1",
        "dokan_tomei_curve00_model.2",
        "dokan_tomei_curve00_model.3",
    ],
];

/// One drawn piece: the model, its centre (world px) and its turn (quarter turns
/// counter-clockwise from the model's own axis, about z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearPipeDraw {
    pub model: ClearPipeModel,
    pub x: f32,
    pub y: f32,
    pub turns: u8,
}

impl ClearPipeDraw {
    /// The sprite frame name of this piece.
    pub fn frame(&self) -> &'static str {
        let m = match self.model {
            ClearPipeModel::Unit => 0,
            ClearPipeModel::Mouth => 1,
            ClearPipeModel::Curve => 2,
        };
        FRAMES[m][(self.turns & 3) as usize]
    }

    /// The model name and quarter turns of a sprite frame name (`frame`'s inverse).
    pub fn from_frame(frame: &str) -> Option<(&'static str, u8)> {
        let (model, turns) = frame.split_once('.')?;
        let model = [
            ClearPipeModel::Unit,
            ClearPipeModel::Mouth,
            ClearPipeModel::Curve,
        ]
        .into_iter()
        .find(|m| m.name() == model)?;
        Some((model.name(), turns.parse::<u8>().ok()? & 3))
    }
}

/// The bend's turn: curve00 opens to the left and down at 0, each quarter turn moves both
/// openings on by a side (down / right, right / up, up / left).
fn bend_turns(from: Dir, to: Dir) -> u8 {
    // the openings: back where the pipe came from, and on where it goes
    let sides = [from.opposite(), to];
    let has = |d: Dir| sides.contains(&d);
    match (has(Dir::Left), has(Dir::Down), has(Dir::Right)) {
        (true, true, _) => 0,
        (_, true, true) => 1,
        (false, false, true) => 2,
        _ => 3,
    }
}

/// The pieces of one clear pipe.
pub fn pieces(s: &Shape) -> Vec<ClearPipeDraw> {
    const T: f32 = 16.0;
    let mut out = Vec::new();
    let all = s.pieces();
    let last = all.len().saturating_sub(1);
    for (i, p) in all.iter().enumerate() {
        match *p {
            Piece::Run { dir, rect } => {
                let (x, y, w, h) = rect;
                let n = if dir.horizontal() { w } else { h };
                for k in 0..n {
                    // the k-th tile along the run, from where it starts
                    let along = match dir {
                        Dir::Right | Dir::Up => k,
                        Dir::Left | Dir::Down => n - 1 - k,
                    };
                    let (cx, cy) = if dir.horizontal() {
                        ((x + along) as f32 * T + T / 2.0, y as f32 * T + T)
                    } else {
                        (x as f32 * T + T, (y + along) as f32 * T + T / 2.0)
                    };
                    // the first tile of the first run and the last of the last are mouths,
                    // facing out of the pipe
                    let (model, face) = if i == 0 && k == 0 {
                        (ClearPipeModel::Mouth, dir.opposite())
                    } else if i == last && k == n - 1 {
                        (ClearPipeModel::Mouth, dir)
                    } else {
                        (ClearPipeModel::Unit, dir)
                    };
                    out.push(ClearPipeDraw {
                        model,
                        x: cx,
                        y: cy,
                        turns: face.quarter_turns(),
                    });
                }
            }
            Piece::Bend { from, to, at } => out.push(ClearPipeDraw {
                model: ClearPipeModel::Curve,
                x: at.0 as f32 * T + T,
                y: at.1 as f32 * T + T,
                turns: bend_turns(from, to),
            }),
        }
    }
    out
}

/// The pieces of every clear pipe of an area (records that make no shape are left out).
pub fn area_pieces(area: &Area) -> Vec<ClearPipeDraw> {
    area.clear_pipes
        .slots
        .values()
        .filter_map(Shape::decode)
        .flat_map(|s| pieces(&s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bends_turn_their_openings_to_the_runs() {
        // curve00 opens left and down: a pipe going right that turns down
        assert_eq!(bend_turns(Dir::Right, Dir::Down), 0);
        assert_eq!(bend_turns(Dir::Up, Dir::Left), 0);
        assert_eq!(bend_turns(Dir::Left, Dir::Down), 1);
        assert_eq!(bend_turns(Dir::Up, Dir::Right), 1);
        assert_eq!(bend_turns(Dir::Left, Dir::Up), 2);
        assert_eq!(bend_turns(Dir::Down, Dir::Right), 2);
        assert_eq!(bend_turns(Dir::Right, Dir::Up), 3);
        assert_eq!(bend_turns(Dir::Down, Dir::Left), 3);
    }

    #[test]
    fn a_pipe_is_mouth_units_bend_units_mouth() {
        let s = Shape {
            origin: (10, 6),
            runs: vec![(Dir::Right, 3), (Dir::Up, 2)],
        };
        let p = pieces(&s);
        let models: Vec<ClearPipeModel> = p.iter().map(|d| d.model).collect();
        use ClearPipeModel::*;
        assert_eq!(models, [Mouth, Unit, Unit, Curve, Unit, Mouth]);
        // the start's mouth faces left, at the run's first tile, centred across its two rows
        assert_eq!((p[0].x, p[0].y, p[0].turns), (168.0, 96.0, 1));
        // the bend after it, then the end's mouth facing up
        assert_eq!((p[3].x, p[3].y, p[3].turns), (224.0, 96.0, 3));
        assert_eq!((p[5].x, p[5].y, p[5].turns), (224.0, 136.0, 0));
        assert_eq!(p[3].frame(), "dokan_tomei_curve00_model.3");
        assert_eq!(
            ClearPipeDraw::from_frame(p[0].frame()),
            Some(("dokan_tomei_top_model", 1))
        );
    }
}
