//! SMM1 course → SMM2 course data (Phase 6, private research/specs/smm1-course-import.md §8):
//! the owner's SMM1 courses (`course_data.cdt` + `course_data_sub.cdt`, or the game's built-ins,
//! a Yaz0 SARC per area) converted once into SMM2 course data (0x5BFC0 bytes) the engine plays,
//! with a report of what could not be carried over.
//!
//! SMM2's course encoding is SMM1's extended, so most fields copy [obs: the SMM1 v208 course
//! reader 0x020828b8 and the built-ins]:
//! - the header: name (UTF-16, byte order only), style, timer, date; each area's theme,
//!   autoscroll and width — **in pixels in both games** (0x021258e0: `width >> 4` columns), so
//!   the area's right boundary is the width; SMM1's goal sits where SMM2's own rule puts it
//!   (goal ground x = right − 152 px in all 76 built-ins), so `goal_x = 5·W/8 − 95`, `goal_y` and
//!   `start_y` are the goal's and the start's ground heights (objects 26 and 37);
//! - objects: the same 1/10-px positions, flags and links, less SMM1's bit 0x800 (the editor's
//!   loader sets it on every object, 0x021f90b4); ground (object 7, one per tile) goes to SMM2's
//!   ground array, tracks (59) to its track array (the extended data is SMM2's end word
//!   `k1 << 16 | k0`, 0xFF → 0x104 "the neighbour writes it"; type `(flags >> 22) & 7`; the
//!   link one up), the goal (26, 27 and the bridge 49 before it), the start (37, 38) and the
//!   player placeholder (69) to the header;
//! - sound effects: an SMM1 effect (type 0–13, variation 0 / 1, `cDefault` / `cShake`) becomes
//!   the SMM2 effect with the same waves (§7), on its part (`sound_effect_id`), on Mario (the
//!   header's player effect, from object 69) or on its spot.
//!
//! SMM2's start ground is two columns wider (−3…6 against SMM1's −3…4): x stays as it is, SMM1
//! ground there is dropped and a course whose columns 5–6 under the start height were not all
//! ground (a pit, parts) is reported as "start widened". What SMM2 can't hold is reported:
//! effects without an SMM2 sound (samba / disco, the bird whistle, the telephone…), Mystery
//! Mushroom costumes (SMB1's style power-up slot: played as Super Mushrooms, which SMCraft's
//! `smb1_mushroom=mystery` extra turns back into Mystery Mushrooms), the Weird Mushroom, the
//! maker's Mii and country, the draw layer, header 0x16 / 0x73.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mmk_formats::archive::{self, Sarc};
use mmk_formats::record::Record;
use mmk_formats::smm1;
use mmk_formats::smm2;
use mmk_formats::smm2::course::{CourseObject, GroundTile, SoundEffect, Track};
use serde_json::{Value, json};

/// SMM1's objects with a header field or an array of their own in SMM2.
const GROUND: i8 = 7;
const GOAL_GROUND: i8 = 26;
const GOAL_POLE: i8 = 27;
const START_GROUND: i8 = 37;
const START_SIGN: i8 = 38;
/// The castle bridge ahead of the goal: SMM1 stores one (14 × 1, ending at the goal ground)
/// in every course, SMM2 none — it lays its own goal out [obs data: all 76 built-ins; none in
/// the SMM2 sample].
const GOAL_BRIDGE: i8 = 49;
const TRACK: i8 = 59;
const PLAYER: i8 = 69;
/// The style power-up slot (Mystery Mushroom in SMB1) and the Super Mushroom.
const STYLE_POWER_UP: i8 = 44;
const SUPER_MUSHROOM: i16 = 20;
/// The Fire Flower (its 0x4 form is SMM2's Superball Flower in SMB1).
const FIRE_FLOWER: i8 = 34;
/// Parts whose 0x4 bit SMM2 never sets (it would read as their other look or form): mushroom
/// platforms, semisolids, conveyors, the Rocky Wrench (SMM2 has no other form of it) [inf].
const NO_ALT_BIT: [i8; 4] = [14, 16, 53, 58];
/// SMM1-only ids: unused (the vine block), the Mystery and Big Mushrooms per the public notes,
/// costume Mario — none in the built-ins.
const DROPPED: [i8; 4] = [32, 50, 51, 65];

/// SMM1's editor bit on every object.
const EDITOR_BIT: u32 = 0x800;

