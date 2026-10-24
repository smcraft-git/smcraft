//! Course and area settings: name, style, timer, theme / night, area size and orientation,
//! start / goal heights, clear condition, autoscroll (with custom-scroll nodes), liquid.

use mmk_formats::smm2::clear;

use crate::{DEFAULT_FLAGS, EditError, Editor, Rect, parts};

/// Width of a new sub-area: every unused sub-area of the sample is 84 tiles wide [corp].
pub const SUB_DEFAULT_WIDTH: i32 = 84;
/// Horizontal area widths in tiles [corp: 24–240 in the sample].
pub const WIDTH_MIN: i32 = 24;
pub const WIDTH_MAX: i32 = 240;
/// Horizontal areas are 27 tiles tall [corp: every one].
pub const HEIGHT_H: i32 = 27;
/// Vertical sub-areas: 48 tiles wide, 28–168 tiles tall in steps of 7 [corp]; new ones 42.
pub const VERT_W: i32 = 48;
pub const VERT_MIN: i32 = 28;
pub const VERT_MAX: i32 = 168;
pub const VERT_STEP: i32 = 7;
pub const VERT_DEFAULT: i32 = 42;
/// Timer range in seconds [obs themes.md: 10–500; corp: all multiples of 10].
pub const TIMER_MIN: i16 = 10;
pub const TIMER_MAX: i16 = 500;
/// Custom autoscroll nodes per area (the editor's counter limit) [obs].
pub const SCROLL_NODES_MAX: usize = 10;

/// The play view the custom scroll's camera centre moves in (sim `world::VIEW_W` / `VIEW_H`):
/// 24 × 13.5 tiles.
pub const VIEW_W: f32 = 24.0 * 16.0;
pub const VIEW_H: f32 = 13.5 * 16.0;

/// A point of the custom scroll's path ([`Editor::scroll_path`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPoint {
    pub kind: ScrollKind,
    /// The camera centre there (world px).
    pub x: f32,
    pub y: f32,
    /// Speed of the leg after it: 0 slow, 1 normal, 2 fast (3 stops).
    pub speed: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKind {
    /// The start unit (its speed is the course header's 0x0E).
    Start,
    /// A course node: its object index.
    Node(usize),
    End,
}

/// Liquid behaviour (area header +5 / +6): mode 0 stays, 1 rises or falls, 2 rises and falls;
/// speed 1 slow – 3 fast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Liquid {
    pub mode: u8,
    pub speed: u8,
    /// Heights in tiles.
    pub start: u8,
    pub end: u8,
}

impl Editor {
    pub fn set_name(&mut self, name: &str) {
        self.course
            .header
            .name
            .set(&name.chars().take(32).collect::<String>());
    }

    pub fn set_description(&mut self, text: &str) {
        self.course
            .header
            .description
            .set(&text.chars().take(75).collect::<String>());
    }

    /// Game style code: "M1", "M3", "MW", "WU", "3W". Parts the new style lacks stay in the file
    /// (the game asks before converting; here the caller decides).
    pub fn set_style(&mut self, style: &str) {
        self.course.header.game_style.set(style);
    }

    /// Time limit in seconds, 10–500 in steps of 10.
    pub fn set_timer(&mut self, seconds: i16) {
        let s = seconds.clamp(TIMER_MIN, TIMER_MAX);
        self.course.header.timer = (s + 5) / 10 * 10;
    }

    /// Theme of the current area: 0 ground … 9 forest.
    pub fn set_theme(&mut self, theme: u8) {
        self.area_mut().header.theme = theme.min(9);
    }

    /// Night version of the current area's theme (not in 3D World).
    pub fn set_night(&mut self, night: bool) {
        let night = night && self.course.header.game_style.as_str_lossy() != "3W";
        let h = &mut self.area_mut().header;
        h.flags = if night { h.flags | 2 } else { h.flags & !2 };
    }

