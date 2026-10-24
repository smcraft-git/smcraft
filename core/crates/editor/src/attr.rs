//! A placed part's attributes: wings, a parachute, the big size and its alternate form — what
//! the maker's attribute balloon offers for a part and applying a choice.
//!
//! The game has no Wing or Parachute part: a part held still for a moment opens a balloon of
//! its attributes, and wings and parachutes are put on and taken off there (research/specs/
//! maker.md C10, private). The rules here follow what the game checks: wings only on parts that
//! can wear them, a parachute only on parts that can take one and that don't ride a Koopa Clown
//! Car, a track or a Lakitu's Cloud and aren't stacked, and neither when the change would pass
//! a part limit (a winged block counts with the enemies, not the blocks).

use crate::rules;
use crate::{EditError, Editor, parts};

/// Wings (course object flag).
pub const WINGS: u32 = 0x2;
/// The alternate form.
pub const ALT: u32 = 0x4;
/// A parachute.
pub const PARACHUTE: u32 = 0x8000;
/// What keeps a part from taking a parachute: riding a Koopa Clown Car (0x200) or a track
/// (0x400), being stacked (0x1000), riding a Lakitu's Cloud (0x10000 / 0x20000) — the mask
/// the game tests before it puts one on.
pub const NO_PARACHUTE: u32 = 0x200 | 0x400 | 0x1000 | 0x1_0000 | 0x2_0000;

/// One choice of the balloon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attr {
    /// The part's own form (`false`) or its alternate form (`true`).
    Form(bool),
    /// Make it big (`true`) or small again (`false`).
    Big(bool),
    /// Put wings on (`true`) or take them off (`false`).
    Wings(bool),
    /// Put a parachute on (`true`) or take it off (`false`).
    Parachute(bool),
}

impl Attr {
    /// The flags a part with `flags` has once this choice is made.
    pub fn apply_to(self, flags: u32) -> u32 {
        match self {
            Attr::Form(alt) => set(flags, ALT, alt),
            Attr::Big(big) => (flags & !0x6000) | if big { 0x4000 } else { 0 },
            Attr::Wings(on) => set(flags, WINGS, on),
            Attr::Parachute(on) => set(flags, PARACHUTE, on),
        }
    }

    /// The part's edit sound key for the change (its `<style>_Edit<Part>` user).
    pub fn sound(self) -> &'static str {
        match self {
            Attr::Form(_) => "p_AttributeChange",
            Attr::Big(_) => "scaleChange",
            Attr::Wings(true) => "voice_getWing",
            Attr::Wings(false) => "lose_wing",
            Attr::Parachute(true) => "p_EquipPara",
            Attr::Parachute(false) => "p_RemovePara",
        }
    }
}

fn set(flags: u32, bit: u32, on: bool) -> u32 {
    if on { flags | bit } else { flags & !bit }
}

/// A button of the balloon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrOption {
    pub attr: Attr,
    /// The part is like this now (the form buttons: the current one is checked).
    pub checked: bool,
    /// The change can be made (false: shown greyed, a part limit would be passed).
    pub enabled: bool,
}

impl Editor {
    /// What the attribute balloon offers for part `index`, in the game's order: its forms,
    /// then big / small, wings, parachute. Empty when the part has none (no balloon).
    pub fn attr_options(&self, index: usize) -> Vec<AttrOption> {
        let Some(o) = self.object(index) else {
            return Vec::new();
        };
        let caps = parts::caps(o.id);
        let mut out = Vec::new();
        let mut offer = |attr: Attr, checked: bool| {
            let enabled = checked || self.attr_allowed(index, attr).is_ok();
            out.push(AttrOption {
                attr,
                checked,
                enabled,
            });
        };
        if parts::forms(o.id) >= 2 {
            let alt = o.flags & ALT != 0;
            offer(Attr::Form(false), !alt);
            offer(Attr::Form(true), alt);
        }
        if caps.big {
            offer(Attr::Big(!o.is_big()), false);
        }
        if caps.wings {
            offer(Attr::Wings(o.flags & WINGS == 0), false);
        }
        if caps.parachute {
            offer(Attr::Parachute(o.flags & PARACHUTE == 0), false);
        }
        out
    }

