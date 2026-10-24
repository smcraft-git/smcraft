//! Stacking (maker spec C11, the course sample): parts that can stack (`parts::caps(id).stack`)
//! form a column; every member carries 0x1000, the bottom one too, each right on top of the
//! one below.

use crate::parts::{self, default_size, footprint};
use crate::{EditError, Editor};

/// The stacked flag every member of a column carries.
pub const STACKED: u32 = 0x1000;

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
}
