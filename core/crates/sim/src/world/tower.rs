//! Stacked parts (flag 0x1000) in play: towers (research objects/stacks.md).
//!
//! A tower is a list of members, bottom first. The bottom one (the base) behaves as it would on
//! its own (a Goomba walks); every member above it is placed on the one below each frame:
//! carried by 0.9 of the lower one's sideways movement, pulled back toward it by a small
//! relative speed (so a walking column leans and sways), and falling under gravity onto its top.
//! A member that dies or leaves (held, kicked, …) drops out of the list; the ones above then
//! fall onto the member below it, or, when the one directly above flies, break away as a tower
//! of their own. A rider pulled too far from the member below (sideways past the two bodies, or
//! more than 64 px up or down) or standing on the ground leaves with everything above it.

use super::World;
use crate::actors::{Actor, Kind, Mode};
use crate::fmath::DetMath;
use crate::level::TILE;

/// Riders take this share of the lower member's sideways movement [obs].
const CARRY: f32 = 0.9;
/// Relative-speed pull toward the lower member, and its decay toward 0 each frame [obs].
const PULL: f32 = 0.009;
const DECAY: f32 = 0.005;
/// A rider further than this above or below the lower member's top leaves [obs].
const GAP: f32 = 64.0;
/// Fall acceleration and fastest fall of a rider [obs].
const GRAVITY: f32 = 0.1875;
const MAX_FALL: f32 = 4.0;
/// How far past its side a rider feels a wall it touches [inf].
const WALL_TOUCH: f32 = 0.5;
/// Facing passes up one member every this many frames [obs].
const TURN_STEP: u8 = 4;
/// The column holds at most this many members [obs].
pub(crate) const MAX_MEMBERS: usize = 128;

#[derive(Clone, Debug)]
pub(crate) struct Tower {
    /// A Bill Blaster or Cannon at the bottom: it stays put; x and top (px).
    fixed: Option<(f32, f32)>,
    members: Vec<Member>,
    clock: u8,
    /// The bottom member last frame (a rider that becomes the base keeps its fall speed).
    base: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
struct Member {
    id: u32,
    /// The facing this member takes (the base's, handed up the column).
    dir: i32,
    /// A rider's own sideways speed relative to the carry, and its fall speed.
    rel_vx: f32,
    vy: f32,
    /// Position at the end of the previous tower step (this frame's movement = now − last).
    last: (f32, f32),
}

impl Member {
    fn new(a: &Actor) -> Self {
        Self {
            id: a.id,
            dir: a.dir,
            rel_vx: 0.0,
            vy: 0.0,
            last: (a.x, a.y),
        }
    }
}

impl Actor {
    /// Height this part adds to a column (a member above stands this far over its feet), per
    /// kind and size [obs: the enemy table's tower heights].
    pub fn tower_height(&self) -> f32 {
        let small = match self.kind {
            Kind::Goomba | Kind::Goombrat | Kind::MontyMole => 15.0,
            Kind::Koopa { .. }
            | Kind::Shell { .. }
            | Kind::Muncher
            | Kind::Buzzy
            | Kind::SpikeTop { .. }
            | Kind::ChainChomp { .. }
            | Kind::AntTrooper { .. } => 14.0,
            Kind::DryBones => {
                if self.big {
                    return 30.0;
                }
                14.0
            }
            Kind::EmptyShell { what } => match what {
                2 => 12.0,
                _ => 14.0,
            },
            Kind::Spiny | Kind::CheepCheep { .. } | Kind::FishBone | Kind::LavaBubble => 12.0,
            Kind::Piranha { .. }
            | Kind::HammerBro
            | Kind::Blooper
            | Kind::Magikoopa
            | Kind::RockyWrench
            | Kind::Wiggler
            | Kind::ShoeGoomba { .. }
            | Kind::BowserJr { .. }
            | Kind::Stingby
            | Kind::Pokey { .. }
            | Kind::Skipsqueak { .. }
            | Kind::Spike => 16.0,
            Kind::BobOmb | Kind::Boo => 15.0,
            Kind::Lakitu => 22.0,
            Kind::Thwomp { .. } => 30.0,
            Kind::BoomBoom { .. } => 27.0,
            Kind::Porcupuffer => 24.0,
            Kind::Bully => 29.0,
            Kind::Bowser { .. } => 32.0,
            Kind::Koopaling { .. } => 18.0,
            _ => 15.0,
        };
        let big = match self.kind {
            // the big rows that aren't exactly twice the small one
            Kind::Goomba | Kind::Goombrat => 31.0,
            Kind::Thwomp { .. } => 62.0,
            Kind::BoomBoom { .. } => 52.0,
            Kind::Bully => 55.0,
            Kind::Stingby | Kind::Skipsqueak { .. } => 31.0,
            Kind::BobOmb | Kind::Boo | Kind::MontyMole => 30.0,
            _ => small * 2.0,
        };
        if self.big { big } else { small }
    }

