//! `mmk trace`: run a per-frame input script through the simulation and write the player's state
//! after every frame as CSV, for frame-by-frame comparison with traces recorded elsewhere (e.g. an
//! emulator). `mmk trace-courses`: the small physics test courses those scripts run on.
//!
//! Script format (one line per run of frames, `#` starts a comment):
//!
//! ```text
//! 20 -            # 20 frames, nothing held
//! 40 RIGHT RUN    # 40 frames of Right + Run
//! 1 RIGHT JUMP
//! ```
//!
//! Buttons: LEFT RIGHT UP DOWN JUMP RUN SPIN (pad names are accepted too: A/B = JUMP,
//! X/Y = RUN, ZL/ZR = SPIN).

use std::path::Path;

use mmk_editor::Editor;
use mmk_formats::archive::sarc;
use mmk_formats::smm2::crypto::{self, KeyTable};
use mmk_sim::input::{self, Input};
use mmk_sim::player::Form;

/// Parse a script into one input per frame.
pub fn parse_script(text: &str) -> Result<Vec<Input>, String> {
    let mut out = Vec::new();
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let n: usize = it
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("line {}: expected a frame count", ln + 1))?;
        let mut b = 0u32;
        for tok in it {
            b |= match tok.to_ascii_uppercase().as_str() {
                "-" => 0,
                "LEFT" => input::LEFT,
                "RIGHT" => input::RIGHT,
                "UP" => input::UP,
                "DOWN" => input::DOWN,
                "JUMP" | "A" | "B" => input::JUMP,
                "RUN" | "X" | "Y" => input::RUN,
                "SPIN" | "ZL" | "ZR" => input::SPIN,
                other => return Err(format!("line {}: unknown button {other}", ln + 1)),
            };
        }
        out.extend(std::iter::repeat_n(Input { buttons: b }, n));
    }
    Ok(out)
}

pub fn parse_form(s: &str) -> Result<Form, String> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "small" => Form::Small,
        "super" => Form::Super,
        "fire" => Form::Fire,
        "raccoon" => Form::Raccoon,
        "cape" => Form::Cape,
        "propeller" => Form::Propeller,
        "cat" => Form::Cat,
        "builder" => Form::Builder,
        "superball" => Form::Superball,
        _ => return Err(format!("unknown form {s}")),
    })
}

pub struct Start {
    pub pos: Option<(f32, f32)>,
    pub facing: Option<i32>,
    pub form: Option<Form>,
    /// Start speed (px/frame, signed), e.g. when the recorded run starts in motion.
    pub vx: Option<f32>,
    /// Start state of the course's first actor: x, y, vx, vy, direction (±1).
    pub actor: Option<[f32; 5]>,
}

/// Run the script; one CSV row per frame (the state after that frame's step).
#[cfg(test)]
pub fn run(course: &[u8], script: &[Input], start: &Start) -> Result<String, String> {
    run_all(course, script, start).map(|(csv, _)| csv)
}