    /// Height (tiles) of the start and goal ground: start 1–25, goal 1–15 (both from 2 in 3D
    /// World) [obs: course validator 0x710056d490].
    pub fn set_start_goal(&mut self, start_y: u8, goal_y: u8) {
        let low = if self.course.header.game_style.as_str_lossy() == "3W" {
            2
        } else {
            1
        };
        self.course.header.start_y = start_y.clamp(low, 25);
        self.course.header.goal_y = goal_y.clamp(low, 15);
    }

    /// Main-area width in tiles (kept for older callers): `set_area_width` on area 0.
    pub fn set_length(&mut self, tiles: i32) {
        let area = self.area;
        self.area = 0;
        let _ = self.set_area_width(tiles);
        self.area = area;
    }

    /// Width (horizontal areas, 24–240 tiles) or height (vertical sub-areas, 28–168 in steps of 7)
    /// of the current area. Fails if parts, ground or tracks would end up outside. In the main
    /// area the goal moves with the right edge: goal_x = (right − 152 px) / 1.6 [course-loading
    /// §1.5].
    pub fn set_area_width(&mut self, tiles: i32) -> Result<(), EditError> {
        let vertical = self.area().header.orientation != 0;
        let (w, h) = if vertical {
            let t = ((tiles.clamp(VERT_MIN, VERT_MAX) + VERT_STEP / 2) / VERT_STEP) * VERT_STEP;
            (VERT_W, t)
        } else {
            (tiles.clamp(WIDTH_MIN, WIDTH_MAX), HEIGHT_H)
        };
        self.resize_area(w, h)
    }

    /// Make the sub-area vertical (48 tiles wide, 42 tall) or horizontal (84 × 27). The main area
    /// is always horizontal.
    pub fn set_vertical(&mut self, vertical: bool) -> Result<(), EditError> {
        if self.area == 0 {
            return Err(EditError::NotAllowed("the main area is horizontal"));
        }
        let (w, h) = if vertical {
            (VERT_W, VERT_DEFAULT)
        } else {
            (SUB_DEFAULT_WIDTH, HEIGHT_H)
        };
        self.resize_area(w, h)?;
        let a = self.area_mut();
        a.header.orientation = vertical as u8;
        // a sub-area has no custom autoscroll
        if a.header.autoscroll_type == 4 {
            a.header.autoscroll_type = 0;
        }
        self.course.header.management_flags |= 0x40;
        Ok(())
    }

    fn resize_area(&mut self, w: i32, h: i32) -> Result<(), EditError> {
        let inside = Rect::new(0, 0, w, h);
        let a = self.area();
        let outside = a.objects.iter().any(|o| {
            !crate::rules::is_rider(o)
                && !crate::rules::own_layer(o.id)
                && !rect_within(&parts::footprint(o), &inside)
        }) || a
            .ground
            .iter()
            .any(|g| !inside.contains(g.x as i32, g.y as i32))
            || a.tracks.iter().any(|t| {
                let (cx, cy) = crate::tracks::centre(t);
                !inside.contains(cx, cy)
            });
        if outside {
            return Err(EditError::Occupied);
        }
        let main = self.area == 0;
        // custom-scroll nodes past the new edges come inside (the camera clamps them anyway)
        for o in self.area_mut().objects.items.iter_mut() {
            if o.id == 89 {
                o.x = o.x.min(((w - 2) * 16 + 8) * 10);
                o.y = o.y.min(((h - 1) * 16 + 8) * 10);
            }
        }
        let hd = &mut self.area_mut().header;
        hd.boundary_right = w * 16;
        hd.boundary_top = h * 16;
        if main {
            self.course.header.goal_x = (((w * 16 - 152) * 10) as f32 / 16.0).round() as i16;
        }
        Ok(())
    }

    // ------------------------------------------------------------ clear condition

