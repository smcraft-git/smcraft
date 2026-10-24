//! Placed sound effects (research/specs/sound-effect-parts.md, private): an effect goes on an
//! empty tile (the area's spot records), on a part that can hold one (its `sound_effect_id`) or
//! on Mario at the start (the header's `player_sound_effect`). What each effect does in play is
//! `mmk_sim::otoasobi`'s; the ids the maker offers, their order and pages are
//! `mmk_formats::smm2::sound_effects`'.
//!
//! The game's rules [obs v3.0.3]:
//! - one effect per spot, per part and on Mario; a new one replaces the old;
//! - parts hold one if their editor record says so (`Mush/EditDB_common.byml`,
//!   `isCanContainOtoasobi?`: enemies, items, blocks, coins, pipes, doors, …; not terrain,
//!   semisolids, bridges, vines, conveyors, tracks, signs, saws, spike traps, the checkpoint
//!   flag, …), see [`can_hold`];
//! - up to 300 spots an area (the course writer's limit, 0x7100fde2c0);
//! - the course writer stores a spot as the tile under the effect's centre (pixel ÷ 16, rounded
//!   down), its course id and a zero byte.

use mmk_formats::record::Record;
use mmk_formats::smm2::sound_effects::valid;
use mmk_formats::smm2::{NO_SOUND_EFFECT, SoundEffect};

use crate::{EditError, Editor, parts};

/// Spots an area holds.
pub const SPOTS: usize = 300;

/// Parts that can hold an effect (`isCanContainOtoasobi?` true in their EditDB record) [obs
/// data]. (The clear pipe, 93, shares the pipe's sound user but its record says no.)
pub fn can_hold(part: i16) -> bool {
    matches!(
        part,
        0..=6
            | 8..=13
            | 15
            | 18..=21
            | 23
            | 25
            | 28..=36
            | 39..=42
            | 44..=48
            | 50..=52
            | 54..=58
            | 60..=63
            | 65
            | 70
            | 72
            | 74
            | 77
            | 78
            | 80
            | 84..=86
            | 91
            | 92
            | 95..=99
            | 102..=107
            | 111..=114
            | 116
            | 117
            | 119..=132
    )
}

/// Where an effect sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundTarget {
    /// An empty tile (index into the area's spot records).
    Spot(usize),
    /// A part (index into the area's objects).
    Part(usize),
    /// Mario at the start.
    Mario,
}

/// An effect in the current area, for drawing: its course id, where it sits and the tile its
/// icon goes on (a part's: its footprint's top-left tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedEffect {
    pub id: u8,
    pub target: SoundTarget,
    pub tile: (i32, i32),
}

impl Editor {
    /// Mario's tiles at the start: x 4, the start ground's top row and the one above
    /// (`Level::start` x = 72 px) [obs].
    pub fn mario_tiles(&self) -> [(i32, i32); 2] {
        let y = self.course.header.start_y as i32;
        [(4, y), (4, y + 1)]
    }

    fn on_mario(&self, tx: i32, ty: i32) -> bool {
        self.area == 0 && self.mario_tiles().contains(&(tx, ty))
    }

    fn spot_at(&self, tx: i32, ty: i32) -> Option<usize> {
        self.area()
            .sound_effects
            .iter()
            .position(|s| s.x as i32 == tx && s.y as i32 == ty)
    }

    /// The effect at a tile: on Mario, on the part there, else a spot.
    pub fn sound_effect_at(&self, tx: i32, ty: i32) -> Option<(u8, SoundTarget)> {
        if self.on_mario(tx, ty) {
            return self
                .course
                .header
                .player_effect()
                .map(|id| (id, SoundTarget::Mario));
        }
        if let Some(i) = self.object_at(tx, ty)
            && let Some(id) = self.area().objects.items[i].sound_effect()
        {
            return Some((id, SoundTarget::Part(i)));
        }
        let i = self.spot_at(tx, ty)?;
        Some((self.area().sound_effects.items[i].id, SoundTarget::Spot(i)))
    }

    /// Put effect `id` (a course id the maker offers) at a tile: on Mario, on the part there,
    /// else on the empty tile. Replaces the effect already there.
    pub fn put_sound_effect(&mut self, id: u8, tx: i32, ty: i32) -> Result<SoundTarget, EditError> {
        if !valid(id) {
            return Err(EditError::NotAllowed("no such sound effect"));
        }
        if self.on_mario(tx, ty) {
            self.course.header.player_sound_effect = id;
            return Ok(SoundTarget::Mario);
        }
        if let Some(i) = self.object_at(tx, ty) {
            let part = self.area().objects.items[i].id;
            if !can_hold(part) {
                return Err(EditError::NotAllowed("this part can't hold a sound effect"));
            }
            self.area_mut().objects.items[i].sound_effect_id = id as i16;
            return Ok(SoundTarget::Part(i));
        }
        if !self.in_bounds(tx, ty) || tx > 255 || ty > 255 {
            return Err(EditError::OutOfBounds);
        }
        if self.has_ground(tx, ty) {
            return Err(EditError::Occupied);
        }
        if let Some(i) = self.spot_at(tx, ty) {
            self.area_mut().sound_effects.items[i].id = id;
            return Ok(SoundTarget::Spot(i));
        }
        if self.area().sound_effects.len() >= SPOTS {
            return Err(EditError::Full);
        }
        let mut s = SoundEffect::parse(&[0u8; 4]);
        (s.id, s.x, s.y) = (id, tx as u8, ty as u8);
        let spots = &mut self.area_mut().sound_effects.items;
        spots.push(s);
        Ok(SoundTarget::Spot(spots.len() - 1))
    }