/// `run`, plus every actor's state after each frame (a second CSV: frame, id, kind / mode, x,
/// y, vx, vy, dir), for traces that follow more objects than the first actor.
pub fn run_all(course: &[u8], script: &[Input], start: &Start) -> Result<(String, String), String> {
    let c = mmk_formats::smm2::Course::parse(course).map_err(|e| e.to_string())?;
    let mut w = mmk_sim::World::from_course(&c);
    if let Some((x, y)) = start.pos {
        w.player.x = x;
        w.player.y = y;
    }
    if let Some(f) = start.facing {
        w.player.facing = f;
    }
    if let Some(f) = start.form {
        w.player.form = f;
    }
    if let Some(v) = start.vx {
        w.player.vx = v;
    }
    if let (Some([x, y, vx, vy, dir]), Some(a)) = (start.actor, w.actors.first_mut()) {
        (a.x, a.y, a.vx, a.vy) = (x, y, vx, vy);
        a.dir = if dir < 0.0 { -1 } else { 1 };
    }
    let first_id = w.actors.first().map_or(u32::MAX, |a| a.id);
    w.reset_camera();
    let mut csv = String::from(
        "frame,x,y,vx,vy,facing,ground,pose,form,input,ax,ay,avx,avy,adir,akind,camx,camy\n",
    );
    let mut all = String::from("frame,id,kind,x,y,vx,vy,dir\n");
    for (i, inp) in script.iter().enumerate() {
        w.step(*inp);
        for a in &w.actors {
            all.push_str(&format!(
                "{i},{},{},{},{},{},{},{}\n",
                a.id,
                format!("{:?}/{:?}", a.kind, a.mode).replace([',', ' '], ""),
                a.x,
                a.y,
                a.vx,
                a.vy,
                a.dir
            ));
        }
        let p = &w.player;
        csv.push_str(&format!(
            "{i},{},{},{},{},{},{},{:?},{:?},{}",
            p.x, p.y, p.vx, p.vy, p.facing, p.on_ground as u8, p.pose, p.form, inp.buttons
        ));
        // the course's first actor (a test course's enemy), by its spawn id
        match w.actors.iter().find(|a| a.id == first_id) {
            Some(a) => csv.push_str(&format!(
                ",{},{},{},{},{},{}",
                a.x,
                a.y,
                a.vx,
                a.vy,
                a.dir,
                format!("{:?}/{:?}", a.kind, a.mode).replace([',', ' '], "")
            )),
            None => csv.push_str(",,,,,,"),
        }
        csv.push_str(&format!(",{},{}\n", w.camera_x, w.camera_y));
    }
    Ok((csv, all))
}

// ---------------------------------------------------------------------------------------------
// Physics test courses

pub const STYLES: [&str; 5] = ["M1", "M3", "MW", "WU", "3W"];
pub const KINDS: [&str; 25] = [
    "flat",
    "wall",
    "slope",
    "water",
    "items",
    "form",
    "enemy",
    "koopa",
    "shell",
    "hit",
    "shellbrk",
    "shellq",
    "blocks",
    "bigblk",
    "spring",
    "note",
    "donut",
    "lift",
    "pswitch",
    "bobomb",
    "slopejoin",
    "cpipe",
    "cpipeg",
    "mushstart",
    "cpipev",
];

/// Kinds made in SMB1 only (the terrain they test is the same in every style).
const M1_ONLY: [&str; 2] = ["slopejoin", "mushstart"];
/// Kinds made in 3D World only (clear pipes are a 3D World part).
const W3_ONLY: [&str; 3] = ["cpipe", "cpipeg", "cpipev"];

/// Where the items / form courses keep their power-up: the ? block's tile column and its row
/// above the floor (the capture walks there first; no test script gets that far).
pub const POWER_BLOCK: (i32, i32) = (140, 4);

/// All test course names: `<style>_<kind>`.
pub fn course_names() -> Vec<String> {
    let mut v = Vec::new();
    for s in STYLES {
        for k in KINDS {
            // (3D World has no Note Blocks)
            if s == "3W" && k == "note"
                || s != "M1" && M1_ONLY.contains(&k)
                || s != "3W" && W3_ONLY.contains(&k)
            {
                continue;
            }
            v.push(format!("{s}_{k}"));
        }
    }
    v
}