/// An unused sub area's right boundary (84 tiles, SMM2's) [obs data].
const UNUSED_SUB_WIDTH: i32 = 1344;
/// Both games' areas are 27 tiles tall.
const BOUNDARY_TOP: i32 = 0x1B0;

/// The SMM2 effect of an SMM1 effect (type, variation), by their waves (§7): None where SMM2
/// has none.
pub fn effect_id(kind: u8, variation: u8) -> Option<u8> {
    let id = match (kind, variation) {
        (0, 0) => 0,
        (0, 1) => 11,
        (1, 0) => 1,
        (1, 1) => 12,
        (2, 0) => 2,
        (2, 1) => 13,
        (3, 0) => 10,
        (3, 1) => 20,
        (4, 0) => 3,
        (4, 1) => 14,
        (5, 0) => 4,
        (5, 1) => 15,
        (6, 0) => 5,
        (6, 1) => 16,
        (8, 0) => 9,
        (8, 1) => 19,
        (9, 0) => 6,
        (10, 0) => 7,
        (10, 1) => 17,
        (11, 0) => 8,
        (11, 1) => 18,
        // the telephone's shake is SMM2's unoffered Drunkard (22): kept only if offered
        (12, 1) => 22,
        _ => return None,
    };
    smm2::sound_effects::valid(id).then_some(id)
}

/// What a conversion could not carry over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Losses {
    /// Effects without an SMM2 sound: (area, type, variation, x, y).
    pub effects: Vec<(usize, u8, u8, u8, u8)>,
    /// Columns 5–6 under the start were not all ground (SMM2's start ground covers them).
    pub start_widened: bool,
    /// Parts standing in SMM2's wider start ground (they stay where they were).
    pub in_start_ground: usize,
    /// Mystery Mushroom costumes (SMB1's style power-up slot), played as Super Mushrooms.
    pub costumes: usize,
    /// Weird Mushrooms (SMB1's Super Mushroom with 0x4), played as Super Mushrooms.
    pub weird: usize,
    /// SMM1-only ids dropped (32, 50, 51, 65).
    pub dropped_ids: BTreeMap<i8, usize>,
    /// Parts with SMM1's 0x40000 bit where SMM2 never has it (kept; informational).
    pub flag_40000: usize,
    /// Ground below the area's bottom (y < 0), dropped.
    pub ground_below: usize,
    /// Objects beyond SMM2's capacity, dropped.
    pub over_capacity: usize,
}

impl Losses {
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
            && !self.start_widened
            && self.in_start_ground == 0
            && self.costumes == 0
            && self.weird == 0
            && self.dropped_ids.is_empty()
            && self.ground_below == 0
            && self.over_capacity == 0
    }

    pub fn to_json(&self) -> Value {
        json!({
            "effects": self.effects.iter().map(|&(a, t, v, x, y)| json!({
                "area": a, "type": t, "variation": v, "x": x, "y": y,
            })).collect::<Vec<_>>(),
            "start_widened": self.start_widened,
            "in_start_ground": self.in_start_ground,
            "costumes": self.costumes,
            "weird_mushrooms": self.weird,
            "dropped_ids": self.dropped_ids.iter().map(|(k, v)| (k.to_string(), json!(v)))
                .collect::<serde_json::Map<_, _>>(),
            "flag_40000": self.flag_40000,
            "ground_below": self.ground_below,
            "over_capacity": self.over_capacity,
            // always: SMM2 course data has no place for them
            "metadata": ["maker Mii and country", "clear check / upload state",
                "draw layer (z)", "header 0x16, 0x73"],
        })
    }
}

/// A converted course.
#[derive(Clone, Debug)]
pub struct Converted {
    pub course: smm2::Course,
    pub name: String,
    pub losses: Losses,
}

fn bad(msg: impl Into<String>) -> String {
    msg.into()
}

/// The column of a 1/10-px x (tile parts sit on their bottom-left tile's centre).
fn tile(v: i64) -> i64 {
    (v - 80).div_euclid(160)
}