    /// Clear condition: `category` 1 parts (defeat / collect / use `magnitude`), 2 status, 3
    /// actions; `index` in the game's condition list (formats `smm2::clear`); 0 for none.
    pub fn set_clear_condition(
        &mut self,
        category: u8,
        index: u8,
        magnitude: i16,
    ) -> Result<(), EditError> {
        let h = &mut self.course.header;
        if category == clear::NONE || index == 0 {
            h.clear_condition_category = 0;
            h.clear_condition = 0;
            h.clear_condition_magnitude = 0;
            h.management_flags &= !0x20;
            return Ok(());
        }
        if !clear::categories(index).contains(&category) {
            return Err(EditError::NotAllowed("clear condition"));
        }
        let hash = clear::hash(index).ok_or(EditError::NotAllowed("clear condition"))?;
        let n = if category == clear::PARTS {
            magnitude.clamp(1, 9999)
        } else {
            0
        };
        h.clear_condition_category = category;
        h.clear_condition = hash;
        h.clear_condition_magnitude = n;
        // management bit 5: the condition has an amount [ext smm2-course.md; corp: 58 of 73]
        if n > 0 {
            h.management_flags |= 0x20;
        } else {
            h.management_flags &= !0x20;
        }
        Ok(())
    }

    // ------------------------------------------------------------ autoscroll

    /// Autoscroll of the current area: 0 none, 1 slow, 2 normal, 3 fast, 4 custom (main area
    /// only, the route is the custom-scroll nodes) [obs: the course validator allows 0–3 in the
    /// sub-area].
    pub fn set_autoscroll(&mut self, kind: u8) {
        let kind = if kind >= 4 && self.area != 0 {
            0
        } else {
            kind.min(4)
        };
        self.area_mut().header.autoscroll_type = kind;
    }

    /// Custom autoscroll speed (course header 0x0E): 0 ×1, 1 ×2, 2 ×3.
    pub fn set_custom_scroll_speed(&mut self, speed: u8) {
        self.course.header.autoscroll_speed = speed.min(2);
    }

    /// Add a custom-scroll node (part 89, 3×1, centred on tile (tx, ty)) with speed `speed`
    /// ((flags >> 18) & 3: 0 slow, 1 normal, 2 fast) for the leg after it. Nodes are numbered
    /// 1… along the path, left to right ([`Editor::renumber_scroll_nodes`]); the start (0) and
    /// the end (11) are the maker's own units, not course parts (89-autoscroll-camera.md Pass 3).
    /// Returns the new node's index.
    pub fn add_scroll_node(&mut self, tx: i32, ty: i32, speed: u8) -> Result<usize, EditError> {
        let n = self.area().objects.iter().filter(|o| o.id == 89).count();
        if self.area != 0 {
            return Err(EditError::NotAllowed("custom autoscroll is main-area only"));
        }
        if n >= SCROLL_NODES_MAX {
            return Err(EditError::Limit("custom scroll nodes"));
        }
        if !self.in_bounds(tx, ty) {
            return Err(EditError::OutOfBounds);
        }
        let (w, _) = self.size();
        let tx = tx.clamp(1, w - 2);
        let flags = DEFAULT_FLAGS | (speed.min(2) as u32) << 18;
        let o = parts::new_object(89, tx - 1, ty, 3, 1, flags);
        let i = self.push_object(o);
        self.renumber_scroll_nodes();
        Ok(i)
    }