    /// Flies (its table "air type" is airborne, or airborne when winged): the member right
    /// above one that drops out of a column breaks away instead of falling onto the one below
    /// [obs].
    pub fn flies_in_tower(&self) -> bool {
        match self.kind {
            Kind::Lakitu
            | Kind::Thwomp { .. }
            | Kind::Boo
            | Kind::Blooper
            | Kind::LavaBubble
            | Kind::Stingby
            | Kind::Cloud => true,
            Kind::Koopa { red } => red && self.wings,
            Kind::Goomba
            | Kind::Goombrat
            | Kind::Piranha { .. }
            | Kind::Muncher
            | Kind::HammerBro
            | Kind::Magikoopa
            | Kind::RockyWrench
            | Kind::Wiggler
            | Kind::ShoeGoomba { .. }
            | Kind::MontyMole
            | Kind::ChainChomp { .. }
            | Kind::CheepCheep { .. }
            | Kind::FishBone
            | Kind::BowserJr { .. }
            | Kind::Bowser { .. }
            | Kind::Koopaling { .. }
            | Kind::Pokey { .. }
            | Kind::Porcupuffer
            | Kind::Bully
            | Kind::Skipsqueak { .. }
            | Kind::Spike => false,
            // Bob-ombs, Spinies, Buzzy Beetles, Spike Tops, Dry Bones, Ant Troopers, items …
            _ => self.wings,
        }
    }

    /// A rider that keeps its own action while stacked (a Piranha Plant bites, a Hammer Bro
    /// throws); the others only idle [obs: these have a tower action of their own; what each
    /// one does is inferred from its normal behaviour].
    pub fn acts_in_tower(&self) -> bool {
        matches!(
            self.kind,
            Kind::Piranha { .. }
                | Kind::HammerBro
                | Kind::BobOmb
                | Kind::Magikoopa
                | Kind::RockyWrench
                | Kind::Wiggler
                | Kind::DryBones
                | Kind::Boo
                | Kind::Blooper
                | Kind::Buzzy
                | Kind::SpikeTop { .. }
                | Kind::Spiny
                | Kind::ShoeGoomba { .. }
                | Kind::BoomBoom { .. }
                | Kind::BowserJr { .. }
                | Kind::Bowser { .. }
        )
    }

    /// Still part of its column. The manager drops a member that is gone, dying or without
    /// collision, out of its tower id, in the Dokan, Carried, Killer, Block or DrawDokan context
    /// (contexts 5, 7, 8, 9, 15), or whose part no longer stacks (its profile's tower bit) [obs
    /// v3.0.3 0x7101285500]: so a stomped Koopa or Buzzy Beetle leaves (its shell is a new
    /// object in the game), as do held, fired and defeated members.
    fn stays_in_tower(&self) -> bool {
        self.mode == Mode::Normal && !self.held
    }
}

impl World {
    /// The column each spawn record belongs to, if any (indexed by spawn record).
    pub(super) fn tower_member_columns(&self) -> Vec<Option<usize>> {
        let mut out = vec![None; self.spawns.len()];
        for (c, col) in self.level.towers.iter().enumerate() {
            for &s in &col.members {
                if let Some(o) = out.get_mut(s) {
                    *o = Some(c);
                }
            }
        }
        out
    }