/// Convert an SMM1 course (its main and sub area files).
pub fn convert(main: &smm1::Course, sub: Option<&smm1::Course>) -> Result<Converted, String> {
    let h = &main.header;
    let style = h.game_style.as_str_lossy();
    if !matches!(style.as_str(), "M1" | "M3" | "MW" | "WU") {
        return Err(bad(format!("unknown style {style:?}")));
    }
    let mut c = smm2::Course::blank(&style);
    let name = h.name.to_string_lossy();
    c.header.name.set(&name);
    c.header.timer = h.timer as i16;
    c.header.year = h.year as i16;
    c.header.month = h.month as i8;
    c.header.day = h.day as i8;
    c.header.hour = h.hour as i8;
    c.header.minute = h.minute as i8;
    let mut losses = Losses::default();
    let height = |cdt: &smm1::Course, kind: i8| {
        cdt.objects
            .iter()
            .find(|o| o.r#type == kind)
            .map(|o| o.height.max(1) as u8)
    };
    let start_y = height(main, START_GROUND).unwrap_or(1);
    let goal_y = height(main, GOAL_GROUND).unwrap_or(1);
    let width = h.width as i32;
    c.header.start_y = start_y;
    c.header.goal_y = goal_y;
    c.header.goal_x = (5 * width / 8 - 95) as i16;
    let sub_used = sub.is_some_and(|s| !s.objects.is_empty());
    // bit 0 always; bit 6: the sub area is used [obs data: the SMM2 sample]
    c.header.management_flags = 1 | if sub_used { 0x40 } else { 0 };
    for (i, cdt) in [Some(main), sub].into_iter().enumerate() {
        let area = if i == 0 { &mut c.main } else { &mut c.sub };
        let Some(cdt) = cdt else {
            area.header.boundary_right = UNUSED_SUB_WIDTH;
            continue;
        };
        area.header.theme = cdt.header.theme;
        area.header.autoscroll_type = cdt.header.autoscroll;
        area.header.boundary_top = BOUNDARY_TOP;
        area.header.boundary_right = if i == 1 && !sub_used {
            UNUSED_SUB_WIDTH
        } else {
            cdt.header.width as i32
        };
        let player = convert_area(cdt, area, i, &style, start_y, &mut losses)?;
        if i == 0 {
            c.header.player_sound_effect = player.unwrap_or(smm2::course::NO_SOUND_EFFECT);
        }
    }
    Ok(Converted {
        course: c,
        name,
        losses,
    })
}

/// One area's objects, ground, tracks and effects; the effect on Mario (object 69), if any.
fn convert_area(
    cdt: &smm1::Course,
    area: &mut smm2::course::Area,
    index: usize,
    style: &str,
    start_y: u8,
    losses: &mut Losses,
) -> Result<Option<u8>, String> {
    let effects: Vec<Option<(u8, u8, u8, u8)>> = cdt
        .sound_effects
        .iter()
        .map(|e| {
            let raw = e.to_bytes().ok()?;
            // empty: x and y both 0xFF (0x02769f34); type < 14, variation ≤ 1
            if raw[3] == 0xFF && raw[4] == 0xFF || raw[0] >= 14 || raw[2] > 1 {
                return None;
            }
            Some((raw[0], raw[2], raw[3], raw[4]))
        })
        .collect();
    let mut on_parts = BTreeSet::new();
    let mut effect_of = |slot: i16, losses: &mut Losses| -> i16 {
        let Some(Some((t, v, x, y))) = usize::try_from(slot).ok().and_then(|s| effects.get(s))
        else {
            return -1;
        };
        on_parts.insert(slot as usize);
        match effect_id(*t, *v) {
            Some(id) => id as i16,
            None => {
                losses.effects.push((index, *t, *v, *x, *y));
                -1
            }
        }
    };
    let mut player = None;
    let mut start_cells = BTreeSet::new();
    for o in cdt.objects.iter() {
        let (col, row) = (tile(o.x as i64), tile(o.y as i64));
        let under_start = index == 0 && (5..=6).contains(&col) && row < start_y as i64;
        match o.r#type {
            GROUND => {
                if under_start {
                    start_cells.insert((col, row));
                    continue;
                }
                if row < 0 || col < 0 || row > 255 || col > 255 {
                    losses.ground_below += 1;
                    continue;
                }
                if area.ground.items.len() >= 4000 {
                    losses.over_capacity += 1;
                    continue;
                }
                let mut g = GroundTile::from_slice(&[0u8; 4], 0);
                (g.x, g.y) = (col as u8, row as u8);
                area.ground.items.push(g);
            }
            TRACK => {
                let mut t = Track::from_slice(&[0u8; 0x0C], 0);
                t.flags = u8::from(o.flags & 0x80 != 0);
                // the template's bottom-left tile; −1 wraps to 255 as in SMM2
                t.x = (o.x as i64 - 80).div_euclid(160) as u8;
                t.y = (o.y as i64 - 80).div_euclid(160) as u8;
                t.r#type = ((o.flags >> 22) & 7) as u8;
                t.link_id = (o.link_id as i32 + 1) as u16;
                let end = |v: u32| if v == 0xFF { 0x104 } else { v as u16 };
                t.k0 = end(o.extended_data & 0xFFFF);
                t.k1 = end(o.extended_data >> 16);
                area.tracks.items.push(t);
            }
            GOAL_GROUND | GOAL_POLE | GOAL_BRIDGE | START_GROUND | START_SIGN => {}
            PLAYER => {
                let e = effect_of(o.effect_index, losses);
                player = u8::try_from(e).ok();
            }
            k if DROPPED.contains(&k) => {
                *losses.dropped_ids.entry(k).or_default() += 1;
            }
            _ => {
                if under_start {
                    losses.in_start_ground += 1;
                }
                if area.objects.items.len() >= 2600 {
                    losses.over_capacity += 1;
                    continue;
                }
                let mut n = CourseObject::from_slice(&[0u8; 0x20], 0);
                n.x = o.x as i32;
                n.y = o.y as i32;
                n.width = o.width as u8;
                n.height = o.height as u8;
                n.flags = o.flags & !EDITOR_BIT;
                n.child_flags = o.child_flags & !EDITOR_BIT;
                n.extended_data = o.extended_data;
                n.id = o.r#type as i16;
                n.child_id = o.child_type as i16;
                n.link_id = o.link_id;
                n.sound_effect_id = effect_of(o.effect_index, losses);
                fix_part(&mut n, style, losses);
                area.objects.items.push(n);
            }
        }
    }
    // SMM2's start ground covers columns 5–6 under the start: were they all ground?
    if index == 0 {
        let full = (5..=6).all(|c| (0..start_y as i64).all(|r| start_cells.contains(&(c, r))));
        losses.start_widened = !full || losses.in_start_ground > 0;
    }
    // the effects no part (nor Mario) holds stand on their spots
    for (slot, e) in effects.iter().enumerate() {
        let Some((t, v, x, y)) = *e else { continue };
        if on_parts.contains(&slot) {
            continue;
        }
        match effect_id(t, v) {
            Some(id) => {
                let mut r = SoundEffect::from_slice(&[0u8; 4], 0);
                (r.id, r.x, r.y) = (id, x, y);
                area.sound_effects.items.push(r);
            }
            None => losses.effects.push((index, t, v, x, y)),
        }
    }
    Ok(player)
}

