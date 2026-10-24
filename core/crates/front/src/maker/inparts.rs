//! The "what is inside" balloon over a container (`Edit_BalloonPartsIn_00`, for a big content
//! `Edit_BalloonPartsInBig_00`): the content's picture in a frame, with the layout's dummy
//! wings and parachute round it when the content carries them (research/specs/maker.md C10,
//! C16, private).
//!
//! What opens it is the hold that opens the attribute balloon: a placed part held still for
//! more than 15 frames (0x7100e90c70). When that part holds something (its child id isn't −1)
//! the contents balloon comes instead of the attribute one (0x7100e9e400, tried first) — for
//! player 1's cursor only, and not for a part riding a track (flag 0x400) [obs v3.0.3]. Moving
//! the part on carries it and closes the balloon (the carry's start clears the balloon's
//! target, 0x7100f9fe00) [obs]; letting go closes it too [inf: what clears the target then was
//! not found]. It has no buttons: the content is shown, not chosen.

use mmk_editor::attr::{PARACHUTE, WINGS};
use mmk_formats::smm2::course::CourseObject;

use super::MakerFlow;
use crate::layout::Layout;

pub const LAYOUT: &str = "Edit_BalloonPartsIn_00";
pub const LAYOUT_BIG: &str = "Edit_BalloonPartsInBig_00";
/// The balloon's side from its part's place on the screen (layout units, centred, y up): to the
/// left of a part right of x −350, below one above y 160 (0x71018c4450) [obs v3.0.3].
const SIDE_X: f32 = -350.0;
const SIDE_Y: f32 = 160.0;
/// A part on a track shows no contents balloon (0x7100e9e400).
const ON_TRACK: u32 = 0x400;

pub struct Contents {
    /// The container (object index).
    pub index: usize,
    /// What is inside and its flags.
    pub child: i16,
    pub child_flags: u32,
    pub layout: Option<Layout>,
}

impl Contents {
    pub fn big(&self) -> bool {
        self.child_flags & 0x6000 == 0x4000
    }

    /// The flags of the content's picture: its own, without the wings and parachute the
    /// layout's dummies show.
    pub fn picture_flags(&self) -> u32 {
        self.child_flags & !(WINGS | PARACHUTE)
    }
}

/// Whether holding `o` shows what is inside it instead of its attribute balloon.
pub fn shows_contents(o: &CourseObject) -> bool {
    o.child_id != -1 && o.flags & ON_TRACK == 0
}

impl MakerFlow {
    /// Open the contents balloon over container `index`. False when it holds nothing.
    pub(super) fn open_contents(&mut self, index: usize) -> bool {
        let Some(o) = self.editor.object(index).filter(|o| shows_contents(o)) else {
            return false;
        };
        let (child, child_flags) = (o.child_id, o.child_flags);
        let big = child_flags & 0x6000 == 0x4000;
        self.contents = Some(Contents {
            index,
            child,
            child_flags,
            layout: self.ui.layout(if big { LAYOUT_BIG } else { LAYOUT }),
        });
        self.layout_contents(true);
        true
    }

    /// The content shown (captures, tests).
    pub fn contents_shown(&self) -> Option<(i16, u32)> {
        self.contents.as_ref().map(|c| (c.child, c.child_flags))
    }

    pub(super) fn close_contents(&mut self) {
        self.contents = None;
    }

    /// Lay the balloon out over its container: the content's picture, the dummy wings and
    /// parachute shown when it has them, on the side the game picks.
    pub(super) fn layout_contents(&mut self, opening: bool) {
        let style = self.style();
        let tick = self.tick;
        let Some(c) = &self.contents else { return };
        let Some(o) = self.editor.object(c.index).cloned() else {
            self.contents = None;
            return;
        };
        let picture = self.icon_key_flags(&style, c.child, c.picture_flags());
        // from the container's top centre [inf: the anchor]
        let fp = mmk_editor::parts::footprint(&o);
        let at = self.to_layout(
            (fp.x as f32 + fp.w as f32 / 2.0) * 16.0,
            (fp.y + fp.h) as f32 * 16.0,
        );
        let Some(c) = &mut self.contents else { return };
        let flags = c.child_flags;
        let Some(l) = &mut c.layout else { return };
        l.set_translate("N_All_00", at);
        l.hold(
            "Direction",
            super::attr::direction(at, SIDE_X, SIDE_Y) as f32,
        );
        // frame 0 shows the dummies, frame 1 hides them [obs data]
        l.hold("Wing", if flags & WINGS != 0 { 0.0 } else { 1.0 });
        l.hold("Parachute", if flags & PARACHUTE != 0 { 0.0 } else { 1.0 });
        match &picture {
            Some(name) => {
                l.show("P_Icon_00", true);
                l.image("P_Icon_00", "PartIcon", name);
            }
            None => l.show("P_Icon_00", false),
        }
        if opening {
            l.play("In", tick);
        }
    }
}