    /// Whether `attr` can be applied to part `index` now (the reason when not).
    pub fn attr_allowed(&self, index: usize, attr: Attr) -> Result<(), EditError> {
        let Some(o) = self.object(index) else {
            return Err(EditError::Missing);
        };
        let caps = parts::caps(o.id);
        match attr {
            Attr::Form(_) if parts::forms(o.id) < 2 => {
                return Err(EditError::NotAllowed("this part has one form"));
            }
            Attr::Big(_) if !caps.big => {
                return Err(EditError::NotAllowed("can't be made big"));
            }
            Attr::Wings(true) if !caps.wings => {
                return Err(EditError::NotAllowed("can't wear wings"));
            }
            Attr::Parachute(true) if !caps.parachute => {
                return Err(EditError::NotAllowed("can't take a parachute"));
            }
            Attr::Parachute(_) if o.flags & NO_PARACHUTE != 0 => {
                return Err(EditError::NotAllowed(
                    "no parachute on a part that rides something or is stacked",
                ));
            }
            _ => {}
        }
        if let Attr::Big(_) = attr {
            // the size change is checked where it is made (`toggle_big`)
            return Ok(());
        }
        let mut n = o.clone();
        n.flags = attr.apply_to(o.flags);
        if rules::counter(&n) != rules::counter(o) {
            self.check_limits(&n, 1)?;
        }
        Ok(())
    }

    /// Apply `attr` to part `index`. Ok(true) when the part changed, Ok(false) when it was
    /// already like that.
    pub fn apply_attr(&mut self, index: usize, attr: Attr) -> Result<bool, EditError> {
        self.attr_allowed(index, attr)?;
        let o = self.object(index).ok_or(EditError::Missing)?;
        let flags = attr.apply_to(o.flags);
        if flags == o.flags {
            return Ok(false);
        }
        if let Attr::Big(_) = attr {
            self.toggle_big(index)?;
            return Ok(true);
        }
        self.set_flags(index, flags);
        Ok(true)
    }

    /// Put wings on part `index` or take them off.
    pub fn set_wings(&mut self, index: usize, on: bool) -> Result<bool, EditError> {
        self.apply_attr(index, Attr::Wings(on))
    }