/// The parts whose SMM1 encoding means something else in SMM2 (§4.1, §5).
fn fix_part(n: &mut CourseObject, style: &str, losses: &mut Losses) {
    let id = n.id as i8;
    if NO_ALT_BIT.contains(&id) {
        n.flags &= !0x4;
    }
    if n.child_id == STYLE_POWER_UP as i16 {
        // SMM1's children 44 all carry 0x4, SMM2's none
        n.child_flags &= !0x4;
    }
    if style == "M1" {
        // the SMB1 style power-up slot is the Mystery Mushroom; the Weird Mushroom is a Super
        // Mushroom with 0x4 (SMM2's Master Sword there); 0x4 on a Fire Flower SMM2's Superball
        for (kind, flags) in [
            (&mut n.id, &mut n.flags),
            (&mut n.child_id, &mut n.child_flags),
        ] {
            if *kind == STYLE_POWER_UP as i16 {
                *kind = SUPER_MUSHROOM;
                *flags &= !0x4;
                losses.costumes += 1;
            } else if *kind == SUPER_MUSHROOM && *flags & 0x4 != 0 {
                *flags &= !0x4;
                losses.weird += 1;
            } else if *kind == FIRE_FLOWER as i16 {
                *flags &= !0x4;
            }
        }
    }
    // 0x40000 on parts outside those whose bits 18–19 mean something in SMM2
    // (pipes, doors, semisolids, mushroom platforms, conveyors)
    const USES_ATTR: [i16; 5] = [9, 55, 16, 14, 53];
    if n.flags & 0x40000 != 0 && !USES_ATTR.contains(&n.id) {
        losses.flag_40000 += 1;
    }
}