/// Build one test course. Every course starts on the start ground with a long flat floor to
/// the right (one tile high; two in 3D World, whose start ground can't be lower); the kind adds
/// the feature under test further along.
pub fn build_course(name: &str) -> Result<Vec<u8>, String> {
    let (style, kind) = name.split_once('_').ok_or("name is <style>_<kind>")?;
    if !STYLES.contains(&style) || !KINDS.contains(&kind) {
        return Err(format!("unknown test course {name}"));
    }
    let err = |x: mmk_editor::EditError| format!("{x:?}");
    // the items / form courses are longer: their power-up block is past every test's reach
    let len = if matches!(kind, "items" | "form" | "bigblk") {
        160
    } else {
        120
    };
    // floor height in tiles: the game rejects a 3D World course with start height 1
    // (slopejoin: the start height of the course it copies)
    let base: i32 = if style == "3W" || kind == "slopejoin" {
        2
    } else {
        1
    };
    let mut e = Editor::new(style);
    e.set_name("Physics");
    e.set_theme(if kind == "water" { 4 } else { 0 });
    e.set_timer(500);
    e.set_length(len);
    e.set_start_goal(base as u8, base as u8);
    // header fields a course made in the game has (the game rejects a course without them and
    // plays a blank one instead) [obs: the game's own title courses, checked in an emulator]
    {
        let h = &mut e.course.header;
        (h.year, h.month, h.day, h.hour, h.minute) = (2019, 4, 16, 16, 5);
        h.game_version = 1;
        h.management_flags = 1;
        h.clear_check_time = u32::MAX;
        h.creation_id = 1;
        // goal x in tenths of a tile: 9.5 tiles left of the right boundary (as in the game's
        // own courses, e.g. 182 tiles → 1725)
        h.goal_x = (len * 10 - 95) as i16;
    }
    e.course.main.header.boundary_type = 1;
    if style == "3W" {
        // as in the game's own 3D World courses: both liquid heights 1, boundary types 0 / 1
        for a in [&mut e.course.main.header, &mut e.course.sub.header] {
            a.liquid_start_height = 1;
            a.liquid_end_height = 1;
        }
        e.course.main.header.boundary_type = 0;
        e.course.sub.header.boundary_type = 1;
    }
    // the start ground covers x < 7 and the goal ground the last 11 columns; ground overlapping
    // either makes the game reject the course
    let floor = |e: &mut Editor, x0: i32, x1: i32| -> Result<(), String> {
        for x in x0.max(7)..=x1.min(len - 12) {
            for y in 0..base {
                e.paint_ground(x, y).map_err(err)?;
            }
        }
        Ok(())
    };
    // rows above the floor
    let up = |y: i32| y + base - 1;
    match kind {
        "flat" => {
            // a 3-wide pit far to the right (x 90..92) for running off a ledge
            floor(&mut e, 0, 89)?;
            floor(&mut e, 93, len - 1)?;
        }
        "water" => floor(&mut e, 0, len - 1)?,
        "enemy" => {
            // a Goomba shut in a one-tile pen (1-high walls at x 19 and 21: it turns in place
            // and waits for a stomp), then a row of Munchers at x 23..26 to land on for a death
            // (3D World has no Munchers: Piranha Plants)
            floor(&mut e, 0, len - 1)?;
            e.paint_ground(19, up(1)).map_err(err)?;
            e.paint_ground(21, up(1)).map_err(err)?;
            e.place(0, 20, up(1)).map_err(err)?;
            let hazard = if style == "3W" { 2 } else { 57 };
            for x in 23..=26 {
                e.place(hazard, x, up(1)).map_err(err)?;
            }
        }
        "koopa" => {
            // a red Koopa on a 1-wide, 2-high pillar at x 20 (it turns at both ledges, so it
            // paces in place: a stomp from the floor always finds it), and a 3-high wall at x 34
            // for a kicked shell to bounce off (it slides back and forth between the wall and the
            // pillar; the player is safe on the pillar's top)
            floor(&mut e, 0, len - 1)?;
            for y in 1..=2 {
                e.paint_ground(20, up(y)).map_err(err)?;
            }
            for y in 1..=3 {
                e.paint_ground(34, up(y)).map_err(err)?;
            }
            let (kw, kh) = mmk_editor::default_size(1);
            e.place_sized(1, 20, up(3), kw, kh, mmk_editor::DEFAULT_FLAGS | 0x4)
                .map_err(err)?;
        }
        "shell" => {
            // an empty Buzzy Beetle shell (part 28 with 0x4: a still shell from the start) at
            // x 20 on the floor, for side kicks, and the 3-high wall at x 34 it bounces off
            floor(&mut e, 0, len - 1)?;
            for y in 1..=3 {
                e.paint_ground(34, up(y)).map_err(err)?;
            }
            // (3D World has no Buzzy Beetle: an empty Spiny shell, part 25 with 0x4)
            let part = if style == "3W" { 25 } else { 28 };
            let (sw, sh) = mmk_editor::default_size(part);
            e.place_sized(part, 20, up(1), sw, sh, mmk_editor::DEFAULT_FLAGS | 0x4)
                .map_err(err)?;
        }
        "hit" | "shellbrk" | "shellq" => {
            // the still shell of `shell` at x 20; then
            // - hit: two Goombas at x 40 and 42, out of the spawn zone until the view comes near
            //   (a running kick, the shell runs through both: the kill chain), and a 3-high wall
            //   at x 56 for the shell to come back off;
            // - shellbrk: a Brick Block on the floor at x 30 (the shell runs into its side);
            // - shellq: a ? Block with a Super Mushroom on the floor at x 30
            floor(&mut e, 0, len - 1)?;
            let part = if style == "3W" { 25 } else { 28 };
            let (sw, sh) = mmk_editor::default_size(part);
            e.place_sized(part, 20, up(1), sw, sh, mmk_editor::DEFAULT_FLAGS | 0x4)
                .map_err(err)?;
            match kind {
                "hit" => {
                    e.place(0, 40, up(1)).map_err(err)?;
                    e.place(0, 42, up(1)).map_err(err)?;
                    for y in 1..=3 {
                        e.paint_ground(56, up(y)).map_err(err)?;
                    }
                }
                "shellbrk" => {
                    e.place(4, 30, up(1)).map_err(err)?;
                }
                _ => {
                    let i = e.place(5, 30, up(1)).map_err(err)?;
                    e.try_set_child(i, 20, mmk_editor::DEFAULT_FLAGS)
                        .map_err(err)?;
                }
            }
        }
        "blocks" | "bigblk" => {
            // blocks to hit from below, 4 tiles up (a 3-tile gap over the floor): a Brick Block
            // at x 12, a ? Block (a coin) at x 18, a ? Block with a Super Mushroom at x 24, a
            // Brick Block with ten coins at x 30; bigblk adds the items courses' power-up block
            // (the capture makes the player Super first)
            floor(&mut e, 0, len - 1)?;
            e.place(4, 12, up(4)).map_err(err)?;
            e.place(5, 18, up(4)).map_err(err)?;
            let i = e.place(5, 24, up(4)).map_err(err)?;
            e.try_set_child(i, 20, mmk_editor::DEFAULT_FLAGS)
                .map_err(err)?;
            let i = e.place(4, 30, up(4)).map_err(err)?;
            e.try_set_child(i, 8, mmk_editor::DEFAULT_FLAGS)
                .map_err(err)?;
            if kind == "bigblk" {
                let (bx, by) = (POWER_BLOCK.0, up(POWER_BLOCK.1));
                let i = e.place(5, bx, by).map_err(err)?;
                e.try_set_child(i, 20, mmk_editor::DEFAULT_FLAGS)
                    .map_err(err)?;
                for y in 1..=2 {
                    e.paint_ground(bx - 4, up(y)).map_err(err)?;
                    e.paint_ground(bx + 4, up(y)).map_err(err)?;
                }
            }
        }
        "spring" => {
            // a Trampoline on the floor at x 14
            floor(&mut e, 0, len - 1)?;
            e.place(10, 14, up(1)).map_err(err)?;
        }
        "note" => {
            // a Note Block on the floor at x 14 (a one-tile step) and one 3 tiles up at x 24
            // (to hit from below)
            floor(&mut e, 0, len - 1)?;
            e.place(23, 14, up(1)).map_err(err)?;
            e.place(23, 24, up(4)).map_err(err)?;
        }
        "donut" => {
            // a row of three Donut Blocks 2 tiles up at x 14..16 over the floor
            floor(&mut e, 0, len - 1)?;
            for x in 14..=16 {
                e.place(21, x, up(3)).map_err(err)?;
            }
        }
        "lift" => {
            // a 3-wide Lift 3 tiles up at x 16..18 (it paces ±48 px about its spawn)
            floor(&mut e, 0, len - 1)?;
            let (_, lh) = mmk_editor::default_size(11);
            e.place_sized(11, 16, up(4), 3, lh, mmk_editor::DEFAULT_FLAGS)
                .map_err(err)?;
        }
        "pswitch" => {
            // a P Switch on the floor at x 14 and a 3-high wall of Brick Blocks at x 20 (coins
            // while the switch runs)
            floor(&mut e, 0, len - 1)?;
            e.place(18, 14, up(1)).map_err(err)?;
            for y in 1..=3 {
                e.place(4, 20, up(y)).map_err(err)?;
            }
        }
        "bobomb" => {
            // a Bob-omb at x 40, out of the spawn zone until the view comes near
            floor(&mut e, 0, len - 1)?;
            e.place(15, 40, up(1)).map_err(err)?;
        }
        "items" | "form" => {
            // the power-up in a ? block 4 tiles up, far enough right that no test reaches it:
            // a Super Mushroom (items) or the style's own power-up (form; part 44: leaf,
            // feather, propeller, bell; a mushroom in SMB1). On the floor the game's items
            // move off or don't show at the title screen [obs: emulator]; out of a block they
            // come to the player. Walls 3 tiles either side keep a moving item close.
            floor(&mut e, 0, len - 1)?;
            let (bx, by) = (POWER_BLOCK.0, up(POWER_BLOCK.1));
            // NSMBU: the Propeller Mushroom is never collected at the title screen (on the
            // floor it stays put and the player's touch does nothing [obs: emulator, its actor
            // found in memory]), so WU_form has the Super Mushroom and the capture makes the
            // Super player a Propeller one (player form +0x4a8 = 3)
            let item = if kind == "items" || style == "WU" {
                20
            } else {
                44
            };
            if item == 44 && style == "3W" {
                // out of a block the Super Bell flies out of the pen; on the floor it stays in
                // it [obs: emulator]
                e.place(item, bx, up(1)).map_err(err)?;
            } else {
                let i = e.place(5, bx, by).map_err(err)?;
                e.try_set_child(i, item, mmk_editor::DEFAULT_FLAGS)
                    .map_err(err)?;
            }
            for y in 1..=2 {
                e.paint_ground(bx - 4, up(y)).map_err(err)?;
                e.paint_ground(bx + 4, up(y)).map_err(err)?;
            }
        }
        "wall" => {
            // floor, then a 12-tall wall at x 40 and a second one at x 46 (a 5-wide shaft for
            // wall jumps), a low ceiling at x 20..26, 3 tiles up
            floor(&mut e, 0, len - 1)?;
            for y in 1..=12 {
                e.paint_ground(40, up(y)).map_err(err)?;
                e.paint_ground(46, up(y)).map_err(err)?;
            }
            for x in 20..=26 {
                e.paint_ground(x, up(3)).map_err(err)?;
            }
        }
        "slope" => {
            // all four slope kinds with their bottom row in the floor's top row (a slope's low
            // end is a tile above its bottom row): floor to x 23, gentle up (8×4), plateau,
            // steep down (5×4), floor x 45..49, steep up, plateau, gentle down; orientation
            // (bits 20–21): 0 rises to the right, 3 falls
            let rise = mmk_editor::DEFAULT_FLAGS;
            let fall = mmk_editor::DEFAULT_FLAGS | (3 << 20);
            let row = base - 1;
            floor(&mut e, 0, 23)?;
            e.place_sized(87, 24, row, 8, 4, rise).map_err(err)?;
            for x in (32..=39).chain(55..=59) {
                for y in 0..=up(3) {
                    e.paint_ground(x, y).map_err(err)?;
                }
            }
            e.place_sized(88, 40, row, 5, 4, fall).map_err(err)?;
            floor(&mut e, 45, 49)?;
            e.place_sized(88, 50, row, 5, 4, rise).map_err(err)?;
            e.place_sized(87, 60, row, 8, 4, fall).map_err(err)?;
            floor(&mut e, 68, len - 1)?;
        }
        "cpipe" => {
            // clear pipes ridden left to right by a walking player, on the floor (top row 1) and
            // a platform 6 high (top row 5) from x 36:
            // - x 12..21: a straight pipe, 10 tiles, rows 2–3, out onto the floor;
            // - x 28..35: right 3, a bend up, up 2, a bend right, right 3, out onto the platform;
            // - x 40..47: two pipes of 4 joined end to end (one ride);
            // - x 50..53: a pipe of 4 with a wall at its exit (the ride turns back)
            floor(&mut e, 0, len - 1)?;
            for x in 36..=63 {
                for y in base..=5 {
                    e.paint_ground(x, y).map_err(err)?;
                }
            }
            for y in 6..=7 {
                e.paint_ground(54, y).map_err(err)?;
            }
            clear_pipe(&mut e, (12, 3), &[(21, 2)])?;
            clear_pipe(
                &mut e,
                (28, 3),
                &[(30, 2), (31, 4), (31, 5), (33, 6), (35, 6)],
            )?;
            clear_pipe(&mut e, (40, 7), &[(43, 6)])?;
            clear_pipe(&mut e, (44, 7), &[(47, 6)])?;
            clear_pipe(&mut e, (50, 7), &[(53, 6)])?;
        }
        "mushstart" => {
            // Super Mushrooms placed on the floor near the start, the player standing still: one
            // in the open at x 10, one shut in a one-tile box at x 14 (walls at 13 and 15), one
            // with a wall flush on its right at x 18 (wall 19), one with a wall flush on its left
            // at x 21 (wall 20)
            floor(&mut e, 0, len - 1)?;
            for x in [13, 15, 19, 20] {
                e.paint_ground(x, up(1)).map_err(err)?;
            }
            for x in [10, 14, 18, 21] {
                e.place(20, x, up(1)).map_err(err)?;
            }
        }
        "cpipev" => {
            // clear pipes' vertical mouths: x 12..16, right 3 from a side mouth, a bend up and up
            // 3 to an upward mouth (out on top, back in with Down, out of the side mouth); x 7–8,
            // a pipe up 6 tiles from a downward mouth 3 tiles over the floor (in from below with
            // Up held in a jump, out of its top)
            floor(&mut e, 0, len - 1)?;
            clear_pipe(&mut e, (12, 3), &[(14, 2), (15, 4), (15, 6)])?;
            e.place_clear_pipe((8, 5), mmk_formats::smm2::clear_pipe::Dir::Up)
                .map_err(err)?;
            let slot = e.clear_pipes().last().map(|p| p.0).ok_or("no pipe")?;
            e.draw_clear_pipe(slot, mmk_editor::clearpipe::PipeEnd::End, (8, 10))
                .map_err(err)?;
        }
        "cpipeg" => {
            // a Goomba at x 22 walking left into the right mouth of a straight 8-tile pipe at
            // x 10..17 (rows 2–3), out of its left mouth
            floor(&mut e, 0, len - 1)?;
            clear_pipe(&mut e, (10, 3), &[(17, 2)])?;
            e.place(0, 22, up(1)).map_err(err)?;
        }
        "slopejoin" => {
            // gentle slopes (4×2 unless noted) beside ground, an open pit under each, walked over
            // left to right on a 5-high floor (top row 4, up a steep slope from the start
            // ground), flat stretches between them. A copies W_3049200's x 50..62, where the game
            // walks flat over the slope's first step:
            // - A x 14: rising, ground above the low end cap and under it (W_3049200's tile ids);
            // - B x 34: rising 6×3, ground above the cap and under it;
            // - D x 50: rising, ground above the cap only (nothing under the cap);
            // - F x 62: rising, ground under the high cap only (the layout's other empty corner;
            //   ground over a diagonal cell makes the game reject the course [obs: emulator]);
            // - G x 74: rising, no ground above the cap (a plain dip: the control);
            // - C x 86: falling, ground above the low cap (on the right) and under it
            let rise = mmk_editor::DEFAULT_FLAGS;
            let fall = mmk_editor::DEFAULT_FLAGS | (1 << 20);
            let pit = |e: &mut Editor, x0: i32, x1: i32, below: i32| {
                for x in x0..=x1 {
                    for y in 0..below {
                        e.erase_ground(x, y);
                    }
                }
            };
            floor(&mut e, 0, len - 1)?;
            for x in 12..len - 11 {
                for y in 2..=4 {
                    e.paint_ground(x, y).map_err(err)?;
                }
            }
            pit(&mut e, 7, 11, 2);
            for x in 7..=11 {
                e.paint_ground(x, 0).map_err(err)?;
            }
            e.place_sized(88, 7, 1, 5, 4, rise).map_err(err)?;
            // A: W_3049200's tiles (columns 50..62 → 14..26), its slope at (55, 3) → (19, 3)
            const A: [(i32, i32, u8); 33] = [
                (51, 4, 59),
                (52, 4, 9),
                (53, 4, 10),
                (54, 4, 10),
                (55, 4, 59),
                (55, 2, 63),
                (55, 1, 63),
                (55, 0, 63),
                (54, 0, 62),
                (53, 0, 62),
                (52, 0, 14),
                (51, 0, 62),
                (50, 0, 13),
                (50, 3, 62),
                (50, 2, 13),
                (51, 2, 62),
                (51, 1, 62),
                (50, 1, 12),
                (52, 2, 14),
                (52, 3, 13),
                (51, 3, 62),
                (52, 1, 13),
                (53, 1, 62),
                (53, 2, 62),
                (54, 2, 12),
                (54, 3, 62),
                (54, 1, 14),
                (53, 3, 62),
                (50, 4, 11),
                (59, 4, 26),
                (60, 4, 7),
                (61, 4, 6),
                (62, 4, 26),
            ];
            pit(&mut e, 19, 22, 5);
            e.place_sized(87, 19, 3, 4, 2, rise).map_err(err)?;
            // B
            pit(&mut e, 34, 39, 5);
            for y in 0..=2 {
                e.paint_ground(34, y).map_err(err)?;
            }
            e.paint_ground(34, 4).map_err(err)?;
            e.place_sized(87, 34, 3, 6, 3, rise).map_err(err)?;
            for x in 40..=43 {
                e.paint_ground(x, 5).map_err(err)?;
            }
            // D
            pit(&mut e, 50, 53, 5);
            e.paint_ground(50, 4).map_err(err)?;
            e.place_sized(87, 50, 3, 4, 2, rise).map_err(err)?;
            // F
            pit(&mut e, 62, 65, 5);
            for y in 0..=2 {
                e.paint_ground(62, y).map_err(err)?;
            }
            e.paint_ground(65, 3).map_err(err)?;
            e.place_sized(87, 62, 3, 4, 2, rise).map_err(err)?;
            // G
            pit(&mut e, 74, 77, 5);
            for y in 0..=2 {
                e.paint_ground(74, y).map_err(err)?;
            }
            e.place_sized(87, 74, 3, 4, 2, rise).map_err(err)?;
            // C (low end on the right, at x 89)
            pit(&mut e, 86, 89, 5);
            for y in 0..=2 {
                e.paint_ground(89, y).map_err(err)?;
            }
            e.paint_ground(89, 4).map_err(err)?;
            e.place_sized(87, 86, 3, 4, 2, fall).map_err(err)?;
            // A's tile ids as W_3049200 has them (last: painting re-tiles neighbours)
            for (x, y, id) in A {
                let (x, y) = (x - 36, y);
                e.paint_ground(x, y).map_err(err)?;
                let g = e.course.main.ground.items.iter_mut();
                for t in g.filter(|t| t.x as i32 == x && t.y as i32 == y) {
                    t.id = id;
                }
            }
        }
        _ => unreachable!(),
    }
    // under a slope's bottom row in 3D World (base 2): fill the floor row below it
    if base == 2 && kind == "slope" {
        for x in (24..=31).chain(40..=44).chain(50..=54).chain(60..=67) {
            e.paint_ground(x, 0).map_err(err)?;
        }
    }
    e.save().map_err(err)
}