    /// Take the effect off a tile (Mario, the part there, else the spot). True if one went.
    pub fn erase_sound_effect(&mut self, tx: i32, ty: i32) -> bool {
        match self.sound_effect_at(tx, ty) {
            Some((_, SoundTarget::Mario)) => {
                self.course.header.player_sound_effect = NO_SOUND_EFFECT;
            }
            Some((_, SoundTarget::Part(i))) => {
                self.area_mut().objects.items[i].sound_effect_id = -1;
            }
            Some((_, SoundTarget::Spot(i))) => {
                self.area_mut().sound_effects.items.remove(i);
            }
            None => return false,
        }
        true
    }

    /// Take the spot off an empty tile (the eraser's last resort, [`Editor::erase`]).
    pub(crate) fn erase_spot(&mut self, tx: i32, ty: i32) -> bool {
        let Some(i) = self.spot_at(tx, ty) else {
            return false;
        };
        self.area_mut().sound_effects.items.remove(i);
        true
    }

    /// The current area's effects, for drawing.
    pub fn placed_effects(&self) -> Vec<PlacedEffect> {
        let mut out = Vec::new();
        if self.area == 0
            && let Some(id) = self.course.header.player_effect()
        {
            let [_, top] = self.mario_tiles();
            out.push(PlacedEffect {
                id,
                target: SoundTarget::Mario,
                tile: top,
            });
        }
        for (i, o) in self.area().objects.iter().enumerate() {
            if let Some(id) = o.sound_effect() {
                let r = parts::footprint(o);
                out.push(PlacedEffect {
                    id,
                    target: SoundTarget::Part(i),
                    tile: (r.x, r.y + r.h - 1),
                });
            }
        }
        for (i, s) in self.area().sound_effects.iter().enumerate() {
            out.push(PlacedEffect {
                id: s.id,
                target: SoundTarget::Spot(i),
                tile: (s.x as i32, s.y as i32),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{flat, round_trip};

    #[test]
    fn effects_go_on_spots_parts_and_mario_and_save() {
        let mut e = flat("M1");
        // a spot, a Goomba holding Applause, Mario with Boss Music
        assert_eq!(e.put_sound_effect(0, 20, 5), Ok(SoundTarget::Spot(0)));
        let g = e.place(0, 24, 2).unwrap();
        assert_eq!(e.put_sound_effect(3, 24, 2), Ok(SoundTarget::Part(g)));
        let [feet, _] = e.mario_tiles();
        assert_eq!(
            e.put_sound_effect(9, feet.0, feet.1),
            Ok(SoundTarget::Mario)
        );
        // replacing, not stacking
        assert_eq!(e.put_sound_effect(11, 20, 5), Ok(SoundTarget::Spot(0)));
        assert_eq!(e.area().sound_effects.len(), 1);
        // the rules: ground, a vine, an id the maker doesn't offer
        assert_eq!(e.put_sound_effect(0, 30, 0), Err(EditError::Occupied));
        let v = e.place(64, 40, 2).unwrap();
        assert!(e.put_sound_effect(0, 40, 2).is_err());
        assert_eq!(e.area().objects.items[v].sound_effect(), None);
        assert!(e.put_sound_effect(21, 21, 5).is_err());
        let back = round_trip(&e);
        assert_eq!(back.header.player_effect(), Some(9));
        assert_eq!(back.main.objects.items[g].sound_effect(), Some(3));
        let s = &back.main.sound_effects.items[0];
        assert_eq!((s.id, s.x, s.y), (11, 20, 5));
        let placed = e.placed_effects();
        assert_eq!(placed.len(), 3);
        assert!(placed.iter().any(|p| p.id == 3 && p.tile == (24, 2)));
        // erasing each
        assert!(e.erase_sound_effect(24, 2));
        assert!(e.erase_sound_effect(20, 5));
        assert!(e.erase_sound_effect(feet.0, feet.1 + 1));
        assert!(!e.erase_sound_effect(20, 5));
        let back = round_trip(&e);
        assert!(back.main.sound_effects.is_empty() && back.header.player_effect().is_none());
        assert_eq!(back.main.objects.items[g].sound_effect(), None);
    }

    #[test]
    fn an_area_holds_300_spots() {
        let mut e = flat("M1");
        for k in 0..SPOTS as i32 {
            e.put_sound_effect(2, 10 + k % 100, 3 + k / 100).unwrap();
        }
        assert_eq!(e.put_sound_effect(2, 9, 9), Err(EditError::Full));
        // the sub-area has its own
        e.set_area(1);
        assert_eq!(e.put_sound_effect(2, 5, 5), Ok(SoundTarget::Spot(0)));
        round_trip(&e);
    }

    #[test]
    fn every_offered_part_with_an_edit_record_follows_it() {
        // a few from each side of `isCanContainOtoasobi?`
        for p in [0, 4, 5, 8, 9, 20, 55, 92, 113, 132] {
            assert!(can_hold(p), "{p}");
        }
        for p in [7, 16, 17, 22, 43, 53, 59, 64, 66, 68, 79, 89, 90, 93, 110] {
            assert!(!can_hold(p), "{p}");
        }
    }
}