    /// Build the runtime towers of the columns that just had a member spawned.
    pub(super) fn form_towers(&mut self, spawned: &[usize]) {
        for c in 0..self.level.towers.len() {
            let col = &self.level.towers[c];
            if !col.members.iter().any(|s| spawned.contains(s)) {
                continue;
            }
            let ids: Vec<u32> = col
                .members
                .iter()
                .filter_map(|&s| {
                    self.actors
                        .iter()
                        .find(|a| a.spawn == Some(s) && a.stays_in_tower())
                        .map(|a| a.id)
                })
                .collect();
            let fixed = col.fixed;
            // a member already in another column leaves it
            for t in &mut self.towers {
                t.members.retain(|m| !ids.contains(&m.id));
            }
            let members: Vec<Member> = ids
                .iter()
                .take(MAX_MEMBERS)
                .filter_map(|&id| self.actors.iter().find(|a| a.id == id))
                .map(Member::new)
                .collect();
            if members.len() + fixed.is_some() as usize >= 2 {
                for m in &members {
                    if let Some(a) = self.actors.iter_mut().find(|a| a.id == m.id) {
                        // a random sway amplitude in [0.5, 1.5) of either sign and a phase in
                        // [0°, 180°) [obs]
                        let r = a.rng;
                        let amp = 0.5 + ((r >> 8) & 0xff) as f32 / 256.0;
                        a.sway_amp = if r & 1 != 0 { -amp } else { amp };
                        a.sway_phase = (r >> 16) % 180;
                    }
                }
                self.towers.push(Tower {
                    fixed,
                    base: None,
                    members,
                    clock: TURN_STEP,
                });
            }
        }
        self.mark_riders();
    }

    /// Flag the riders (every member but a free base), so the actor step skips their own motion.
    fn mark_riders(&mut self) {
        for a in &mut self.actors {
            a.stacked = false;
        }
        for t in &self.towers {
            let first = if t.fixed.is_some() { 0 } else { 1 };
            for m in t.members.iter().skip(first) {
                if let Some(a) = self.actors.iter_mut().find(|a| a.id == m.id) {
                    a.stacked = true;
                }
            }
        }
    }

    /// One frame of every tower, after the actors stepped (the base has moved).
    pub(super) fn step_towers(&mut self) {
        let mut towers = std::mem::take(&mut self.towers);
        let mut born: Vec<Tower> = Vec::new();
        for t in &mut towers {
            self.step_tower(t, &mut born);
        }
        towers.extend(born);
        towers.retain(|t| t.members.len() + t.fixed.is_some() as usize >= 2);
        self.towers = towers;
        self.mark_riders();
        self.sway_riders();
    }

    fn actor_index(&self, id: u32) -> Option<usize> {
        self.actors.iter().position(|a| a.id == id)
    }