    /// The custom-scroll nodes of the area in path order: (object index, centre tile, speed).
    pub fn scroll_nodes(&self) -> Vec<(usize, (i32, i32), u8)> {
        let mut v: Vec<(u32, usize, (i32, i32), u8)> = self
            .area()
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.id == 89)
            .map(|(i, o)| {
                let r = parts::footprint(o);
                (
                    (o.flags >> 20) & 0xF,
                    i,
                    (r.x + r.w / 2, r.y),
                    ((o.flags >> 18) & 3) as u8,
                )
            })
            .collect();
        v.sort_by_key(|n| (n.0, n.1));
        v.into_iter().map(|(_, i, t, s)| (i, t, s)).collect()
    }

    /// Move node `index` so its centre is on tile (tx, ty) (inside the area); the path order
    /// follows.
    pub fn move_scroll_node(&mut self, index: usize, tx: i32, ty: i32) -> Result<(), EditError> {
        let (w, h) = self.size();
        let Some(o) = self.area_mut().objects.items.get_mut(index) else {
            return Err(EditError::Missing);
        };
        if o.id != 89 {
            return Err(EditError::Missing);
        }
        let (tx, ty) = (tx.clamp(1, w - 2), ty.clamp(0, h - 1));
        o.x = (tx * 16 + 8) * 10;
        o.y = (ty * 16 + 8) * 10;
        self.renumber_scroll_nodes();
        Ok(())
    }

    /// Speed of node `index` for the leg after it (0 slow, 1 normal, 2 fast).
    pub fn set_scroll_node_speed(&mut self, index: usize, speed: u8) {
        if let Some(o) = self.area_mut().objects.items.get_mut(index)
            && o.id == 89
        {
            o.flags = (o.flags & !(3 << 18)) | (speed.min(2) as u32) << 18;
        }
    }

    /// Take node `index` out of the path (the others are numbered again).
    pub fn remove_scroll_node(&mut self, index: usize) {
        if self.object(index).is_some_and(|o| o.id == 89) {
            self.area_mut().objects.items.remove(index);
            self.renumber_scroll_nodes();
        }
    }

    /// Number the nodes 1… left to right (ties: the old order), (flags >> 20) & 0xF: the path
    /// runs through them in that order [inf: the game's numbering rule was not read; the
    /// corpus' nodes go along the course].
    pub fn renumber_scroll_nodes(&mut self) {
        let mut v: Vec<(i32, u32, usize)> = self
            .area()
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.id == 89)
            .map(|(i, o)| (o.x, (o.flags >> 20) & 0xF, i))
            .collect();
        v.sort();
        for (k, (_, _, i)) in v.into_iter().enumerate() {
            let o = &mut self.area_mut().objects.items[i];
            o.flags = (o.flags & !(0xF << 20)) | ((k as u32 + 1) & 0xF) << 20;
        }
    }

    /// The custom scroll's path as the camera follows it (world px, the camera centre): the
    /// start unit, the nodes, the end unit; each point with its kind and the speed of the leg
    /// after it. As the play side builds it (sim `Level::finish_custom_scroll`, 89-autoscroll-
    /// camera.md Pass 3): the start at min(right − 160, 160) + 24, height (start_y + 4)·16 − 8
    /// clamped to 104…328, + 4, speed header 0x0E; the end likewise at max(right − 160, 160)
    /// − 24 from goal_y; every point clamped to the view-centre range.
    pub fn scroll_path(&self) -> Vec<ScrollPoint> {
        self.scroll_points(true)
    }

    /// Where the custom scroll's markers stand in the Course Maker: the points of
    /// [`Editor::scroll_path`] before the clamp to the view-centre range (the start unit at x
    /// 184 on a new course, where the camera starts at 192) [obs: the reference frames,
    /// research/specs/maker.md C13].
    pub fn scroll_marks(&self) -> Vec<ScrollPoint> {
        self.scroll_points(false)
    }

    fn scroll_points(&self, clamp: bool) -> Vec<ScrollPoint> {
        let a = &self.course.main.header;
        let (left, right) = (a.boundary_left as f32, a.boundary_right as f32);
        let (bottom, top) = (a.boundary_bottom as f32, a.boundary_top as f32);
        let span = |v: f32, lo: f32, hi: f32| {
            if lo > hi {
                (lo + hi) / 2.0
            } else {
                v.clamp(lo, hi)
            }
        };
        let (x0, x1) = (left + VIEW_W / 2.0, right - VIEW_W / 2.0);
        let (y0, y1) = (bottom + VIEW_H / 2.0, top - VIEW_H / 2.0);
        let y = |h: u8| {
            let y = if a.boundary_type != 0 {
                104.0
            } else {
                ((h as f32 + 4.0) * 16.0 - 8.0).clamp(104.0, 328.0)
            };
            y + 4.0
        };
        let h = &self.course.header;
        let edge = right - 160.0;
        let mut v = vec![ScrollPoint {
            kind: ScrollKind::Start,
            x: edge.min(160.0) + 24.0,
            y: y(h.start_y),
            speed: h.autoscroll_speed & 3,
        }];
        let main = &self.course.main;
        let mut nodes: Vec<(u32, usize)> = main
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.id == 89)
            .map(|(i, o)| ((o.flags >> 20) & 0xF, i))
            .collect();
        nodes.sort();
        for (n, i) in nodes {
            let o = &main.objects.items[i];
            let p = ScrollPoint {
                kind: ScrollKind::Node(i),
                x: o.x as f32 / 10.0,
                y: o.y as f32 / 10.0,
                speed: ((o.flags >> 18) & 3) as u8,
            };
            match n {
                // a node 0 replaces the start, an 11 is the end
                0 => v[0] = p,
                11 => {}
                _ => v.push(p),
            }
        }
        v.push(ScrollPoint {
            kind: ScrollKind::End,
            x: edge.max(160.0) - 24.0,
            y: y(h.goal_y),
            speed: 0,
        });
        if clamp {
            for p in &mut v {
                p.x = span(p.x, x0, x1);
                p.y = span(p.y, y0, y1);
            }
        }
        v
    }

    // ------------------------------------------------------------ liquid

    /// Water / lava / poison level of the current area (castle: lava, forest: water, poison at
    /// night; underwater areas are always full). Heights clamp to the area.
    pub fn set_liquid(&mut self, l: Liquid) {
        let (_, h) = self.size();
        let top = (h - 1).clamp(0, 255) as u8;
        let mode = l.mode.min(2);
        let speed = if mode == 0 { 0 } else { l.speed.clamp(1, 3) };
        let hd = &mut self.area_mut().header;
        hd.liquid_mode = mode;
        hd.liquid_speed = speed;
        hd.liquid_start_height = l.start.min(top);
        hd.liquid_end_height = if mode == 0 {
            l.start.min(top)
        } else {
            l.end.min(top)
        };
    }

    /// The current area's adjustable liquid, if its theme has one: 1 water (forest), 2 lava
    /// (castle), 3 poison (forest at night) [themes.md §3; obs: the maker's water-level actor
    /// draws only in themes 2 and 9, 0x7100effd00].
    pub fn liquid_kind(&self) -> Option<u8> {
        match self.theme() {
            (2, _) => Some(2),
            (9, true) => Some(3),
            (9, false) => Some(1),
            _ => None,
        }
    }

    /// The heights (tiles) the maker's water-level handles may take: 0 … 24 (from 1 in 3D World,
    /// up to 167 in a vertical sub-area), within the area [obs 0x7100f008a0: one more than the
    /// dragged height is clamped to 1 (2 when the edit scene's field +0x1c is 4, read as the
    /// style 3D World [inf]) … 25 (168 when the area is vertical)].
    pub fn liquid_range(&self) -> (u8, u8) {
        let low = if self.course.header.game_style.as_str_lossy() == "3W" {
            1
        } else {
            0
        };
        let (_, h) = self.size();
        let high = if self.area().header.orientation != 0 {
            167
        } else {
            24
        };
        (low, high.min(h - 1).clamp(low as i32, 255) as u8)
    }

    pub fn liquid(&self) -> Liquid {
        let h = &self.area().header;
        Liquid {
            mode: h.liquid_mode,
            speed: h.liquid_speed,
            start: h.liquid_start_height,
            end: h.liquid_end_height,
        }
    }
}