/// A clear pipe drawn from grid corner `corner` to the right, then on toward each tile of
/// `path` in turn (the editor's gesture: on, or round a bend to the side).
fn clear_pipe(e: &mut Editor, corner: (i32, i32), path: &[(i32, i32)]) -> Result<(), String> {
    use mmk_editor::clearpipe::PipeEnd;
    use mmk_formats::smm2::clear_pipe::Dir;
    let err = |x: mmk_editor::EditError| format!("{x:?}");
    let slot = e.place_clear_pipe(corner, Dir::Right).map_err(err)?;
    for &t in path {
        e.draw_clear_pipe(slot, PipeEnd::End, t).map_err(err)?;
    }
    Ok(())
}

/// Encrypt a course and wrap it like the game's built-in courses (Yaz0 → SARC → `<stem>.bcd`).
pub fn szs(course: &[u8], table: &KeyTable, stem: &str) -> Result<Vec<u8>, String> {
    let iv = [0x5Au8; 16];
    let state = [0x1234_5678, 0x9ABC_DEF0, 0x0F1E_2D3C, 0x4B5A_6978];
    let bcd = crypto::encrypt_bcd(course, table, crypto::default_header(), iv, state)
        .map_err(|e| e.to_string())?;
    let name = format!("{stem}.bcd");
    let arc = sarc::build(&[(name.as_str(), &bcd)]);
    Ok(yaz0_store(&align_data(&arc, 0x100)))
}