    fn step_tower(&mut self, t: &mut Tower, born: &mut Vec<Tower>) {
        // members that dropped out: the one right above a flier breaks away
        let mut breaks: Vec<u32> = Vec::new();
        let mut k = 0;
        while k < t.members.len() {
            let gone = self
                .actor_index(t.members[k].id)
                .is_none_or(|i| !self.actors[i].stays_in_tower());
            if gone {
                if let Some(up) = t.members.get(k + 1)
                    && let Some(i) = self.actor_index(up.id)
                    && self.actors[i].flies_in_tower()
                {
                    breaks.push(up.id);
                }
                t.members.remove(k);
            } else {
                k += 1;
            }
        }
        if t.members.is_empty() {
            return;
        }
        // the bottom member: free unless it stands on a fixed base
        let mut start = 0;
        let (mut below_x, mut below_top, mut below_dx, mut below_hw, base_dy);
        match t.fixed {
            Some((x, top)) => {
                below_x = x;
                below_top = top;
                below_dx = 0.0;
                below_hw = 8.0;
                base_dy = 0.0;
            }
            None => {
                let Some(i) = self.actor_index(t.members[0].id) else {
                    return;
                };
                if t.base != Some(t.members[0].id) {
                    // a rider that became the base falls on its own from here
                    if t.base.is_some() {
                        self.actors[i].vy = t.members[0].vy;
                    }
                    t.base = Some(t.members[0].id);
                }
                let b = &self.actors[i];
                let m = &mut t.members[0];
                m.dir = b.dir;
                below_dx = b.x - m.last.0;
                base_dy = b.y - m.last.1;
                m.last = (b.x, b.y);
                below_x = b.x;
                below_top = b.y + b.tower_height();
                below_hw = b.half_w();
                start = 1;
            }
        }
        let mut k = start;
        while k < t.members.len() {
            let m = t.members[k];
            let Some(i) = self.actor_index(m.id) else {
                k += 1;
                continue;
            };
            let (x, y, hw) = (self.actors[i].x, self.actors[i].y, self.actors[i].half_w());
            let offset = below_x - (x + below_dx * CARRY);
            let off_body = offset.abs() - below_hw - hw + 5.0 > 0.0;
            let off_gap = (y - below_top).abs() > GAP;
            let grounded = (k > 0 || t.fixed.is_none()) && self.rider_grounded(i);
            if breaks.contains(&m.id) || off_body || off_gap || grounded {
                // this member and the ones above become a tower of their own; it faces away
                // from the one it left
                let dir = if breaks.contains(&m.id) || grounded {
                    self.actors[i].dir
                } else if offset >= 0.0 {
                    -1
                } else {
                    1
                };
                let mut rest: Vec<Member> = t.members.split_off(k);
                rest[0].dir = dir;
                let a = &mut self.actors[i];
                a.dir = dir;
                a.vy = rest[0].vy;
                a.vx = 0.0;
                a.stacked = false;
                rest[0].last = (a.x, a.y);
                if rest.len() >= 2 {
                    born.push(Tower {
                        fixed: None,
                        base: Some(rest[0].id),
                        members: rest,
                        clock: TURN_STEP,
                    });
                }
                break;
            }
            // sideways: pulled toward the lower member, the pull's speed decaying toward 0
            let pull = if offset > PULL {
                PULL
            } else if offset < -PULL {
                -PULL
            } else {
                0.0
            };
            let mut v = m.rel_vx + pull;
            v = if v > 0.0 {
                (v - DECAY).max(0.0)
            } else {
                (v + DECAY).min(0.0)
            };
            // a wall the rider touches cuts its own speed into it to the pull (the carry still
            // moves it) [obs v3.0.3 0x7101299dd0: the rider's BG wall flags +0x659 bits 4 / 5];
            // its terrain check then holds it out of the wall [inf: taken as flush with the cell]
            let a = &self.actors[i];
            let mid = a.y + a.height() / 2.0;
            let solid = |px: f32| self.level.cell_at(px, mid).is_solid();
            if !(pull >= 0.0 && v > 0.0) {
                if pull <= 0.0 && v < 0.0 && solid(x - hw - WALL_TOUCH) {
                    v = v.max(pull);
                }
            } else if solid(x + hw + WALL_TOUCH) {
                v = v.min(pull);
            }
            let mut nx = v + below_dx * CARRY + x;
            if nx > x && solid(nx + hw) {
                nx = ((nx + hw) / TILE).floor() * TILE - hw;
            } else if nx < x && solid(nx - hw) {
                nx = (((nx - hw) / TILE).floor() + 1.0) * TILE + hw;
            }
            // down: gravity plus the base's vertical movement, landing on the lower member's top
            let mut vy = (m.vy - GRAVITY).max(-MAX_FALL);
            let mut ny = y + vy + base_dy;
            if ny <= below_top {
                ny = below_top;
                vy = 0.0;
            }
            let a = &mut self.actors[i];
            a.x = nx;
            a.y = ny;
            a.vx = 0.0;
            a.vy = 0.0;
            a.on_ground = vy == 0.0;
            a.dir = m.dir;
            let mm = &mut t.members[k];
            mm.rel_vx = v;
            mm.vy = vy;
            below_dx = nx - mm.last.0;
            mm.last = (nx, ny);
            below_x = nx;
            below_top = ny + a.tower_height();
            below_hw = hw;
            k += 1;
        }
        // facing climbs the column one member every 4 frames
        if t.members.len() >= 2 {
            t.clock -= 1;
            if t.clock == 0 {
                t.clock = TURN_STEP;
                for k in (1..t.members.len()).rev() {
                    t.members[k].dir = t.members[k - 1].dir;
                }
            }
        }
    }

