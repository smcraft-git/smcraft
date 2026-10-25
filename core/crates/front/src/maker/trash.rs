//! The trash can `Edit_TrashCan_00` (the page's `L_TrashCan_00`, research/specs/maker.md C16,
//! private): a one-time animation part. Its controller is the game's plain `UIOneTimeAnimCtrl`
//! (vtable 0x71029cfab0) [obs v3.0.3]: set up with `Act` and an In / Out switch (0x7101b4d640),
//! started by its slot 0x180 (0x7101b4d930: `Act` from frame 0 at speed 1), and each frame,
//! while shown, it takes itself away once `Act` has ended (slot 0x150 0x7101b4d830 → slot 0x178
//! 0x7101b4d880). `Act` (45 frames: the lid jumps and bounces, the can dips — eating a part)
//! sounds `se_ui_edit_trash` and `SE_DropErase` through `Anm_Act_TrashCan` [obs data]; no
//! code asks for the parts' own `dropErase` key.
//!
//! What starts it in the Course Maker was not found [?]: no caller of the page's sub-part 8
//! (+0x800) or of the controller's slot 0x180 in the editor or UI code; the same layout is in
//! the Super World maker's page (0x71019d3bf0) and Story Mode's assist editor
//! (`Edit_SupportEdit_00`, `UIEditAssistPage`, at its +0x7d0), with no caller found either.
//! SMCraft has the controller ([`MakerFlow::trash_act`], capture `maker-trash`) but nothing in
//! the maker starts it.

use super::{MakerFlow, PAGE};

/// The trash can's `Act` length (frames) [obs data].
pub const ACT: i32 = 45;
/// Its `In` / `Out` length.
pub const IN_OUT: i32 = 5;

const PANE: &str = "L_TrashCan_00";

/// The trash can's state: shown since `since`, and taken away at `out`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrashCan {
    pub since: Option<i32>,
    out: Option<i32>,
}

impl TrashCan {
    pub fn shown(&self) -> bool {
        self.since.is_some()
    }
}

impl MakerFlow {
    /// The trash can at layout point `at` eats something: shown (`In`), `Act` with its sounds;
    /// it goes again once `Act` has ended (`Out`).
    pub fn trash_act(&mut self, at: [f32; 2]) {
        let tick = self.tick;
        self.trash = TrashCan {
            since: Some(tick),
            out: None,
        };
        let Some(p) = &mut self.page else { return };
        p.show(PANE, true);
        p.set_pane(
            &format!("{PANE}/N_TrashCan_00"),
            mmk_ui::PaneOverride {
                translate: Some(at),
                ..Default::default()
            },
        );
        p.stop(&format!("{PANE}/Out"));
        p.play(&format!("{PANE}/In"), tick);
        p.play(&format!("{PANE}/Act"), tick);
        self.ui.sound_key(PAGE, "Anm_Act_TrashCan", &[]);
    }

    /// Each tick: `Act` over → `Out`, then hidden.
    pub(super) fn trash_tick(&mut self) {
        let tick = self.tick;
        let Some(since) = self.trash.since else {
            return;
        };
        match self.trash.out {
            None if tick - since >= ACT => {
                self.trash.out = Some(tick);
                if let Some(p) = &mut self.page {
                    p.stop(&format!("{PANE}/In"));
                    p.play(&format!("{PANE}/Out"), tick);
                }
            }
            Some(out) if tick - out >= IN_OUT => {
                self.trash = TrashCan::default();
                if let Some(p) = &mut self.page {
                    p.show(PANE, false);
                }
            }
            _ => {}
        }
    }
}