/// Read an SMM1 area file: a `.cdt`, or a built-in's Yaz0 SARC holding one.
pub fn read_area(path: &Path) -> Result<smm1::Course, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw = if data.len() == smm1::course::CDT_SIZE {
        data
    } else {
        let unwrapped = archive::unwrap(data).map_err(|e| format!("{}: {e}", path.display()))?;
        let sarc = Sarc::parse(unwrapped).map_err(|e| format!("{}: {e}", path.display()))?;
        let (_, cdt) = sarc
            .files()
            .find(|(n, _)| n.ends_with(".cdt"))
            .ok_or_else(|| format!("{}: no .cdt inside", path.display()))?;
        cdt.to_vec()
    };
    smm1::Course::parse(&raw).map_err(|e| format!("{}: {e}", path.display()))
}

/// The main and sub area files of `input`: a course folder (`course_data.cdt`,
/// `course_data_sub.cdt`), a main `.cdt` / built-in `.szs` (its `_sub` sibling beside it).
pub fn area_files(input: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
    if input.is_dir() {
        let main = input.join("course_data.cdt");
        if !main.is_file() {
            return Err(format!("{}: no course_data.cdt", input.display()));
        }
        let sub = input.join("course_data_sub.cdt");
        return Ok((main, sub.is_file().then_some(sub)));
    }
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("{}: no file name", input.display()))?;
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("");
    let sub = input.with_file_name(format!("{stem}_sub.{ext}"));
    Ok((input.to_path_buf(), sub.is_file().then_some(sub)))
}