    /// A rider whose feet are on solid ground (not on the member below) leaves its column.
    fn rider_grounded(&self, i: usize) -> bool {
        let a = &self.actors[i];
        self.level.cell_at(a.x, a.y - 0.5).is_solid()
    }

    /// The riders' sway (drawn only): each eases toward amplitude · sin(phase) by at most 1 px a
    /// frame, the phase turning 5° a frame; off a tower it eases back to 0 [obs].
    fn sway_riders(&mut self) {
        for a in &mut self.actors {
            let target = if a.stacked {
                a.sway_phase = (a.sway_phase + 5) % 360;
                a.sway_amp * (a.sway_phase as f32).to_radians().dsin()
            } else {
                0.0
            };
            a.sway += (target - a.sway).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::World;
    use crate::actors::{Kind, Mode};
    use crate::input::Input;
    use crate::level::{Level, Spawn, TILE, TowerColumn};

    fn spawn(kind: Kind, x: f32, y: f32) -> Spawn {
        Spawn {
            kind,
            x,
            y,
            big: false,
            wings: false,
            pipe: None,
            parachute: false,
            rail: None,
            link: -1,
            clown: false,
            alt: false,
        }
    }

    /// A column of `kinds` (bottom first) at tile x 20 on flat ground (top at y 32), stacked a
    /// tile apart as placed.
    fn column(kinds: &[Kind]) -> World {
        let mut lvl = Level::flat(60);
        for (k, &kind) in kinds.iter().enumerate() {
            lvl.spawns
                .push(spawn(kind, 20.0 * TILE + 8.0, 32.0 + 16.0 * k as f32));
        }
        let x = 20.0 * TILE + 8.0;
        lvl.towers = vec![TowerColumn {
            fixed: None,
            members: (0..kinds.len()).collect(),
            spawn_box: (x - 8.0, 32.0, x + 8.0, 32.0 + 16.0 * kinds.len() as f32),
        }];
        World::with_level(lvl)
    }

    fn run(w: &mut World, frames: u32) {
        for _ in 0..frames {
            w.step(Input::default());
        }
    }

    fn at(w: &World, s: usize) -> &crate::actors::Actor {
        w.actors.iter().find(|a| a.spawn == Some(s)).unwrap()
    }

    fn at_mut(w: &mut World, s: usize) -> &mut crate::actors::Actor {
        w.actors.iter_mut().find(|a| a.spawn == Some(s)).unwrap()
    }

    #[test]
    fn a_goomba_column_walks_as_one_and_leans() {
        let mut w = column(&[Kind::Goomba; 3]);
        run(&mut w, 2);
        let x0 = at(&w, 0).x;
        // the riders settle onto the tops below them: 15 px apart, not the 16 they were placed
        run(&mut w, 30);
        let (a, b, c) = (at(&w, 0), at(&w, 1), at(&w, 2));
        assert_eq!((b.y - a.y, c.y - b.y), (15.0, 15.0));
        assert!(b.stacked && c.stacked && !a.stacked);
        // walking, the riders lag behind and are pulled back: never exactly over the base, never
        // off it
        let mut lag = 0.0f32;
        for _ in 0..240 {
            run(&mut w, 1);
            let (a, b) = (at(&w, 0), at(&w, 1));
            lag = lag.max((b.x - a.x).abs());
            assert!((b.x - a.x).abs() < 11.0, "still on it");
        }
        assert!((at(&w, 0).x - x0).abs() > 50.0, "the base walked");
        assert!(lag > 0.2, "the column leans: {lag}");
        let (a, b, c) = (at(&w, 0), at(&w, 1), at(&w, 2));
        assert_eq!(
            (b.dir, c.dir),
            (a.dir, a.dir),
            "the riders face the base's way"
        );
        assert!(w.towers.len() == 1 && w.towers[0].members.len() == 3);
    }

    #[test]
    fn a_turn_climbs_the_column_four_frames_a_member() {
        let mut w = column(&[Kind::Goomba; 3]);
        run(&mut w, 20);
        let d = at(&w, 0).dir;
        at_mut(&mut w, 0).dir = -d;
        let mut turned = [None, None];
        for f in 1..=12 {
            run(&mut w, 1);
            for (k, t) in turned.iter_mut().enumerate() {
                if t.is_none() && at(&w, k + 1).dir == -d {
                    *t = Some(f);
                }
            }
        }
        let (t1, t2) = (turned[0].unwrap(), turned[1].unwrap());
        // the base's facing is handed up when the 4-frame clock ticks; a rider shows it on its
        // next placement
        assert!(t1 <= 5, "the first rider turns within 5 frames: {t1}");
        assert_eq!(t2 - t1, 4, "the next one 4 frames later");
    }

    #[test]
    fn knocking_out_the_middle_drops_the_top_onto_the_bottom() {
        let mut w = column(&[Kind::Goomba; 3]);
        run(&mut w, 30);
        at_mut(&mut w, 1).mode = Mode::DieFall;
        run(&mut w, 1);
        // it falls (it isn't moved down at once) …
        let gap = at(&w, 2).y - at(&w, 0).y;
        assert!(gap > 15.0 && at(&w, 2).stacked, "falling: {gap}");
        // … and lands on the bottom one's top
        run(&mut w, 30);
        assert_eq!(at(&w, 2).y - at(&w, 0).y, 15.0);
        assert!(at(&w, 2).stacked);
    }

    #[test]
    fn with_the_base_gone_the_next_one_walks() {
        let mut w = column(&[Kind::Goomba; 3]);
        run(&mut w, 30);
        let x1 = at(&w, 1).x;
        at_mut(&mut w, 0).mode = Mode::DieFall;
        run(&mut w, 120);
        let (b, c) = (at(&w, 1), at(&w, 2));
        assert!(!b.stacked && (b.x - x1).abs() > 20.0, "the new base walks");
        assert!(b.on_ground && b.y == 32.0, "and has dropped to the ground");
        assert!(c.stacked && c.y - b.y == 15.0, "the top one rides it");
    }

    #[test]
    fn a_flier_above_a_lost_member_breaks_away() {
        // Goomba, Goomba, Boo: the Boo flies, so it doesn't drop onto the bottom Goomba
        let mut w = column(&[Kind::Goomba, Kind::Goomba, Kind::Boo]);
        run(&mut w, 30);
        assert!(at(&w, 2).stacked);
        at_mut(&mut w, 1).mode = Mode::DieFall;
        run(&mut w, 2);
        assert!(!at(&w, 2).stacked, "the Boo is on its own");
        assert!(w.towers.is_empty(), "the bottom Goomba alone is no tower");
        // a Goomba in its place stays and falls onto the bottom one
        let mut w = column(&[Kind::Goomba; 3]);
        run(&mut w, 30);
        at_mut(&mut w, 1).mode = Mode::DieFall;
        run(&mut w, 2);
        assert!(at(&w, 2).stacked);
    }

    #[test]
    fn a_rider_held_back_by_a_wall_starts_its_own_column() {
        // the base walks left under a wall that the riders run into
        let mut w = column(&[Kind::Goomba; 3]);
        for y in 3..6 {
            w.level.set_solid(18, y);
        }
        let mut split = None;
        for f in 0..200 {
            run(&mut w, 1);
            if split.is_none() && !at(&w, 1).stacked {
                split = Some(f);
            }
        }
        assert!(split.is_some(), "the middle one left the base");
        let (a, b, c) = (at(&w, 0), at(&w, 1), at(&w, 2));
        assert!(a.x < 18.0 * TILE, "the base walked on under the wall");
        assert!(
            !a.stacked && !b.stacked && c.stacked,
            "the top one still rides the middle one"
        );
        assert_eq!(w.towers.len(), 1);
        assert_eq!(w.towers[0].members[0].id, b.id);
        assert_eq!(b.y, 32.0, "which has dropped to the ground");
    }

    #[test]
    fn riders_on_a_bill_blaster_stay_put() {
        let mut lvl = Level::flat(60);
        let x = 20.0 * TILE + 8.0;
        lvl.spawns.push(spawn(Kind::Goomba, x, 32.0 + 32.0));
        lvl.towers = vec![TowerColumn {
            fixed: Some((x, 32.0 + 30.0)),
            members: vec![0],
            spawn_box: (x - 8.0, 32.0, x + 8.0, 32.0 + 48.0),
        }];
        // the blaster itself: two solid tiles
        lvl.set_solid(20, 2);
        lvl.set_solid(20, 3);
        let mut w = World::with_level(lvl);
        run(&mut w, 200);
        let g = at(&w, 0);
        assert!(g.stacked);
        assert_eq!((g.x, g.y), (x, 62.0), "it waits on the blaster's top");
    }

    #[test]
    fn a_stacked_hammer_bro_keeps_throwing() {
        let mut w = column(&[Kind::Goomba, Kind::HammerBro]);
        let mut threw = false;
        for _ in 0..400 {
            run(&mut w, 1);
            threw |= w.actors.iter().any(|a| a.kind == Kind::Hammer);
            let (a, b) = (at(&w, 0), at(&w, 1));
            assert!(
                b.stacked && (b.y - a.y - 15.0).abs() < 1.5,
                "it stays on the Goomba"
            );
        }
        assert!(threw);
    }

    #[test]
    fn a_column_spawns_only_through_its_manager_box() {
        // the members stand in view, but the column's manager box is far off: nothing yet
        let mut lvl = Level::flat(400);
        let x = 20.0 * TILE + 8.0;
        lvl.spawns.push(spawn(Kind::Goomba, x, 32.0));
        lvl.spawns.push(spawn(Kind::Goomba, x, 48.0));
        lvl.towers = vec![TowerColumn {
            fixed: None,
            members: vec![0, 1],
            spawn_box: (5000.0, 32.0, 5016.0, 64.0),
        }];
        let mut w = World::with_level(lvl);
        run(&mut w, 2);
        assert!(w.actors.iter().all(|a| a.spawn.is_none_or(|s| s > 1)));
        // with the box in view both come out in the same frame, stacked
        let mut w = column(&[Kind::Goomba, Kind::Goomba]);
        run(&mut w, 1);
        assert!(at(&w, 0).age == at(&w, 1).age && at(&w, 1).stacked);
    }

    #[test]
    fn a_stomped_koopa_leaves_the_column_as_its_shell() {
        let mut w = column(&[Kind::Goomba, Kind::Koopa { red: false }, Kind::Goomba]);
        run(&mut w, 30);
        assert!(at_mut(&mut w, 1).stomp());
        run(&mut w, 30);
        let (a, b, c) = (at(&w, 0), at(&w, 1), at(&w, 2));
        assert!(!b.stacked, "the shell is out of the column");
        assert!(
            c.stacked && (c.y - a.y - 15.0).abs() < 1.5,
            "the top one fell onto the bottom"
        );
    }

    #[test]
    fn a_rider_against_a_wall_keeps_only_the_pull_into_it() {
        let mut w = column(&[Kind::Muncher, Kind::Goomba]);
        run(&mut w, 2);
        // a wall right of the rider, touching it, and a fast relative speed into it
        let x = at(&w, 1).x;
        let tx = ((x + 8.0) / TILE).floor() as i32;
        w.level.set_solid(tx, 3);
        let id = at(&w, 1).id;
        let m = w.towers[0].members.iter_mut().find(|m| m.id == id).unwrap();
        m.rel_vx = 1.0;
        run(&mut w, 1);
        let m = w.towers[0].members.iter().find(|m| m.id == id).unwrap();
        assert!(m.rel_vx <= super::PULL, "{}", m.rel_vx);
        assert!(at(&w, 1).x + 8.0 <= tx as f32 * TILE + 1e-3);
    }

    #[test]
    fn a_column_holds_at_most_128_members() {
        let mut lvl = Level::flat(60);
        lvl.top = 200.0 * TILE;
        for k in 0..130 {
            lvl.spawns.push(spawn(
                Kind::Muncher,
                20.0 * TILE + 8.0,
                32.0 + 16.0 * k as f32,
            ));
        }
        let x = 20.0 * TILE + 8.0;
        lvl.towers = vec![TowerColumn {
            fixed: None,
            members: (0..130).collect(),
            spawn_box: (x - 8.0, 32.0, x + 8.0, 32.0 + 16.0 * 130.0),
        }];
        let mut w = World::with_level(lvl);
        run(&mut w, 1);
        assert_eq!(w.towers[0].members.len(), super::MAX_MEMBERS);
    }
}