/// Move a SARC's data section to `offset` (the game's own course archives keep it at 0x100).
fn align_data(arc: &[u8], offset: usize) -> Vec<u8> {
    let old = u32::from_le_bytes(arc[0x0C..0x10].try_into().unwrap()) as usize;
    if old >= offset {
        return arc.to_vec();
    }
    let mut out = arc[..old].to_vec();
    out.resize(offset, 0);
    out.extend_from_slice(&arc[old..]);
    let len = out.len() as u32;
    out[0x08..0x0C].copy_from_slice(&len.to_le_bytes());
    out[0x0C..0x10].copy_from_slice(&(offset as u32).to_le_bytes());
    out
}

/// Yaz0 without back-references (every group is 8 literal bytes): valid for any reader.
fn yaz0_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + data.len() * 9 / 8 + 1);
    out.extend_from_slice(b"Yaz0");
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0u8; 8]);
    for chunk in data.chunks(8) {
        out.push(0xFF);
        out.extend_from_slice(chunk);
    }
    out
}

/// Write `<name>.course` for each selected course; with `title_dir` (needs the key table), also
/// the last one as the ten replacement title courses `title_course_data_00…09.szs` (the title
/// screen plays one of them, so an emulator with a file-replacement mod boots straight into it).
pub fn write_courses(
    out: &Path,
    only: Option<&str>,
    table: Option<&KeyTable>,
    title_dir: Option<&Path>,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut last = None;
    // `only` may also name a raw course file (any course becomes the title course)
    if let Some(path) = only.filter(|o| Path::new(o).is_file()) {
        last = Some(std::fs::read(path).map_err(|e| e.to_string())?);
    }
    for name in course_names() {
        if last.is_some() && only.is_some_and(|o| Path::new(o).is_file()) {
            break;
        }
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let c = build_course(&name)?;
        std::fs::write(out.join(format!("{name}.course")), &c).map_err(|e| e.to_string())?;
        println!("{name}");
        last = Some(c);
    }
    if let Some(dir) = title_dir {
        let table = table.ok_or("--title needs --table")?;
        let c = last.ok_or("no course selected")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for i in 0..10 {
            let stem = format!("title_course_data_{i:02}");
            std::fs::write(dir.join(format!("{stem}.szs")), szs(&c, table, &stem)?)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_expand_per_frame() {
        let s = parse_script("2 -\n# comment\n3 RIGHT Y  # run\n1 right a\n").unwrap();
        assert_eq!(s.len(), 6);
        assert_eq!(s[0].buttons, 0);
        assert_eq!(s[2].buttons, input::RIGHT | input::RUN);
        assert_eq!(s[5].buttons, input::RIGHT | input::JUMP);
        assert!(parse_script("x RIGHT").is_err());
        assert!(parse_script("1 FLY").is_err());
    }

    #[test]
    fn every_test_course_builds_and_runs() {
        let script = parse_script("10 -\n30 RIGHT RUN\n20 RIGHT JUMP\n").unwrap();
        for name in course_names() {
            let c = build_course(&name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let csv = run(
                &c,
                &script,
                &Start {
                    pos: None,
                    facing: None,
                    form: None,
                    vx: None,
                    actor: None,
                },
            )
            .unwrap();
            assert_eq!(csv.lines().count(), 61, "{name}");
        }
    }

    #[test]
    fn walking_right_moves_right() {
        let c = build_course("M1_flat").unwrap();
        let script = parse_script("40 RIGHT").unwrap();
        let csv = run(
            &c,
            &script,
            &Start {
                pos: None,
                facing: None,
                form: None,
                vx: None,
                actor: None,
            },
        )
        .unwrap();
        let last = csv.lines().last().unwrap();
        let vx: f32 = last.split(',').nth(3).unwrap().parse().unwrap();
        assert!((vx - 1.5).abs() < 1e-4, "{last}");
    }

    #[test]
    fn yaz0_store_round_trips() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let z = yaz0_store(&data);
        assert_eq!(mmk_formats::archive::yaz0::decompress(&z).unwrap(), data);
    }
}