/// Convert the course at `input` (see [`area_files`]).
pub fn convert_path(input: &Path) -> Result<Converted, String> {
    let (main, sub) = area_files(input)?;
    let m = read_area(&main)?;
    let s = sub.as_deref().map(read_area).transpose()?;
    convert(&m, s.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(kind: i8, col: i64, row: i64) -> smm1::course::CourseObject {
        let mut o = smm1::course::CourseObject::from_slice(&[0u8; 0x20], 0);
        o.x = (160 * col + 80) as u32;
        o.y = (160 * row + 80) as i16;
        o.width = 1;
        o.height = 1;
        o.flags = 0x0600_0840;
        o.child_flags = 0x800;
        o.r#type = kind;
        o.child_type = -1;
        o.link_id = -1;
        o.effect_index = -1;
        o
    }

    fn blank_cdt(style: &str, width: u32) -> smm1::Course {
        let mut c = smm1::Course::parse(&vec![0u8; smm1::course::CDT_SIZE]).unwrap();
        c.header.game_style.set(style);
        c.header.width = width;
        c.header.timer = 300;
        for e in c.sound_effects.items.iter_mut() {
            *e = smm1::course::SoundEffect::from_slice(&smm1::course::EMPTY_SOUND, 0);
        }
        c
    }

    #[test]
    fn the_sound_effects_map_by_their_waves() {
        assert_eq!(effect_id(0, 0), Some(0));
        assert_eq!(effect_id(0, 1), Some(11));
        assert_eq!(effect_id(3, 0), Some(10));
        assert_eq!(effect_id(8, 1), Some(19));
        assert_eq!(effect_id(10, 1), Some(17));
        assert_eq!(effect_id(7, 0), None, "samba");
        assert_eq!(effect_id(9, 1), None, "bird whistle");
        assert_eq!(effect_id(12, 0), None, "telephone");
    }

    /// A small course: the goal from the width, start and goal heights, ground and its start
    /// columns, a part's flags and effect, an effect on a spot, a track piece, Mario's effect.
    #[test]
    fn a_course_converts() {
        let mut m = blank_cdt("M1", 1280);
        let mut start = object(START_GROUND, 0, 0);
        start.height = 3;
        start.width = 8;
        let mut goal = object(GOAL_GROUND, 70, 0);
        goal.height = 2;
        m.objects
            .items
            .extend([start, goal, object(START_SIGN, 2, 3)]);
        // the start ground's columns 5–6 under the start, all ground: dropped, not a loss
        for c in 5..=6 {
            for r in 0..3 {
                m.objects.items.push(object(GROUND, c, r));
            }
        }
        m.objects.items.push(object(GROUND, 9, 0));
        let mut goomba = object(0, 10, 1);
        goomba.effect_index = 0;
        goomba.flags |= 0x2;
        m.objects.items.push(goomba);
        let mut piece = object(TRACK, 20, 4);
        piece.width = 3;
        piece.height = 3;
        piece.flags = 0x0640_0840; // type 1 (vertical)
        piece.extended_data = 0x0090_00FF;
        piece.link_id = 3;
        m.objects.items.push(piece);
        let mut mario = object(PLAYER, 2, 3);
        mario.effect_index = 2;
        m.objects.items.push(mario);
        let mut fx = |slot: usize, t, v, x, y| {
            let mut b = [0u8; 8];
            (b[0], b[1], b[2], b[3], b[4]) = (t, 0xFF, v, x, y);
            m.sound_effects.items[slot] = smm1::course::SoundEffect::from_slice(&b, 0);
        };
        fx(0, 2, 0, 10, 1);
        fx(1, 6, 1, 30, 5);
        fx(2, 0, 1, 2, 3);
        fx(3, 7, 0, 40, 2);
        let out = convert(&m, None).unwrap();
        let c = &out.course;
        assert_eq!(c.header.game_style.as_str_lossy(), "M1");
        assert_eq!(c.main.header.boundary_right, 1280);
        assert_eq!(c.header.goal_x, 5 * 1280 / 8 - 95);
        assert_eq!((c.header.start_y, c.header.goal_y), (3, 2));
        assert_eq!(c.sub.header.boundary_right, UNUSED_SUB_WIDTH);
        assert_eq!(c.main.ground.len(), 1, "only the ground past the start");
        let g = &c.main.objects.items[0];
        assert_eq!((g.id, g.flags), (0, 0x0600_0042));
        assert_eq!(g.sound_effect_id, 2, "Kick");
        let t = &c.main.tracks.items[0];
        assert_eq!((t.x, t.y, t.r#type, t.link_id), (20, 4, 1, 4));
        assert_eq!((t.k0, t.k1), (0x104, 0x90));
        assert_eq!(c.header.player_sound_effect, 11, "Scream on Mario");
        let spots: Vec<(u8, u8, u8)> = c
            .main
            .sound_effects
            .iter()
            .map(|s| (s.id, s.x, s.y))
            .collect();
        assert_eq!(
            spots,
            [(16, 30, 5)],
            "Fireworks on its spot; the samba is lost"
        );
        assert_eq!(out.losses.effects, vec![(0, 7, 0, 40, 2)]);
        assert!(!out.losses.start_widened);
        // it writes and reads back as SMM2 course data
        let bytes = c.to_bytes().unwrap();
        assert_eq!(bytes.len(), smm2::COURSE_SIZE);
        let again = smm2::Course::parse(&bytes).unwrap();
        assert_eq!(again.main.header.object_count, 1);
        assert_eq!(again.main.header.track_count, 1);
    }

    #[test]
    fn smb1s_style_power_up_is_a_super_mushroom() {
        let mut m = blank_cdt("M1", 640);
        let mut block = object(4, 3, 4);
        block.child_type = STYLE_POWER_UP;
        block.child_flags = 0x0600_0844;
        m.objects.items.push(block);
        let mut semi = object(16, 8, 2);
        semi.flags |= 0x4;
        m.objects.items.push(semi);
        let out = convert(&m, None).unwrap();
        let b = &out.course.main.objects.items[0];
        assert_eq!((b.child_id, b.child_flags), (SUPER_MUSHROOM, 0x0600_0040));
        assert_eq!(out.losses.costumes, 1);
        assert_eq!(out.course.main.objects.items[1].flags & 0x4, 0);
        // the start columns had no ground: SMM2's start ground fills a pit there
        assert!(out.losses.start_widened);
    }

    /// The owner's SMM1 built-ins (skipped without the dump): all 76 convert and read back.
    #[test]
    fn owner_built_ins_convert() {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let dir =
            home.join("smm-work/extracted/smm1-disc/GM000500001018DC00000000000100/content/Course");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            eprintln!("SMM1 dump not present; skipping");
            return;
        };
        let mut n = 0;
        for e in rd.flatten() {
            let p = e.path();
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if !name.ends_with(".szs") || name.ends_with("_sub.szs") {
                continue;
            }
            let out = convert_path(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
            let bytes = out.course.to_bytes().unwrap();
            smm2::Course::parse(&bytes).unwrap();
            // the goal sits where SMM1 put it: SMM2's goal ground (right − 152) is SMM1's
            let w = out.course.main.header.boundary_right;
            assert_eq!(w % 16, 0, "{name}");
            n += 1;
        }
        assert_eq!(n, 76);
    }
}
