//! Stacking (maker spec C11, the course sample): parts that can stack (`parts::caps(id).stack`)
//! form a column; every member carries 0x1000, the bottom one too, each right on top of the
//! one below. Only parts that ride nothing and face the default way stack: the game's drop
//! handler (0x7100f5ad90, v3.0.3) takes a part above or below into the column only when its
//! flags masked with 0xE038601 are 0x6000000 — no pipe (0x1), Koopa Clown Car (0x200), track
//! (0x400), parachute (0x8000), Lakitu's Cloud (0x30000) or claw (0x8000000), both facing bits
//! 25–26 at their default — and sets 0x1000 only on such parts [obs; corp: all 1819 stacked
//! parts of the sample pass that test].
//!
//! Erasing a member moves nothing: after an erase the game rebuilds the area's columns
//! (0x7100fb57f0 → 0x7100f5ad90, 0x7100f5c8a0): each column's bottom part is found by walking
//! down from part to part one tile (16 px) apart at the same x (0x7100f5ce60), the members are
//! gathered upward from it and lined up on its x (0x7100f5da00, x only), and 0x1000 is set on
//! a column of more than one part — so the parts above the gap stay where they are as a column
//! of their own, and nothing there clears the flag of a part left alone [obs v3.0.3].

use crate::parts::{self, default_size, footprint};
use crate::{EditError, Editor};

/// The stacked flag every member of a column carries.
pub const STACKED: u32 = 0x1000;
/// The bits the game tests before a part joins a column, and what they must be.
pub const STACK_MASK: u32 = 0x0E03_8601;
pub const STACK_OK: u32 = 0x0600_0000;

/// Whether a part with `flags` may be in a column (its own stacked bit aside).
pub fn may_stack(flags: u32) -> bool {
    flags & STACK_MASK == STACK_OK
}

impl Editor {
    /// The members of the column part `index` is in, from it upward (it first).
    pub fn column_from(&self, index: usize) -> Vec<usize> {
        let mut out = vec![index];
        let mut cur = index;
        while let Some(o) = self.object(cur) {
            let r = footprint(o);
            let above = self
                .area()
                .objects
                .items
                .iter()
                .enumerate()
                .find(|(i, a)| {
                    !out.contains(i)
                        && a.flags & STACKED != 0
                        && a.x == o.x
                        && footprint(a).y == r.y + r.h
                })
                .map(|(i, _)| i);
            match above {
                Some(i) => {
                    out.push(i);
                    cur = i;
                }
                None => break,
            }
        }
        out
    }

    /// Put a new `part` on top of the column part `index` is in (both must stack): it and the
    /// column's members carry the stacked flag. Ok(the new part's index).
    pub fn stack_onto(&mut self, index: usize, part: i16, flags: u32) -> Result<usize, EditError> {
        let o = self.object(index).ok_or(EditError::Missing)?;
        if !parts::caps(o.id).stack || !parts::caps(part).stack {
            return Err(EditError::NotAllowed("these parts don't stack"));
        }
        let column = self.column_from(index);
        let riding = column
            .iter()
            .filter_map(|&i| self.object(i))
            .any(|m| !may_stack(m.flags));
        if riding || !may_stack(flags) {
            return Err(EditError::NotAllowed(
                "a part that rides something or has a parachute doesn't stack",
            ));
        }
        let top = *column.last().unwrap_or(&index);
        let r = footprint(self.object(top).ok_or(EditError::Missing)?);
        let (w, h) = default_size(part);
        // centred on the one below (the sample's members share their x)
        let tx = r.x + (r.w - w as i32) / 2;
        let new = self.place_sized(part, tx, r.y + r.h, w, h, flags | STACKED)?;
        for i in column {
            if let Some(m) = self.object(i).map(|m| m.flags) {
                self.set_flags(i, m | STACKED);
            }
        }
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::flat;

    #[test]
    fn goombas_stack_into_a_column() {
        let mut e = flat("M1");
        let g = e.place(0, 10, 2).unwrap();
        let a = e.stack_onto(g, 0, crate::DEFAULT_FLAGS).unwrap();
        let b = e.stack_onto(g, 1, crate::DEFAULT_FLAGS).unwrap();
        assert_eq!(e.column_from(g), [g, a, b], "a Koopa goes on the top");
        for i in [g, a, b] {
            assert_ne!(e.object(i).unwrap().flags & STACKED, 0);
        }
        let (rg, rb) = (
            footprint(e.object(g).unwrap()),
            footprint(e.object(b).unwrap()),
        );
        assert_eq!((rb.x, rb.y), (rg.x, rg.y + 2));
        // a pipe doesn't stack
        let p = e.place(9, 30, 2).unwrap();
        assert!(e.stack_onto(p, 0, crate::DEFAULT_FLAGS).is_err());
    }

    #[test]
    fn erasing_a_member_leaves_the_parts_above_where_they_are() {
        let mut e = flat("M1");
        let g = e.place(0, 10, 2).unwrap();
        e.stack_onto(g, 0, crate::DEFAULT_FLAGS).unwrap();
        e.stack_onto(g, 0, crate::DEFAULT_FLAGS).unwrap();
        e.stack_onto(g, 1, crate::DEFAULT_FLAGS).unwrap();
        assert_eq!(e.column_from(g).len(), 4);
        // erase the second one: the two above don't fall
        assert!(e.erase(10, 3));
        let at = |e: &Editor, y| e.object_at(10, y);
        assert!(at(&e, 3).is_none(), "the gap stays");
        let (bottom, upper) = (at(&e, 2).unwrap(), at(&e, 4).unwrap());
        assert_eq!(e.column_from(bottom), [bottom], "the bottom one is alone");
        assert_eq!(
            e.column_from(upper).len(),
            2,
            "the two above are a column of their own"
        );
        for i in [bottom, upper] {
            assert_ne!(e.object(i).unwrap().flags & STACKED, 0, "flags stay");
        }
        assert_eq!(
            e.object(at(&e, 5).unwrap()).unwrap().id,
            1,
            "the Koopa on top"
        );
    }

    #[test]
    fn riders_and_parachutes_stay_out_of_columns() {
        let mut e = flat("M1");
        let g = e.place(0, 10, 2).unwrap();
        // a parachuting Goomba doesn't go on top, nor onto a parachuting one
        assert!(matches!(
            e.stack_onto(g, 0, crate::DEFAULT_FLAGS | 0x8000),
            Err(EditError::NotAllowed(_))
        ));
        let p = e.place(0, 20, 2).unwrap();
        e.set_parachute(p, true).unwrap();
        assert!(e.stack_onto(p, 0, crate::DEFAULT_FLAGS).is_err());
        // a part facing another way than the default (bits 25–26) doesn't either
        assert!(
            e.stack_onto(g, 0, crate::DEFAULT_FLAGS & !0x0200_0000)
                .is_err()
        );
        assert!(e.stack_onto(g, 1, crate::DEFAULT_FLAGS).is_ok());
        assert!(may_stack(crate::DEFAULT_FLAGS | STACKED | 0x4));
    }
}