    /// Put a parachute on part `index` or take it off.
    pub fn set_parachute(&mut self, index: usize, on: bool) -> Result<bool, EditError> {
        self.apply_attr(index, Attr::Parachute(on))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::flat;

    #[test]
    fn wings_and_parachutes_go_on_and_off() {
        let mut e = flat("M1");
        let g = e.place(0, 10, 2).unwrap();
        assert_eq!(e.set_wings(g, true), Ok(true));
        assert_eq!(e.object(g).unwrap().flags & WINGS, WINGS);
        assert_eq!(e.set_wings(g, true), Ok(false), "already winged");
        assert_eq!(e.set_parachute(g, true), Ok(true));
        let f = e.object(g).unwrap().flags;
        assert_eq!(f & (WINGS | PARACHUTE), WINGS | PARACHUTE);
        assert_eq!(f & 0x0600_0000, 0x0600_0000, "the default bits stay");
        assert_eq!(e.set_wings(g, false), Ok(true));
        assert_eq!(e.set_parachute(g, false), Ok(true));
        assert_eq!(e.object(g).unwrap().flags, crate::DEFAULT_FLAGS);
    }

    #[test]
    fn only_parts_that_can_carry_them() {
        let mut e = flat("M1");
        // a Hard Block (6) can wear wings but takes no parachute; a pipe (9) neither
        let b = e.place(6, 10, 2).unwrap();
        assert_eq!(e.set_wings(b, true), Ok(true));
        assert!(matches!(
            e.set_parachute(b, true),
            Err(EditError::NotAllowed(_))
        ));
        let p = e.place(9, 20, 2).unwrap();
        assert!(matches!(
            e.set_wings(p, true),
            Err(EditError::NotAllowed(_))
        ));
        assert!(
            e.attr_options(p)
                .iter()
                .all(|o| !matches!(o.attr, Attr::Wings(_) | Attr::Parachute(_))),
            "a pipe's balloon has no wings or parachute"
        );
        // taking off what isn't there is no change, and allowed
        assert_eq!(e.set_wings(p, false), Ok(false));
    }

    #[test]
    fn no_parachute_on_a_rider_or_a_stack() {
        let mut e = flat("M1");
        let g = e.place(0, 10, 2).unwrap();
        for bit in [0x200, 0x400, 0x1000, 0x1_0000, 0x2_0000] {
            e.set_flags(g, crate::DEFAULT_FLAGS | bit);
            assert!(
                matches!(e.set_parachute(g, true), Err(EditError::NotAllowed(_))),
                "bit {bit:#x}"
            );
            assert!(e.set_wings(g, true).is_ok(), "wings still go on ({bit:#x})");
            e.set_wings(g, false).unwrap();
        }
    }

    #[test]
    fn the_balloon_offers_the_parts_attributes_in_order() {
        let mut e = flat("M1");
        let k = e.place(1, 10, 2).unwrap(); // Koopa: two forms, big, wings, parachute
        let a: Vec<Attr> = e.attr_options(k).iter().map(|o| o.attr).collect();
        assert_eq!(
            a,
            [
                Attr::Form(false),
                Attr::Form(true),
                Attr::Big(true),
                Attr::Wings(true),
                Attr::Parachute(true)
            ]
        );
        assert!(e.attr_options(k)[0].checked, "the own form is current");
        e.set_wings(k, true).unwrap();
        e.set_parachute(k, true).unwrap();
        let a: Vec<Attr> = e.attr_options(k).iter().map(|o| o.attr).collect();
        assert!(a.contains(&Attr::Wings(false)) && a.contains(&Attr::Parachute(false)));
        // a 1-Up Mushroom (33) wears wings and a parachute but has one form and no big size
        let c = e.place(33, 20, 2).unwrap();
        let a: Vec<Attr> = e.attr_options(c).iter().map(|o| o.attr).collect();
        assert_eq!(a, [Attr::Wings(true), Attr::Parachute(true)]);
    }

    #[test]
    fn a_full_counter_greys_the_wings_of_a_block() {
        let mut e = flat("M1");
        for i in 0..100 {
            e.place(0, 30 + i, 2).unwrap();
        }
        // a plain block counts with the blocks; with wings it would be the 101st enemy
        let b = e.place(4, 150, 6).unwrap();
        assert_eq!(
            e.set_wings(b, true),
            Err(EditError::Limit("enemies and gizmos"))
        );
        let w = e
            .attr_options(b)
            .into_iter()
            .find(|o| o.attr == Attr::Wings(true))
            .unwrap();
        assert!(!w.enabled);
        // an enemy stays in its counter: its wings still go on
        let g = e.object_at(30, 2).unwrap();
        assert_eq!(e.set_wings(g, true), Ok(true));
    }

    #[test]
    fn big_and_forms_through_the_balloon() {
        let mut e = flat("M1");
        let k = e.place(1, 10, 2).unwrap();
        assert_eq!(e.apply_attr(k, Attr::Form(true)), Ok(true));
        assert_eq!(e.object(k).unwrap().flags & ALT, ALT);
        assert_eq!(e.apply_attr(k, Attr::Big(true)), Ok(true));
        let k = e.object_at(10, 2).unwrap();
        assert!(e.object(k).unwrap().is_big());
        assert_eq!(e.apply_attr(k, Attr::Big(false)), Ok(true));
    }
}