fn rect_within(r: &Rect, outer: &Rect) -> bool {
    r.x >= outer.x
        && r.y >= outer.y
        && r.x + r.w <= outer.x + outer.w
        && r.y + r.h <= outer.y + outer.h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::*;
    use mmk_sim::level::Level;

    #[test]
    fn both_areas_and_vertical_sub_areas() {
        let mut e = Editor::new("M1");
        for x in 7..100 {
            e.paint_ground(x, 0).unwrap();
        }
        e.set_area(1);
        e.set_theme(1);
        e.set_night(true);
        e.set_vertical(true).unwrap();
        assert_eq!(e.size(), (48, 42));
        e.set_area_width(100).unwrap();
        assert_eq!(e.size(), (48, 98), "steps of 7");
        for x in 0..48 {
            e.paint_ground(x, 0).unwrap();
        }
        e.place(0, 10, 60).unwrap();
        assert_eq!(
            e.set_area_width(28),
            Err(EditError::Occupied),
            "a Goomba would be outside"
        );
        e.set_area(0);
        e.set_area_width(120).unwrap();
        assert_eq!(e.course.header.goal_x, ((120 * 16 - 152) * 10 / 16) as i16);
        assert!(e.set_vertical(true).is_err());
        let back = round_trip(&e);
        assert!(back.header.management_flags & 0x40 != 0);
        let sub = Level::from_course(&back, 1);
        assert!(sub.vertical && sub.night && sub.theme == 1);
        assert_eq!(sub.top, 98.0 * 16.0);
        let main = Level::from_course(&back, 0);
        assert_eq!(main.right, 120.0 * 16.0);
        assert!(main.goal.is_some());
    }

    #[test]
    fn course_settings() {
        let mut e = flat("M1");
        e.set_timer(333);
        assert_eq!(e.course.header.timer, 330);
        e.set_timer(2);
        assert_eq!(e.course.header.timer, 10);
        // defeat 3 Goombas
        e.set_clear_condition(clear::PARTS, 1, 3).unwrap();
        assert_eq!(e.course.header.clear_condition, 0x7f07acbf);
        assert!(
            e.set_clear_condition(clear::STATUS, 1, 0).is_err(),
            "Goombas are a count"
        );
        for x in [20, 24, 28] {
            e.place(0, x, 2).unwrap();
        }
        let back = round_trip(&e);
        let w = mmk_sim::World::from_course(&back);
        assert_eq!(w.clear.goal, mmk_sim::clear::Goal::Defeat(1));
        assert_eq!(w.clear.n, 3);
        // reach the goal without taking damage
        e.set_clear_condition(clear::ACTIONS, 70, 5).unwrap();
        assert_eq!(e.course.header.clear_condition_magnitude, 0);
        let w = mmk_sim::World::from_course(&round_trip(&e));
        assert_eq!(w.clear.goal, mmk_sim::clear::Goal::NoDamage);
        e.set_clear_condition(0, 0, 0).unwrap();
        assert_eq!(e.course.header.management_flags & 0x20, 0);
    }

    #[test]
    fn autoscroll_and_liquid() {
        let mut e = flat("M1");
        e.set_autoscroll(4);
        e.set_custom_scroll_speed(1);
        e.add_scroll_node(12, 6, 0).unwrap();
        e.add_scroll_node(40, 12, 2).unwrap();
        e.set_theme(2);
        e.set_liquid(Liquid {
            mode: 2,
            speed: 3,
            start: 2,
            end: 8,
        });
        let back = round_trip(&e);
        let lvl = Level::from_course(&back, 0);
        assert_eq!(lvl.autoscroll, 4);
        // the two nodes (numbered 1 and 2 left to right) between the start and the end units
        let mut nodes = lvl.scroll_nodes.clone();
        nodes.sort_by_key(|n| n.0);
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[2], (2, 40.0 * 16.0 + 8.0, 12.0 * 16.0 + 8.0, 2));
        let l = lvl.liquid.unwrap();
        assert_eq!(
            (l.kind, l.mode, l.speed),
            (2, 2, 3),
            "lava, rising and falling, fast"
        );
        e.set_liquid(Liquid {
            mode: 0,
            speed: 3,
            start: 4,
            end: 9,
        });
        assert_eq!(
            e.liquid(),
            Liquid {
                mode: 0,
                speed: 0,
                start: 4,
                end: 4
            }
        );
    }

    #[test]
    fn the_scroll_path_is_the_one_the_camera_follows() {
        let mut e = flat("M1");
        e.set_autoscroll(4);
        e.set_custom_scroll_speed(1);
        // put out of order: numbered by x
        let b = e.add_scroll_node(60, 3, 2).unwrap();
        let a = e.add_scroll_node(30, 20, 0).unwrap();
        let nodes = e.scroll_nodes();
        assert_eq!(nodes.iter().map(|n| n.0).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(nodes[0].1, (30, 20));
        let path = e.scroll_path();
        let lvl = Level::from_course(&round_trip(&e), 0);
        let mut want = lvl.scroll_nodes.clone();
        want.sort_by_key(|n| n.0);
        assert_eq!(path.len(), want.len());
        for (p, w) in path.iter().zip(&want) {
            assert_eq!((p.x, p.y, p.speed), (w.1, w.2, w.3), "{p:?} {w:?}");
        }
        assert_eq!(path[0].kind, ScrollKind::Start);
        assert_eq!(path[0].speed, 1, "the start's speed is the header's");
        assert_eq!(path[1].kind, ScrollKind::Node(a));
        assert_eq!(path.last().unwrap().kind, ScrollKind::End);
        // the markers stand where the path is before the clamp: the start at 184 (the camera
        // from 192), the end 184 from the right; the nodes where they are (this one above
        // the camera's highest centre)
        let marks = e.scroll_marks();
        assert_eq!(marks.len(), path.len());
        assert_eq!(marks[0].x, 184.0);
        assert_eq!(path[0].x, 192.0);
        let right = e.area().header.boundary_right as f32;
        assert_eq!(marks.last().unwrap().x, right - 184.0);
        assert_eq!(
            (marks[1].x, marks[1].y),
            (30.0 * 16.0 + 8.0, 20.0 * 16.0 + 8.0)
        );
        assert!(path[1].y < marks[1].y);
        // a node moved past the other changes places on the path; speed and removal
        e.move_scroll_node(a, 80, 6).unwrap();
        let nodes = e.scroll_nodes();
        assert_eq!(nodes[0].0, b);
        e.set_scroll_node_speed(b, 1);
        assert_eq!(e.scroll_nodes()[0].2, 1);
        e.remove_scroll_node(b);
        let nodes = e.scroll_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(e.object(nodes[0].0).unwrap().flags >> 20 & 0xF, 1);
    }

    #[test]
    fn scroll_nodes_are_on_their_own_layer() {
        let mut e = flat("M1");
        e.set_autoscroll(4);
        let n = e.add_scroll_node(20, 8, 0).unwrap();
        assert!(e.object_at(20, 8).is_none(), "not picked as a part");
        e.place(4, 20, 8).expect("a block goes where a node is");
        assert!(e.objects_in(Rect::new(18, 7, 5, 3)).iter().all(|&i| i != n));
        for _ in 1..SCROLL_NODES_MAX {
            e.add_scroll_node(40, 6, 0).unwrap();
        }
        assert_eq!(
            e.add_scroll_node(50, 6, 0),
            Err(EditError::Limit("custom scroll nodes"))
        );
        // narrowing the area brings nodes inside
        let mut e = Editor::new("M1");
        e.add_scroll_node(150, 8, 0).unwrap();
        e.set_area_width(100).unwrap();
        let (_, (tx, _), _) = e.scroll_nodes()[0];
        assert!(tx < 100);
    }

    #[test]
    fn liquid_kind_and_range() {
        let mut e = flat("M1");
        assert_eq!(e.liquid_kind(), None);
        e.set_theme(9);
        assert_eq!(e.liquid_kind(), Some(1));
        e.set_night(true);
        assert_eq!(e.liquid_kind(), Some(3));
        e.set_theme(2);
        assert_eq!(e.liquid_kind(), Some(2));
        assert_eq!(e.liquid_range(), (0, 24));
        let mut e = flat("3W");
        e.set_theme(2);
        assert_eq!(e.liquid_range(), (1, 24));
    }
}
